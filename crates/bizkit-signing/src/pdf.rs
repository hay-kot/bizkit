//! Reading uploaded PDFs, and the final PDF's signatures and audit pages.

use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::time::UnixMillis;
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};

use crate::fields::{FieldKind, FieldValue};
use crate::overlay::{self, Filled, Mark, Rect};
use crate::seal::{self, SealKey};
use crate::signature::{PAD_HEIGHT, PAD_WIDTH, Signature};

/// The time on an upload's dry-run seal; any time works.
const DRY_RUN_AT: UnixMillis = UnixMillis(1_795_000_000_000);

/// The most pages a request's PDF may have.
pub(crate) const MAX_PAGES: usize = 500;

const TOO_SLOW: &str = "took too long to check; try a smaller file";

/// Which queue a parse waits in and how long it may take, waiting included.
/// Parses run one at a time per class, because a small file can inflate a
/// lot; large uploads have their own class so a slow one never holds up
/// `pdf_base64` creates or page counts.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ParseClass {
    /// `pdf_base64` (at most 700 KiB) and page counts of stored files: 5 s.
    Small,
    /// Multipart uploads (at most 25 MiB): 20 s.
    Large,
}

static SMALL_PARSES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
static LARGE_PARSES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

impl ParseClass {
    const fn timeout(self) -> Duration {
        match self {
            Self::Small => Duration::from_secs(5),
            Self::Large => Duration::from_secs(20),
        }
    }

    const fn permits(self) -> &'static tokio::sync::Semaphore {
        match self {
            Self::Small => &SMALL_PARSES,
            Self::Large => &LARGE_PARSES,
        }
    }
}

/// Runs `parse` off the async runtime in `class`'s queue. Waiting for the
/// permit counts toward the timeout. The permit moves into the parse, so a
/// parse that outlives the timeout still holds it until it ends, and the
/// next file in that class waits rather than piling up threads.
async fn parse_in<T: Send + 'static>(
    class: ParseClass,
    parse: impl FnOnce() -> Result<T, &'static str> + Send + 'static,
) -> Result<T, &'static str> {
    parse_within(class.permits(), class.timeout(), parse).await
}

async fn parse_within<T: Send + 'static>(
    permits: &'static tokio::sync::Semaphore,
    timeout: Duration,
    parse: impl FnOnce() -> Result<T, &'static str> + Send + 'static,
) -> Result<T, &'static str> {
    let run = async {
        let Ok(permit) = permits.acquire().await else {
            return Err("must be a PDF that opens");
        };
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            parse()
        })
        .await
        .unwrap_or(Err("must be a PDF that opens"))
    };
    tokio::time::timeout(timeout, run).await.unwrap_or(Err(TOO_SLOW))
}

fn load(bytes: &[u8]) -> Result<Document, &'static str> {
    let doc = Document::load_mem(bytes).map_err(|_| "must be a PDF that opens")?;
    // lopdf decrypts files with an empty password as it loads them, and
    // the copy it saves would drop the protection.
    if doc.is_encrypted() || doc.encryption_state.is_some() {
        return Err("must not be encrypted or password-protected");
    }
    Ok(doc)
}

/// Checks that `bytes` is a PDF bizkit can add pages to and seal, and
/// returns its page count. The error is a message for the file's field.
pub(crate) async fn check(bytes: Vec<u8>, class: ParseClass) -> Result<u32, &'static str> {
    if !bytes.starts_with(b"%PDF-") {
        return Err("must be a PDF");
    }
    parse_in(class, move || {
        let doc = load(&bytes)?;
        let pages = doc.get_pages().len();
        if pages == 0 {
            return Err("must have at least one page");
        }
        if pages > MAX_PAGES {
            return Err("must have at most 500 pages");
        }
        // The editor places fields on every page, so each page's box must read.
        let unreadable = doc.get_pages().values().any(|id| overlay::page_box(&doc, *id).is_err());
        if unreadable {
            return Err("must have pages with readable sizes, turned by multiples of 90 degrees");
        }
        // Fill sample fields and seal a copy with a throwaway key now, so a
        // file that can't take them, the added page or the seal is refused
        // here instead of after everyone signed.
        let sealed = SealKey::generate("bizkit", DRY_RUN_AT)
            .and_then(|key| seal::seal(finish(doc, &Record::dry_run())?, &key, DRY_RUN_AT));
        if let Err(err) = sealed {
            tracing::info!(error = format!("{err:#}"), "an upload can't be sealed");
            return Err("must be a PDF bizkit can add pages to and seal");
        }
        u32::try_from(pages).map_err(|_| "must have at most 500 pages")
    })
    .await
}

