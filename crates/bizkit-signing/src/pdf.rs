//! Reading uploaded PDFs, and the final PDF's signatures and audit pages.

use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::time::UnixMillis;
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, ObjectId, Stream, StringFormat, dictionary};

use crate::signature::{PAD_HEIGHT, PAD_WIDTH, Signature};

/// How long parsing an upload may take. Parsing runs off the async
/// runtime; a hostile file that takes longer is refused.
const PARSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Uploads parse one at a time: a small file can inflate a lot.
static PARSING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

/// Checks that `bytes` is a PDF bizkit can add pages to. The error is a
/// message for the `pdf_base64` field.
pub(crate) async fn check(bytes: Vec<u8>) -> Result<(), &'static str> {
    if !bytes.starts_with(b"%PDF-") {
        return Err("must be a PDF");
    }
    let Ok(_permit) = PARSING.acquire().await else { return Err("must be a PDF that opens") };
    let parsed = tokio::task::spawn_blocking(move || {
        let doc = Document::load_mem(&bytes).map_err(|_| "must be a PDF that opens")?;
        // lopdf decrypts files with an empty password as it loads them, and
        // the copy it saves would drop the protection.
        if doc.is_encrypted() || doc.encryption_state.is_some() {
            return Err("must not be encrypted or password-protected");
        }
        if doc.get_pages().is_empty() {
            return Err("must have at least one page");
        }
        Ok(())
    });
    match tokio::time::timeout(PARSE_TIMEOUT, parsed).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) | Err(_) => Err("must be a PDF that opens"),
    }
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
fn encode(text: &str) -> Vec<u8> {
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

/// One signer, as the signatures page prints them.
#[derive(Debug, Clone)]
pub(crate) struct SignedBy {
    pub(crate) name: String,
    pub(crate) email: String,
    pub(crate) signed_at: UnixMillis,
    pub(crate) signature: Signature,
}

/// One audit line.
#[derive(Debug, Clone)]
pub(crate) struct AuditLine {
    pub(crate) at: UnixMillis,
    pub(crate) what: String,
    pub(crate) client: Option<String>,
}

/// What the appended pages say.
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

const PAGE_WIDTH: f32 = 612.0;
const PAGE_HEIGHT: f32 = 792.0;
const MARGIN: f32 = 56.0;
const BOTTOM: f32 = 56.0;
const SIG_BOX_WIDTH: f32 = 280.0;
const SIG_BOX_HEIGHT: f32 = 84.0;

#[derive(Debug, Clone, Copy)]
enum Font {
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

/// Lays out pages top to bottom, starting a new page when one fills.
#[derive(Default)]
struct Pages {
    done: Vec<Vec<Operation>>,
    ops: Vec<Operation>,
    y: f32,
}

impl Pages {
    fn new() -> Self {
        Self { done: Vec::new(), ops: Vec::new(), y: PAGE_HEIGHT - MARGIN }
    }

    fn need(&mut self, height: f32) {
        if self.y - height < BOTTOM {
            self.done.push(std::mem::take(&mut self.ops));
            self.y = PAGE_HEIGHT - MARGIN;
        }
    }

    /// Text at `x` on the current line; strings go in hex, so no text
    /// can close the string and inject operators.
    fn text_at(&mut self, (x, y): (f32, f32), font: Font, size: f32, text: &str) {
        self.ops.extend([
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(font.name().as_bytes().to_vec()), size.into()]),
            Operation::new("Td", vec![x.into(), y.into()]),
            Operation::new("Tj", vec![Object::String(encode(text), StringFormat::Hexadecimal)]),
            Operation::new("ET", vec![]),
        ]);
    }

    fn line(&mut self, font: Font, size: f32, text: &str) {
        let height = size * 1.4;
        self.need(height);
        self.y -= height;
        self.text_at((MARGIN, self.y), font, size, text);
    }

    fn paragraph(&mut self, size: f32, width: usize, text: &str) {
        for line in wrap(text, width) {
            self.line(Font::Regular, size, &line);
        }
    }

    fn gap(&mut self, height: f32) {
        self.y -= height;
    }

    fn signature(&mut self, signer: &SignedBy) {
        self.need(SIG_BOX_HEIGHT + 48.0);
        let top = self.y - 8.0;
        let bottom = top - SIG_BOX_HEIGHT;
        self.ops.extend([
            Operation::new("q", vec![]),
            Operation::new("G", vec![0.7.into()]),
            Operation::new("w", vec![0.5.into()]),
            Operation::new(
                "re",
                vec![MARGIN.into(), bottom.into(), SIG_BOX_WIDTH.into(), SIG_BOX_HEIGHT.into()],
            ),
            Operation::new("S", vec![]),
            Operation::new("Q", vec![]),
        ]);
        match &signer.signature {
            Signature::Drawn { strokes } => self.strokes(strokes, bottom),
            Signature::Typed { name } => {
                self.text_at((MARGIN + 12.0, bottom + 30.0), Font::Italic, 26.0, name);
            }
        }
        self.y = bottom;
        self.line(Font::Bold, 10.0, &format!("{} <{}>", signer.name, signer.email));
        self.line(Font::Regular, 9.0, &format!("Signed {}", utc(signer.signed_at)));
        self.gap(14.0);
    }

    fn strokes(&mut self, strokes: &[Vec<[u16; 2]>], bottom: f32) {
        let scale_x = SIG_BOX_WIDTH / f32::from(PAD_WIDTH);
        let scale_y = SIG_BOX_HEIGHT / f32::from(PAD_HEIGHT);
        let point = |[x, y]: [u16; 2]| -> (f32, f32) {
            (
                f32::mul_add(f32::from(x), scale_x, MARGIN),
                f32::mul_add(f32::from(PAD_HEIGHT.saturating_sub(y)), scale_y, bottom),
            )
        };
        self.ops.extend([
            Operation::new("q", vec![]),
            Operation::new("w", vec![1.4.into()]),
            Operation::new("J", vec![1.into()]),
            Operation::new("j", vec![1.into()]),
        ]);
        for stroke in strokes {
            let mut points = stroke.iter().map(|p| point(*p));
            let Some((start_x, start_y)) = points.next() else { continue };
            self.ops.push(Operation::new("m", vec![start_x.into(), start_y.into()]));
            for (x, y) in points {
                self.ops.push(Operation::new("l", vec![x.into(), y.into()]));
            }
            self.ops.push(Operation::new("S", vec![]));
        }
        self.ops.push(Operation::new("Q", vec![]));
    }

    fn finish(mut self) -> Vec<Vec<Operation>> {
        if !self.ops.is_empty() {
            self.done.push(self.ops);
        }
        self.done
    }
}