/// The page count of a stored PDF that passed [`check`] when it was
/// uploaded, for requests from before page counts were stored. It only
/// loads the file: no page cap and no dry-run seal.
pub(crate) async fn page_count(bytes: Vec<u8>) -> Result<u32, &'static str> {
    parse_in(ParseClass::Small, move || {
        let pages = load(&bytes)?.get_pages().len();
        u32::try_from(pages).map_err(|_| "has too many pages")
    })
    .await
}

/// The sealed PDF's file name: `contract.pdf` becomes `contract-signed.pdf`.
pub(crate) fn signed_file_name(original: &str) -> String {
    let stem =
        original.strip_suffix(".pdf").or_else(|| original.strip_suffix(".PDF")).unwrap_or(original);
    format!("{stem}-signed.pdf")
}

/// The `WinAnsi` byte for `c`, the encoding of the standard fonts the pages use.
pub(crate) fn win_ansi(c: char) -> Option<u8> {
    let code = u32::from(c);
    if (0x20..=0x7e).contains(&code) || (0xa0..=0xff).contains(&code) {
        return u8::try_from(code).ok();
    }
    Some(match c {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8a,
        '‹' => 0x8b,
        'Œ' => 0x8c,
        'Ž' => 0x8e,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9a,
        '›' => 0x9b,
        'œ' => 0x9c,
        'ž' => 0x9e,
        'Ÿ' => 0x9f,
        _ => return None,
    })
}

/// `text` in `WinAnsi`; characters the standard fonts lack print as `?`.
pub(crate) fn encode(text: &str) -> Vec<u8> {
    text.chars().map(|c| if c.is_control() { b' ' } else { win_ansi(c).unwrap_or(b'?') }).collect()
}

/// `text` split into lines of at most `width` characters, at spaces where possible.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word: String = word.to_owned();
        while word.chars().count() > width {
            let head: String = word.chars().take(width).collect();
            word = word.chars().skip(width).collect();
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            lines.push(head);
        }
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// A UTC time as `2026-11-24 12:00:00 UTC`.
pub(crate) fn utc(at: UnixMillis) -> String {
    at.to_rfc3339().map_or_else(
        |_| at.0.to_string(),
        |text| {
            let text: String = text.chars().take(19).collect();
            format!("{} UTC", text.replace('T', " "))
        },
    )
}

/// A filled field, as the certificate lists it and the overlay draws it.
#[derive(Debug, Clone)]
pub(crate) struct FieldRecord {
    pub(crate) page: u32,
    pub(crate) rect: Rect,
    pub(crate) kind: FieldKind,
    pub(crate) label: String,
    pub(crate) value: FieldValue,
}

impl FieldRecord {
    /// `Page 2, Text 'Company': Acme`.
    fn line(&self) -> String {
        let kind = match self.kind {
            FieldKind::Signature => "Signature",
            FieldKind::Initials => "Initials",
            FieldKind::Date => "Date",
            FieldKind::Text => "Text",
            FieldKind::Checkbox => "Checkbox",
        };
        let label =
            if self.label.is_empty() { String::new() } else { format!(" '{}'", self.label) };
        let value = match &self.value {
            FieldValue::Signed => "signed",
            FieldValue::Initialed => "initialed",
            FieldValue::Date { date } => date,
            FieldValue::Text { text } => text,
            FieldValue::Checkbox { checked: true } => "ticked",
            FieldValue::Checkbox { checked: false } => "not ticked",
        };
        format!("Page {}, {kind}{label}: {value}", self.page)
    }
}

/// One signer, as the certificate prints them.
#[derive(Debug, Clone)]
pub(crate) struct SignedBy {
    pub(crate) name: String,
    pub(crate) email: String,
    pub(crate) signing_order: u8,
    pub(crate) signed_at: UnixMillis,
    /// From the signer's `signed` event.
    pub(crate) ip: Option<String>,
    pub(crate) user_agent: Option<String>,
    pub(crate) signature: Signature,
    pub(crate) initials: Option<Signature>,
    /// Their filled fields. Empty for requests made before fields.
    pub(crate) fields: Vec<FieldRecord>,
}

/// One audit line.
#[derive(Debug, Clone)]
pub(crate) struct AuditLine {
    pub(crate) at: UnixMillis,
    pub(crate) what: String,
    pub(crate) client: Option<String>,
}

/// What the final PDF shows: the fields on their pages, and the
/// certificate page.
#[derive(Debug, Clone)]
pub(crate) struct Record {
    pub(crate) title: String,
    pub(crate) file_name: String,
    pub(crate) request_id: String,
    pub(crate) sender: String,
    pub(crate) original_sha256: String,
    pub(crate) signers: Vec<SignedBy>,
    pub(crate) audit: Vec<AuditLine>,
}

impl Record {
    /// A record as long as a typical one, with one field of each kind on
    /// page 1, for the upload's dry run.
    fn dry_run() -> Self {
        let at = DRY_RUN_AT;
        let drawn = Signature::Drawn { strokes: vec![vec![[0, 0], [PAD_WIDTH, PAD_HEIGHT]]] };
        let field = |y: f64, kind, value| FieldRecord {
            page: 1,
            rect: Rect { x: 0.1, y, width: 0.3, height: 0.05 },
            kind,
            label: String::new(),
            value,
        };
        Self {
            title: "Dry run".to_owned(),
            file_name: "upload.pdf".to_owned(),
            request_id: "dry-run".to_owned(),
            sender: "sender@example.test".to_owned(),
            original_sha256: "0".repeat(64),
            signers: vec![SignedBy {
                name: "Signer".to_owned(),
                email: "signer@example.test".to_owned(),
                signing_order: 1,
                signed_at: at,
                ip: None,
                user_agent: None,
                signature: drawn,
                initials: Some(Signature::Typed { name: "SG".to_owned() }),
                fields: vec![
                    field(0.1, FieldKind::Signature, FieldValue::Signed),
                    field(0.2, FieldKind::Initials, FieldValue::Initialed),
                    field(0.3, FieldKind::Date, FieldValue::Date { date: "2026-11-24".to_owned() }),
                    field(0.4, FieldKind::Text, FieldValue::Text { text: "Acme".to_owned() }),
                    field(0.5, FieldKind::Checkbox, FieldValue::Checkbox { checked: true }),
                ],
            }],
            audit: vec![AuditLine { at, what: "Signed".to_owned(), client: None }],
        }
    }

    /// The fields to draw, with each signer's adopted signature and
    /// initials in theirs.
    fn filled(&self) -> Vec<Filled<'_>> {
        let mut filled = Vec::new();
        for signer in &self.signers {
            for field in &signer.fields {
                let mark = match &field.value {
                    FieldValue::Signed => signer.signature.mark(),
                    FieldValue::Initialed => match &signer.initials {
                        Some(initials) => initials.mark(),
                        None => continue,
                    },
                    FieldValue::Date { date } => Mark::Date(date),
                    FieldValue::Text { text } => Mark::Text(text),
                    FieldValue::Checkbox { checked: true } => Mark::Check,
                    FieldValue::Checkbox { checked: false } => continue,
                };
                filled.push(Filled { page: field.page, rect: field.rect, mark });
            }
        }
        filled
    }
}

const PAGE_WIDTH: f64 = 612.0;
const PAGE_HEIGHT: f64 = 792.0;
const MARGIN: f64 = 56.0;
const BOTTOM: f64 = 56.0;
const SIG_BOX_WIDTH: f64 = 200.0;
const INITIALS_BOX_WIDTH: f64 = 80.0;
const SIG_BOX_HEIGHT: f64 = 60.0;
/// The smallest text a field shrinks to; longer text is clipped.
const MIN_TEXT_SIZE: f64 = 6.0;
/// Helvetica's cap height, in thousandths of the font size.
const CAP_HEIGHT: f64 = 718.0;

/// Helvetica's glyph widths in thousandths of the font size, by `WinAnsi`
/// byte, from the standard 14 font AFM metrics. Helvetica-Oblique has the
/// same widths. Bytes `WinAnsi` leaves undefined are 0; [`encode`] never
/// produces them.
#[rustfmt::skip]
const HELVETICA_WIDTHS: [u16; 256] = [
    // 0x00 - 0x1f: control codes
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    // 0x20: space ! " # $ % & ' ( ) * + , - . /
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
    // 0x30: 0-9 : ; < = > ?
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
    // 0x40: @ A-O
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
    // 0x50: P-Z [ \ ] ^ _
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
    // 0x60: ` a-o
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    // 0x70: p-z { | } ~ (0x7f undefined)
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, 0,
    // 0x80: Euro, -, quotesinglbase, florin, quotedblbase, ellipsis, dagger,
    // daggerdbl, circumflex, perthousand, Scaron, guilsinglleft, OE, -, Zcaron, -
    556, 0, 222, 556, 333, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0,
    // 0x90: -, quoteleft, quoteright, quotedblleft, quotedblright, bullet,
    // endash, emdash, tilde, trademark, scaron, guilsinglright, oe, -, zcaron, Ydieresis
    0, 222, 222, 333, 333, 350, 556, 1000, 333, 1000, 500, 333, 944, 0, 500, 667,
    // 0xa0: nbsp, exclamdown, cent, sterling, currency, yen, brokenbar, section,
    // dieresis, copyright, ordfeminine, guillemotleft, logicalnot, hyphen, registered, macron
    278, 333, 556, 556, 556, 556, 260, 556, 333, 737, 370, 556, 584, 333, 737, 333,
    // 0xb0: degree, plusminus, twosuperior, threesuperior, acute, mu, paragraph,
    // periodcentered, cedilla, onesuperior, ordmasculine, guillemotright,
    // onequarter, onehalf, threequarters, questiondown
    400, 584, 333, 333, 333, 556, 537, 278, 333, 333, 365, 556, 834, 834, 834, 611,
    // 0xc0: Agrave-Aring, AE, Ccedilla, Egrave-Edieresis, Igrave-Idieresis
    667, 667, 667, 667, 667, 667, 1000, 722, 667, 667, 667, 667, 278, 278, 278, 278,
    // 0xd0: Eth, Ntilde, Ograve-Odieresis, multiply, Oslash, Ugrave-Udieresis, Yacute, Thorn, germandbls
    722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667, 611,
    // 0xe0: agrave-aring, ae, ccedilla, egrave-edieresis, igrave-idieresis
    556, 556, 556, 556, 556, 556, 889, 500, 556, 556, 556, 556, 278, 278, 278, 278,
    // 0xf0: eth, ntilde, ograve-odieresis, divide, oslash, ugrave-udieresis, yacute, thorn, ydieresis
    556, 556, 556, 556, 556, 556, 556, 584, 611, 556, 556, 556, 556, 500, 556, 500,
];