fn layout(record: &Record) -> Vec<Vec<Operation>> {
    let mut pages = Pages::new();
    pages.line(Font::Bold, 18.0, "Signatures");
    pages.paragraph(10.0, 90, &record.title);
    pages.paragraph(
        8.0,
        110,
        &format!("{} - SHA-256 of the original: {}", record.file_name, record.original_sha256),
    );
    pages.gap(10.0);
    for signer in &record.signers {
        pages.signature(signer);
    }
    pages.done.push(std::mem::take(&mut pages.ops));
    pages.y = PAGE_HEIGHT - MARGIN;

    pages.line(Font::Bold, 18.0, "Audit trail");
    pages.paragraph(
        9.0,
        100,
        &format!(
            "Request {} - \"{}\" - sent by {}",
            record.request_id, record.title, record.sender
        ),
    );
    pages.paragraph(
        9.0,
        100,
        &format!(
            "The original file ({}) has SHA-256 {}; bizkit keeps it with this request. Each signer received a \
             private link by email, so their identity rests on control of that address. Each signer agreed to sign \
             electronically before signing. bizkit sealed this file with its platform certificate after the last \
             signature; any change to the file breaks the seal.",
            record.file_name, record.original_sha256
        ),
    );
    pages.gap(8.0);
    for entry in &record.audit {
        pages.line(Font::Bold, 8.5, &utc(entry.at));
        pages.paragraph(8.5, 110, &entry.what);
        if let Some(client) = &entry.client {
            pages.paragraph(7.5, 125, client);
        }
        pages.gap(4.0);
    }
    pages.finish()
}