/// The width of `text` in Helvetica at size 1, in points.
fn text_width(text: &str) -> f64 {
    let width = |byte: &u8| HELVETICA_WIDTHS.get(usize::from(*byte)).copied().unwrap_or(0);
    let thousandths: u32 = encode(text).iter().map(|byte| u32::from(width(byte))).sum();
    f64::from(thousandths) / 1000.0
}

/// The standard 14 fonts the added content uses, by resource name.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Font {
    Regular,
    Bold,
    Italic,
}

impl Font {
    const fn name(self) -> &'static str {
        match self {
            Self::Regular => "F1",
            Self::Bold => "F2",
            Self::Italic => "F3",
        }
    }
}

/// A `/Resources` object with the three [`Font`]s.
pub(crate) fn font_resources(doc: &mut Document) -> ObjectId {
    let mut font = |base: &str| {
        doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => base, "Encoding" => "WinAnsiEncoding",
        })
    };
    let (regular, bold, italic) =
        (font("Helvetica"), font("Helvetica-Bold"), font("Helvetica-Oblique"));
    let fonts: Dictionary = dictionary! { "F1" => regular, "F2" => bold, "F3" => italic };
    doc.add_object(dictionary! { "Font" => fonts })
}

/// Text with its baseline at `(x, y)`. Strings go in hex, so no text can
/// close the string and inject operators.
fn text_op((x, y): (f64, f64), font: Font, size: f64, text: &str) -> Vec<Operation> {
    vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(font.name().as_bytes().to_vec()), size.into()]),
        Operation::new("Td", vec![x.into(), y.into()]),
        Operation::new("Tj", vec![Object::String(encode(text), StringFormat::Hexadecimal)]),
        Operation::new("ET", vec![]),
    ]
}

/// One line of `text` in the box `(x, y, width, height)`: sized to the
/// height, shrunk to fit the width down to 6 pt, and centred vertically.
/// Regular or italic only; their widths are the ones measured.
pub(crate) fn fitted_text(
    font: Font,
    text: &str,
    (x, y, width, height): (f64, f64, f64, f64),
) -> Vec<Operation> {
    let pad = (height * 0.1).min(width * 0.05);
    let room = pad.mul_add(-2.0, width);
    let em = text_width(text);
    let mut size = height * 0.7;
    if em * size > room && em > 0.0 {
        size = (room / em).max(MIN_TEXT_SIZE);
    }
    let baseline = y + size.mul_add(-CAP_HEIGHT / 1000.0, height) / 2.0;
    text_op((x + pad, baseline), font, size, text)
}

/// Pad strokes scaled into the box `(x, y, width, height)`: the drawing's
/// own extent fills 90% of the box with its aspect ratio kept, centred,
/// and the pad's y-down points flipped.
pub(crate) fn strokes(
    strokes: &[Vec<[u16; 2]>],
    (x, y, width, height): (f64, f64, f64, f64),
) -> Vec<Operation> {
    let all = || strokes.iter().flatten();
    let (Some(min_x), Some(max_x), Some(min_y), Some(max_y)) = (
        all().map(|p| p[0]).min(),
        all().map(|p| p[0]).max(),
        all().map(|p| p[1]).min(),
        all().map(|p| p[1]).max(),
    ) else {
        return Vec::new();
    };
    let extent_x = f64::from(max_x - min_x).max(1.0);
    let extent_y = f64::from(max_y - min_y).max(1.0);
    let scale = (width * 0.9 / extent_x).min(height * 0.9 / extent_y);
    let left = extent_x.mul_add(-scale, width) / 2.0 + x;
    let bottom = extent_y.mul_add(-scale, height) / 2.0 + y;
    let point = |[px, py]: [u16; 2]| -> (f64, f64) {
        (f64::from(px - min_x).mul_add(scale, left), f64::from(max_y - py).mul_add(scale, bottom))
    };
    let mut ops = vec![
        Operation::new("q", vec![]),
        Operation::new("w", vec![(scale * 5.0).clamp(0.6, 2.5).into()]),
        Operation::new("J", vec![1.into()]),
        Operation::new("j", vec![1.into()]),
    ];
    for stroke in strokes {
        let mut path = stroke.iter().map(|p| point(*p));
        let Some((start_x, start_y)) = path.next() else { continue };
        ops.push(Operation::new("m", vec![start_x.into(), start_y.into()]));
        for (to_x, to_y) in path {
            ops.push(Operation::new("l", vec![to_x.into(), to_y.into()]));
        }
        ops.push(Operation::new("S", vec![]));
    }
    ops.push(Operation::new("Q", vec![]));
    ops
}

/// A signature or initials in the box `(x, y, width, height)`.
fn adopted(signature: &Signature, area: (f64, f64, f64, f64)) -> Vec<Operation> {
    match signature {
        Signature::Drawn { strokes: drawn } => strokes(drawn, area),
        Signature::Typed { name } => fitted_text(Font::Italic, name, area),
    }
}

/// Lays out pages top to bottom, starting a new page when one fills.
#[derive(Default)]
struct Pages {
    done: Vec<Vec<Operation>>,
    ops: Vec<Operation>,
    y: f64,
}

impl Pages {
    fn new() -> Self {
        Self { done: Vec::new(), ops: Vec::new(), y: PAGE_HEIGHT - MARGIN }
    }

    fn need(&mut self, height: f64) {
        if self.y - height < BOTTOM {
            self.done.push(std::mem::take(&mut self.ops));
            self.y = PAGE_HEIGHT - MARGIN;
        }
    }

    fn line(&mut self, font: Font, size: f64, text: &str) {
        let height = size * 1.4;
        self.need(height);
        self.y -= height;
        self.ops.extend(text_op((MARGIN, self.y), font, size, text));
    }

    fn paragraph(&mut self, size: f64, width: usize, text: &str) {
        for line in wrap(text, width) {
            self.line(Font::Regular, size, &line);
        }
    }

    fn gap(&mut self, height: f64) {
        self.y -= height;
    }

    /// An outlined box at `(x, bottom)` holding `signature`, captioned.
    fn adopted_box(
        &mut self,
        (x, bottom, width): (f64, f64, f64),
        caption: &str,
        signature: &Signature,
    ) {
        self.ops.extend([
            Operation::new("q", vec![]),
            Operation::new("G", vec![0.7.into()]),
            Operation::new("w", vec![0.5.into()]),
            Operation::new(
                "re",
                vec![x.into(), bottom.into(), width.into(), SIG_BOX_HEIGHT.into()],
            ),
            Operation::new("S", vec![]),
            Operation::new("Q", vec![]),
        ]);
        self.ops.extend(adopted(signature, (x, bottom, width, SIG_BOX_HEIGHT)));
        self.ops.extend(text_op((x, bottom - 10.0), Font::Regular, 7.0, caption));
    }

    fn signer(&mut self, signer: &SignedBy) {
        self.need(60.0);
        self.line(Font::Bold, 10.0, &format!("{} <{}>", signer.name, signer.email));
        self.line(
            Font::Regular,
            9.0,
            &format!("Signing order {} - signed {}", signer.signing_order, utc(signer.signed_at)),
        );
        if signer.ip.is_some() || signer.user_agent.is_some() {
            let client = format!(
                "IP {} - {}",
                signer.ip.as_deref().unwrap_or("unknown"),
                signer.user_agent.as_deref().unwrap_or("unknown browser")
            );
            self.paragraph(7.5, 125, &client);
        }
        self.need(SIG_BOX_HEIGHT + 24.0);
        let bottom = self.y - 6.0 - SIG_BOX_HEIGHT;
        self.adopted_box((MARGIN, bottom, SIG_BOX_WIDTH), "Signature", &signer.signature);
        if let Some(initials) = &signer.initials {
            let x = MARGIN + SIG_BOX_WIDTH + 12.0;
            self.adopted_box((x, bottom, INITIALS_BOX_WIDTH), "Initials", initials);
        }
        self.y = bottom - 14.0;
        for field in &signer.fields {
            self.paragraph(8.5, 110, &field.line());
        }
        self.gap(14.0);
    }

    fn finish(mut self) -> Vec<Vec<Operation>> {
        if !self.ops.is_empty() {
            self.done.push(self.ops);
        }
        self.done
    }
}