/// Loads the original and appends the signatures and audit pages.
pub(crate) fn assemble(original: &[u8], record: &Record) -> anyhow::Result<Document> {
    let mut doc = Document::load_mem(original).context("reading the original PDF")?;
    let pages_id = match doc.catalog().context("the PDF has no catalog")?.get(b"Pages") {
        Ok(Object::Reference(id)) => *id,
        _ => anyhow::bail!("the PDF's page tree isn't a reference"),
    };
    let font = |target: &mut Document, base: &str| {
        target.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => base, "Encoding" => "WinAnsiEncoding",
        })
    };
    let (regular, bold, italic) = (
        font(&mut doc, "Helvetica"),
        font(&mut doc, "Helvetica-Bold"),
        font(&mut doc, "Helvetica-Oblique"),
    );
    let resources = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => regular, "F2" => bold, "F3" => italic },
    });
    let mut added: Vec<ObjectId> = Vec::new();
    for ops in layout(record) {
        let content = Content { operations: ops }.encode().context("encoding a page")?;
        let stream = doc.add_object(Stream::new(dictionary! {}, content));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), PAGE_WIDTH.into(), PAGE_HEIGHT.into()],
            "Contents" => stream,
            "Resources" => resources,
        });
        added.push(page);
    }
    let tree = doc.get_dictionary_mut(pages_id).context("reading the page tree")?;
    let count = tree.get(b"Count").and_then(Object::as_i64).unwrap_or(0);
    let mut kids = match tree.get(b"Kids") {
        Ok(Object::Array(kids)) => kids.clone(),
        _ => anyhow::bail!("the page tree has no Kids array"),
    };
    kids.extend(added.iter().map(|id| Object::Reference(*id)));
    tree.set("Kids", kids);
    tree.set("Count", count + i64::try_from(added.len()).unwrap_or(0));
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(audit: usize) -> Record {
        Record {
            title: "Services (agreement) <x>".to_owned(),
            file_name: "agreement.pdf".to_owned(),
            request_id: "r1".to_owned(),
            sender: "ops@example.test".to_owned(),
            original_sha256: "ab".repeat(32),
            signers: vec![
                SignedBy {
                    name: "Ada".to_owned(),
                    email: "ada@example.test".to_owned(),
                    signed_at: UnixMillis(1_795_000_000_000),
                    signature: Signature::Drawn {
                        strokes: vec![vec![[0, 0], [500, 150], [1000, 300]]],
                    },
                },
                SignedBy {
                    name: "Пётр".to_owned(),
                    email: "p@example.test".to_owned(),
                    signed_at: UnixMillis(1_795_000_000_000),
                    signature: Signature::Typed { name: "Peter".to_owned() },
                },
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
    fn the_pages_follow_the_original() {
        let original = crate::testing::sample_pdf(&["One", "Two"]);
        let doc = assemble(&original, &record(3)).unwrap();
        let mut out = Vec::new();
        let mut doc = doc;
        doc.save_to(&mut out).unwrap();
        let back = Document::load_mem(&out).unwrap();
        let pages = back.get_pages();
        assert_eq!(pages.len(), 4, "two original pages, signatures, audit");
        let text = back.extract_text(&[3, 4]).unwrap();
        assert!(text.contains("Signatures"), "{text}");
        assert!(text.contains("Audit trail"), "{text}");
        assert!(text.contains("???? <p@example.test>"), "Cyrillic prints as ?: {text}");
    }

    #[test]
    fn a_long_trail_spills_onto_more_pages() {
        let original = crate::testing::sample_pdf(&["One"]);
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