/// The certificate: a summary, each signer with their adopted signature
/// and fields, then the events.
fn layout(record: &Record, pages: usize) -> Vec<Vec<Operation>> {
    let mut out = Pages::new();
    out.line(Font::Bold, 18.0, "Certificate of completion");
    out.paragraph(10.0, 90, &record.title);
    out.paragraph(9.0, 100, &format!("Request {} - sent by {}", record.request_id, record.sender));
    let pages = if pages == 1 { "1 page".to_owned() } else { format!("{pages} pages") };
    out.paragraph(
        8.0,
        110,
        &format!(
            "{} - {pages} - SHA-256 of the original: {}",
            record.file_name, record.original_sha256
        ),
    );
    out.gap(4.0);
    out.paragraph(
        9.0,
        100,
        "bizkit keeps the original file with this request. Each signer received a private link by email, so \
         their identity rests on control of that address. Each signer agreed to sign electronically before \
         signing, and their fields were drawn onto the pages above. bizkit sealed this file with its \
         platform certificate after the last signature; any change to the file breaks the seal.",
    );
    out.gap(10.0);
    out.line(Font::Bold, 13.0, "Signers");
    out.gap(4.0);
    for signer in &record.signers {
        out.signer(signer);
    }
    out.gap(6.0);
    out.line(Font::Bold, 13.0, "Events");
    out.gap(4.0);
    for entry in &record.audit {
        out.line(Font::Bold, 8.5, &utc(entry.at));
        out.paragraph(8.5, 110, &entry.what);
        if let Some(client) = &entry.client {
            out.paragraph(7.5, 125, client);
        }
        out.gap(4.0);
    }
    out.finish()
}

/// Loads the original, draws the fields on its pages and appends the
/// certificate.
pub(crate) fn assemble(original: &[u8], record: &Record) -> anyhow::Result<Document> {
    let doc = Document::load_mem(original).context("reading the original PDF")?;
    finish(doc, record)
}

fn finish(mut doc: Document, record: &Record) -> anyhow::Result<Document> {
    let pages = doc.get_pages().len();
    overlay::apply(&mut doc, &record.filled()).context("drawing the fields")?;
    append_certificate(doc, record, pages)
}

/// Appends the certificate pages to `doc`.
fn append_certificate(
    mut doc: Document,
    record: &Record,
    original_pages: usize,
) -> anyhow::Result<Document> {
    let pages_id = match doc.catalog().context("the PDF has no catalog")?.get(b"Pages") {
        Ok(Object::Reference(id)) => *id,
        _ => anyhow::bail!("the PDF's page tree isn't a reference"),
    };
    let mut kids = match doc.get_dictionary(pages_id).context("reading the page tree")?.get(b"Kids")
    {
        Ok(Object::Array(kids)) => kids.clone(),
        Ok(Object::Reference(id)) => doc
            .get_object(*id)
            .and_then(Object::as_array)
            .context("reading the page tree's Kids")?
            .clone(),
        _ => anyhow::bail!("the page tree has no Kids array"),
    };
    let resources = font_resources(&mut doc);
    let mut added: Vec<ObjectId> = Vec::new();
    for ops in layout(record, original_pages) {
        let content = Content { operations: ops }.encode().context("encoding a page")?;
        let stream = doc.add_object(Stream::new(dictionary! {}, content));
        let media_box = || vec![0.into(), 0.into(), PAGE_WIDTH.into(), PAGE_HEIGHT.into()];
        // Rotate and CropBox inherit from the page tree; set them so the
        // original's values don't turn or clip the added pages.
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => media_box(),
            "CropBox" => media_box(),
            "Rotate" => 0,
            "Contents" => stream,
            "Resources" => resources,
        });
        added.push(page);
    }
    let tree = doc.get_dictionary_mut(pages_id).context("reading the page tree")?;
    let count = tree.get(b"Count").and_then(Object::as_i64).unwrap_or(0);
    kids.extend(added.iter().map(|id| Object::Reference(*id)));
    tree.set("Kids", kids);
    tree.set("Count", count + i64::try_from(added.len()).unwrap_or(0));
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer(name: &str, email: &str, signature: Signature) -> SignedBy {
        SignedBy {
            name: name.to_owned(),
            email: email.to_owned(),
            signing_order: 1,
            signed_at: UnixMillis(1_795_000_000_000),
            ip: Some("192.0.2.1".to_owned()),
            user_agent: Some("Mozilla/5.0".to_owned()),
            signature,
            initials: None,
            fields: Vec::new(),
        }
    }

    fn record(audit: usize) -> Record {
        let mut ada = signer(
            "Ada",
            "ada@example.test",
            Signature::Drawn { strokes: vec![vec![[0, 0], [500, 150], [1000, 300]]] },
        );
        ada.initials = Some(Signature::Typed { name: "AL".to_owned() });
        ada.fields = vec![
            FieldRecord {
                page: 1,
                rect: Rect { x: 0.1, y: 0.8, width: 0.3, height: 0.06 },
                kind: FieldKind::Signature,
                label: String::new(),
                value: FieldValue::Signed,
            },
            FieldRecord {
                page: 2,
                rect: Rect { x: 0.1, y: 0.1, width: 0.3, height: 0.04 },
                kind: FieldKind::Text,
                label: "Company".to_owned(),
                value: FieldValue::Text { text: "Acme".to_owned() },
            },
        ];
        Record {
            title: "Services (agreement) <x>".to_owned(),
            file_name: "agreement.pdf".to_owned(),
            request_id: "r1".to_owned(),
            sender: "ops@example.test".to_owned(),
            original_sha256: "ab".repeat(32),
            signers: vec![
                ada,
                signer("Пётр", "p@example.test", Signature::Typed { name: "Peter".to_owned() }),
            ],
            audit: (0..audit)
                .map(|n| AuditLine {
                    at: UnixMillis(1_795_000_000_000),
                    what: format!("Event {n}"),
                    client: Some("IP 192.0.2.1 - Mozilla/5.0".to_owned()),
                })
                .collect(),
        }
    }

    #[test]
    fn one_certificate_page_follows_the_original() {
        let original = crate::testing::sample_pdf(&["One", "Two"]);
        let doc = assemble(&original, &record(3)).unwrap();
        let mut out = Vec::new();
        let mut doc = doc;
        doc.save_to(&mut out).unwrap();
        let back = Document::load_mem(&out).unwrap();
        let pages = back.get_pages();
        assert_eq!(pages.len(), 3, "two original pages and the certificate");
        let text = back.extract_text(&[3]).unwrap();
        assert!(text.contains("Certificate of completion"), "{text}");
        assert!(text.contains("2 pages"), "{text}");
        assert!(text.contains("Signing order 1"), "{text}");
        assert!(text.contains("IP 192.0.2.1 - Mozilla/5.0"), "{text}");
        assert!(text.contains("Page 2, Text 'Company': Acme"), "{text}");
        assert!(text.contains("Page 1, Signature: signed"), "{text}");
        assert!(text.contains("Events"), "{text}");
        assert!(text.contains("???? <p@example.test>"), "Cyrillic prints as ?: {text}");
        assert!(!text.contains("Audit trail"), "{text}");
        for page in [1, 2] {
            let xobjects = back
                .get_dictionary(pages[&page])
                .unwrap()
                .get(b"Resources")
                .and_then(Object::as_dict)
                .and_then(|r| r.get(b"XObject"))
                .and_then(Object::as_dict)
                .unwrap();
            assert!(xobjects.has(b"BizkitFields"), "page {page} has its overlay");
        }
    }

    #[test]
    fn helvetica_widths_match_the_afm() {
        assert!((text_width("Hello") - 2.278).abs() < 1e-4, "{}", text_width("Hello"));
        assert_eq!(HELVETICA_WIDTHS[usize::from(b' ')], 278);
        assert_eq!(HELVETICA_WIDTHS[0x80], 556, "Euro");
        assert_eq!(HELVETICA_WIDTHS[0xff], 500, "ydieresis");
    }

    #[test]
    fn text_shrinks_to_the_width_but_not_below_6_points() {
        let size = |ops: &[Operation]| {
            f64::from(
                ops.iter().find(|op| op.operator == "Tf").unwrap().operands[1].as_float().unwrap(),
            )
        };
        let short = fitted_text(Font::Regular, "Hi", (0.0, 0.0, 200.0, 20.0));
        assert!((size(&short) - 14.0).abs() < 1e-4, "the height sets the size");
        // 24 wide less a 1.2 pad each side leaves 21.6 for 2.278 em.
        let long = fitted_text(Font::Regular, "Hello", (0.0, 0.0, 24.0, 20.0));
        assert!(size(&long).mul_add(2.278, -21.6).abs() < 1e-3, "{}", size(&long));
        let huge = fitted_text(Font::Regular, &"W".repeat(200), (0.0, 0.0, 50.0, 20.0));
        assert!((size(&huge) - MIN_TEXT_SIZE).abs() < 1e-4);
    }

    #[test]
    fn strokes_fill_the_box_with_their_aspect_kept() {
        let ops = strokes(&[vec![[100, 100], [300, 200]]], (10.0, 20.0, 100.0, 100.0));
        let points: Vec<Vec<f64>> = ops
            .iter()
            .filter(|op| op.operator == "m" || op.operator == "l")
            .map(|op| op.operands.iter().map(|n| f64::from(n.as_float().unwrap())).collect())
            .collect();
        // 200 x 100 of pad scaled by 0.45 to 90 x 45, centred. The pad's y
        // runs down, so its first point is the higher one.
        let expected = [[15.0, 92.5], [105.0, 47.5]];
        for (point, want) in points.iter().zip(expected) {
            assert!(
                (point[0] - want[0]).abs() < 1e-3 && (point[1] - want[1]).abs() < 1e-3,
                "{points:?}"
            );
        }
    }

    /// `sample_pdf` with its page tree changed by `edit`.
    fn edited(edit: impl FnOnce(&mut Document, ObjectId)) -> Vec<u8> {
        let mut doc = Document::load_mem(&crate::testing::sample_pdf(&["One", "Two"])).unwrap();
        let pages_id = doc.catalog().unwrap().get(b"Pages").unwrap().as_reference().unwrap();
        edit(&mut doc, pages_id);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn added_pages_take_an_indirect_kids_array_and_ignore_inherited_rotation() {
        let original = edited(|doc, pages_id| {
            let kids = doc.get_dictionary(pages_id).unwrap().get(b"Kids").unwrap().clone();
            let kids_id = doc.add_object(kids);
            let tree = doc.get_dictionary_mut(pages_id).unwrap();
            tree.set("Kids", kids_id);
            tree.set("Rotate", 90);
            tree.set("CropBox", vec![0.into(), 0.into(), 100.into(), 100.into()]);
        });
        let doc = assemble(&original, &record(1)).unwrap();
        let pages = doc.get_pages();
        assert_eq!(pages.len(), 3);
        let added = doc.get_dictionary(pages[&3]).unwrap();
        assert_eq!(added.get(b"Rotate").unwrap().as_i64().unwrap(), 0);
        assert_eq!(added.get(b"CropBox").unwrap(), added.get(b"MediaBox").unwrap());
    }

    #[tokio::test]
    async fn uploads_that_cant_be_sealed_are_refused() {
        assert_eq!(
            check(crate::testing::sample_pdf(&["One", "Two"]), ParseClass::Small).await,
            Ok(2)
        );
        let direct_tree = edited(|doc, pages_id| {
            let tree = doc.get_dictionary(pages_id).unwrap().clone();
            doc.catalog_mut().unwrap().set("Pages", tree);
        });
        assert_eq!(check(direct_tree, ParseClass::Small).await, Err("must have at least one page"));
        let no_kids = edited(|doc, pages_id| {
            doc.get_dictionary_mut(pages_id).unwrap().set("Kids", 7);
        });
        assert!(check(no_kids, ParseClass::Small).await.is_err());
        let turned = edited(|doc, pages_id| {
            doc.get_dictionary_mut(pages_id).unwrap().set("Rotate", 45);
        });
        assert_eq!(
            check(turned, ParseClass::Small).await,
            Err("must have pages with readable sizes, turned by multiples of 90 degrees")
        );
        let flat = edited(|doc, pages_id| {
            let flat = vec![0.into(), 0.into(), 612.into(), 0.into()];
            doc.get_dictionary_mut(pages_id).unwrap().set("MediaBox", flat);
        });
        assert!(check(flat, ParseClass::Small).await.is_err(), "a MediaBox with no area");
    }

    #[tokio::test]
    async fn page_counts_skip_the_upload_checks() {
        let pages = vec!["x"; MAX_PAGES + 1];
        assert_eq!(page_count(crate::testing::sample_pdf(&pages)).await, Ok(501));
        assert_eq!(page_count(b"%PDF-1.7 garbage".to_vec()).await, Err("must be a PDF that opens"));
    }

    #[tokio::test]
    async fn a_parse_that_waits_past_its_budget_times_out() {
        let held = ParseClass::Large.permits().acquire().await.unwrap();
        let waited =
            parse_within(ParseClass::Large.permits(), Duration::from_millis(50), || Ok(())).await;
        assert_eq!(waited, Err(TOO_SLOW));
        assert_eq!(
            check(crate::testing::sample_pdf(&["x"]), ParseClass::Small).await,
            Ok(1),
            "small files have their own queue"
        );
        drop(held);
    }

    #[tokio::test]
    async fn uploads_have_at_most_500_pages() {
        let pages = vec!["x"; MAX_PAGES + 1];
        let refused = check(crate::testing::sample_pdf(&pages), ParseClass::Small).await;
        assert_eq!(refused, Err("must have at most 500 pages"));
    }

    #[test]
    fn a_long_trail_spills_onto_more_pages() {
        let original = crate::testing::sample_pdf(&["One", "Two"]);
        let doc = assemble(&original, &record(80)).unwrap();
        assert!(doc.get_pages().len() > 3, "{}", doc.get_pages().len());
    }

    #[test]
    fn text_wraps_at_spaces() {
        assert_eq!(wrap("aaa bbb ccc", 7), ["aaa bbb", "ccc"]);
        assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(wrap("", 4), [""]);
    }

    #[test]
    fn signed_names_keep_the_stem() {
        assert_eq!(signed_file_name("contract.pdf"), "contract-signed.pdf");
        assert_eq!(signed_file_name("NDA.PDF"), "NDA-signed.pdf");
        assert_eq!(signed_file_name("plain"), "plain-signed.pdf");
    }
}
