//! Draws filled fields onto the PDF's own pages before the seal.
//!
//! Each page with fields gets one Form `XObject` that draws them, painted
//! after the page's original content. The original streams are wrapped in
//! `q`/`Q` so graphics state they leave behind can't move or restyle the
//! overlay, and the page gets its own copy of its resources so dicts shared
//! with other pages, or inherited from the page tree, are never written
//! through. lopdf's page helpers (`add_page_contents`,
//! `get_or_create_resources`, `add_xobject`, `get_page_resources`) do
//! neither, so nothing here uses them.

use std::collections::BTreeMap;

use anyhow::{Context as _, bail};
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

use crate::pdf::{self, Font};

/// The page size pdf.js and other viewers assume when `MediaBox` is missing.
const LETTER: [f64; 4] = [0.0, 0.0, 612.0, 792.0];
/// How far up the page tree inherited attributes are looked for.
const MAX_TREE_DEPTH: usize = 64;
/// The `XObject` name the overlay takes, with a number added if a page
/// already uses it.
const XOBJECT_NAME: &str = "BizkitFields";
const CONTENTS_SHAPE: &str = "the page's Contents isn't a stream or an array";

/// How a page turns for display, clockwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rotation {
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    /// `/Rotate` modulo 360, as pdf.js reads it. None unless it is a
    /// multiple of 90; pdf.js would show such a page unrotated, so uploads
    /// refuse it rather than guess.
    const fn from_degrees(degrees: i64) -> Option<Self> {
        match degrees.rem_euclid(360) {
            0 => Some(Self::R0),
            90 => Some(Self::R90),
            180 => Some(Self::R180),
            270 => Some(Self::R270),
            _ => None,
        }
    }
}

/// The visible box of a page in user space: `CropBox` intersected with
/// `MediaBox`, both resolved through inherited attributes, and its
/// rotation. `x0 < x1` and `y0 < y1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PageBox {
    pub(crate) x0: f64,
    pub(crate) y0: f64,
    pub(crate) x1: f64,
    pub(crate) y1: f64,
    pub(crate) rotate: Rotation,
}

impl PageBox {
    /// Width and height as displayed, after rotation.
    fn display_size(&self) -> (f64, f64) {
        let (width, height) = (self.x1 - self.x0, self.y1 - self.y0);
        match self.rotate {
            Rotation::R0 | Rotation::R180 => (width, height),
            Rotation::R90 | Rotation::R270 => (height, width),
        }
    }
}

/// A field's place: fractions of the displayed page, origin top left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

/// What a filled field shows.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Mark<'a> {
    /// A drawn signature or initials, as pad strokes.
    Strokes(&'a [Vec<[u16; 2]>]),
    /// A typed signature or initials.
    Typed(&'a str),
    /// A text field's text.
    Text(&'a str),
    /// A date field's `YYYY-MM-DD`.
    Date(&'a str),
    /// A ticked checkbox.
    Check,
}

/// A field to draw on page `page` (from 1).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Filled<'a> {
    pub(crate) page: u32,
    pub(crate) rect: Rect,
    pub(crate) mark: Mark<'a>,
}

fn resolve<'a>(doc: &'a Document, object: &'a Object) -> anyhow::Result<&'a Object> {
    doc.dereference(object).map(|(_, object)| object).context("following a reference")
}

/// `key` on the page, or on the nearest Pages node above it that has it.
fn inherited<'a>(
    doc: &'a Document,
    page_id: ObjectId,
    key: &[u8],
) -> anyhow::Result<Option<&'a Object>> {
    let mut node = doc.get_dictionary(page_id).context("reading a page")?;
    for _ in 0..MAX_TREE_DEPTH {
        if let Ok(value) = node.get(key) {
            return resolve(doc, value).map(Some);
        }
        match node.get(b"Parent") {
            Ok(Object::Reference(parent)) => {
                node = doc.get_dictionary(*parent).context("reading a page tree node")?;
            }
            _ => return Ok(None),
        }
    }
    bail!("the page tree is deeper than {MAX_TREE_DEPTH} levels")
}

/// A rectangle array, normalized so the first corner is the lower left.
/// None unless it has four numbers and a positive area.
fn rectangle(doc: &Document, object: &Object) -> Option<[f64; 4]> {
    let items = resolve(doc, object).ok()?.as_array().ok()?;
    let numbers: Vec<f64> = items
        .iter()
        .map(|item| {
            let number = f64::from(resolve(doc, item).ok()?.as_float().ok()?);
            number.is_finite().then_some(number)
        })
        .collect::<Option<_>>()?;
    let [a, b, c, d] = numbers[..] else { return None };
    let rect = [a.min(c), b.min(d), a.max(c), b.max(d)];
    (rect[2] > rect[0] && rect[3] > rect[1]).then_some(rect)
}

fn intersect(a: [f64; 4], b: [f64; 4]) -> Option<[f64; 4]> {
    let rect = [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])];
    (rect[2] > rect[0] && rect[3] > rect[1]).then_some(rect)
}

/// The page's visible box and rotation, as pdf.js shows it. A missing
/// `MediaBox` is US Letter and a `CropBox` that doesn't overlap it is
/// ignored, as in pdf.js; a `MediaBox` that isn't a rectangle, or a
/// rotation that isn't a multiple of 90, is an error.
pub(crate) fn page_box(doc: &Document, page_id: ObjectId) -> anyhow::Result<PageBox> {
    let media = match inherited(doc, page_id, b"MediaBox")? {
        None => LETTER,
        Some(object) => rectangle(doc, object).context("the page's MediaBox isn't a rectangle")?,
    };
    let view = inherited(doc, page_id, b"CropBox")?
        .and_then(|object| rectangle(doc, object))
        .and_then(|crop| intersect(crop, media))
        .unwrap_or(media);
    let rotate = match inherited(doc, page_id, b"Rotate")? {
        None => Rotation::R0,
        Some(object) => rotation(object).context("the page's Rotate isn't a multiple of 90")?,
    };
    let [x0, y0, x1, y1] = view;
    Ok(PageBox { x0, y0, x1, y1, rotate })
}

fn rotation(object: &Object) -> Option<Rotation> {
    if let Object::Integer(degrees) = object {
        return Rotation::from_degrees(*degrees);
    }
    let degrees = f64::from(object.as_float().ok()?).rem_euclid(360.0);
    [(0.0, Rotation::R0), (90.0, Rotation::R90), (180.0, Rotation::R180), (270.0, Rotation::R270)]
        .into_iter()
        .find_map(|(at, rotation)| ((degrees - at).abs() < 1e-6).then_some(rotation))
}

/// The field's size in points, as displayed.
pub(crate) fn field_size(page: &PageBox, rect: &Rect) -> (f64, f64) {
    let (width, height) = page.display_size();
    (rect.width * width, rect.height * height)
}

/// The `cm` matrix that maps the field's own box (0 to its width and
/// height in points, y up, upright as displayed) onto `rect` on this page
/// in user space. `rect` is in display space, so the matrix turns the box
/// back against the page's rotation.
pub(crate) fn field_matrix(page: &PageBox, rect: &Rect) -> [f64; 6] {
    let (display_width, display_height) = page.display_size();
    let (_, height) = field_size(page, rect);
    let left = rect.x * display_width;
    // The field's bottom edge, measured down from the displayed top.
    let bottom = rect.y.mul_add(display_height, height);
    let PageBox { x0, y0, x1, y1, .. } = *page;
    match page.rotate {
        Rotation::R0 => [1.0, 0.0, 0.0, 1.0, x0 + left, y1 - bottom],
        Rotation::R90 => [0.0, 1.0, -1.0, 0.0, x0 + bottom, y0 + left],
        Rotation::R180 => [-1.0, 0.0, 0.0, -1.0, x1 - left, y0 + bottom],
        Rotation::R270 => [0.0, -1.0, 1.0, 0.0, x1 - bottom, y1 - left],
    }
}

/// One field's operators: its matrix, a clip to its box, and the mark.
fn draw(page: &PageBox, field: &Filled<'_>) -> Vec<Operation> {
    let (width, height) = field_size(page, &field.rect);
    let matrix = field_matrix(page, &field.rect);
    let mut ops = vec![
        Operation::new("q", vec![]),
        Operation::new("cm", matrix.iter().map(|n| Object::from(*n)).collect()),
        Operation::new("re", vec![0.into(), 0.into(), width.into(), height.into()]),
        Operation::new("W", vec![]),
        Operation::new("n", vec![]),
    ];
    let area = (0.0, 0.0, width, height);
    ops.extend(match field.mark {
        Mark::Strokes(strokes) => pdf::strokes(strokes, area),
        Mark::Typed(text) => pdf::fitted_text(Font::Italic, text, area),
        Mark::Text(text) | Mark::Date(text) => pdf::fitted_text(Font::Regular, text, area),
        Mark::Check => tick(area),
    });
    ops.push(Operation::new("Q", vec![]));
    ops
}

/// A stroked tick in the largest square that fits, centred.
fn tick((x, y, width, height): (f64, f64, f64, f64)) -> Vec<Operation> {
    let side = width.min(height);
    let (left, bottom) = (x + (width - side) / 2.0, y + (height - side) / 2.0);
    let point = |u: f64, v: f64| vec![u.mul_add(side, left).into(), v.mul_add(side, bottom).into()];
    vec![
        Operation::new("q", vec![]),
        Operation::new("w", vec![(side * 0.1).into()]),
        Operation::new("J", vec![1.into()]),
        Operation::new("j", vec![1.into()]),
        Operation::new("m", point(0.18, 0.52)),
        Operation::new("l", point(0.42, 0.26)),
        Operation::new("l", point(0.84, 0.8)),
        Operation::new("S", vec![]),
        Operation::new("Q", vec![]),
    ]
}

/// The page's content streams, from a stream reference, an array, or a
/// reference to an array.
fn content_streams(doc: &Document, page_id: ObjectId) -> anyhow::Result<Vec<Object>> {
    let page = doc.get_dictionary(page_id).context("reading a page")?;
    let Ok(contents) = page.get(b"Contents") else { return Ok(Vec::new()) };
    let items = if let Object::Reference(id) = contents {
        let target = doc.get_object(*id).context("reading the page's Contents")?;
        if matches!(target, Object::Stream(_)) {
            return Ok(vec![Object::Reference(*id)]);
        }
        target.as_array().context(CONTENTS_SHAPE)?.clone()
    } else {
        contents.as_array().context(CONTENTS_SHAPE)?.clone()
    };
    Ok(items.into_iter().filter(|item| matches!(item, Object::Reference(_))).collect())
}

/// How many `q` the original content leaves open after its lowest point,
/// and how many `Q` it pops past its start. Content that can't be decoded
/// counts as balanced, which is what most files are.
fn unbalanced(doc: &Document, streams: &[Object]) -> (usize, usize) {
    let mut bytes = Vec::new();
    for item in streams {
        let Ok(stream) = resolve(doc, item).and_then(|o| o.as_stream().context("a content stream"))
        else {
            continue;
        };
        let Ok(content) = stream.get_plain_content() else { return (0, 0) };
        bytes.extend_from_slice(&content);
        bytes.push(b'\n');
    }
    let Ok(content) = Content::decode(&bytes) else { return (0, 0) };
    let (mut depth, mut lowest) = (0_i64, 0_i64);
    for op in &content.operations {
        match op.operator.as_str() {
            "q" => depth += 1,
            "Q" => {
                depth -= 1;
                lowest = lowest.min(depth);
            }
            _ => {}
        }
    }
    let extra_close = usize::try_from(-lowest).unwrap_or(0);
    let open = usize::try_from(depth - lowest).unwrap_or(0);
    (open, extra_close)
}

fn operators(name: &str, count: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(count * 2);
    for _ in 0..count {
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(b'\n');
    }
    bytes
}

/// `[q-stream, ...original, Q-stream, Do-stream]`. The opening stream
/// pushes enough `q` that the original's unmatched `Q` never pops past
/// it, and the closing one pops whatever the original left open, so the
/// overlay draws in the page's initial state.
fn wrapped_contents(
    doc: &mut Document,
    page_id: ObjectId,
    name: &str,
) -> anyhow::Result<Vec<Object>> {
    let original = content_streams(doc, page_id)?;
    let (open, extra_close) = unbalanced(doc, &original);
    let pushes = 1 + extra_close;
    let open_stream = doc.add_object(Stream::new(dictionary! {}, operators("q", pushes)));
    let close_stream = doc.add_object(Stream::new(dictionary! {}, operators("Q", 1 + open)));
    let draw = Content {
        operations: vec![Operation::new("Do", vec![Object::Name(name.as_bytes().to_vec())])],
    }
    .encode()
    .context("encoding the overlay call")?;
    let draw_stream = doc.add_object(Stream::new(dictionary! {}, draw));
    let mut contents = vec![Object::Reference(open_stream)];
    contents.extend(original);
    contents.extend([Object::Reference(close_stream), Object::Reference(draw_stream)]);
    Ok(contents)
}

/// A copy of the page's resources, its own or inherited, with a copy of
/// its `/XObject` dict holding `form` under a name the page doesn't use.
fn own_resources(
    doc: &Document,
    page_id: ObjectId,
    form: ObjectId,
) -> anyhow::Result<(Dictionary, String)> {
    let mut resources = match inherited(doc, page_id, b"Resources")? {
        Some(Object::Dictionary(dict)) => dict.clone(),
        Some(_) => bail!("the page's Resources isn't a dictionary"),
        None => Dictionary::new(),
    };
    let mut xobjects = match resources.get(b"XObject") {
        Ok(object) => resolve(doc, object)?
            .as_dict()
            .context("the page's XObject resources aren't a dictionary")?
            .clone(),
        Err(_) => Dictionary::new(),
    };
    let name = (0..1000)
        .map(|n| if n == 0 { XOBJECT_NAME.to_owned() } else { format!("{XOBJECT_NAME}{n}") })
        .find(|name| !xobjects.has(name.as_bytes()))
        .context("no free XObject name")?;
    xobjects.set(name.as_bytes().to_vec(), Object::Reference(form));
    resources.set("XObject", xobjects);
    Ok((resources, name))
}

/// Draws every filled field onto its page.
pub(crate) fn apply(doc: &mut Document, fields: &[Filled<'_>]) -> anyhow::Result<()> {
    let mut by_page: BTreeMap<u32, Vec<&Filled<'_>>> = BTreeMap::new();
    for field in fields {
        by_page.entry(field.page).or_default().push(field);
    }
    if by_page.is_empty() {
        return Ok(());
    }
    let pages = doc.get_pages();
    let fonts = pdf::font_resources(doc);
    for (number, filled) in by_page {
        let page_id =
            *pages.get(&number).with_context(|| format!("the PDF has no page {number}"))?;
        let page = page_box(doc, page_id).with_context(|| format!("reading page {number}"))?;
        let operations: Vec<Operation> =
            filled.iter().flat_map(|field| draw(&page, field)).collect();
        let content = Content { operations }.encode().context("encoding the fields")?;
        let bbox = vec![page.x0.into(), page.y0.into(), page.x1.into(), page.y1.into()];
        let form = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form", "BBox" => bbox, "Resources" => fonts,
            },
            content,
        ));
        let (resources, name) = own_resources(doc, page_id, form)?;
        let contents = wrapped_contents(doc, page_id, &name)?;
        let dict = doc.get_dictionary_mut(page_id).context("reading a page")?;
        dict.set("Resources", resources);
        dict.set("Contents", contents);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use lopdf::StringFormat;

    use super::*;
    use crate::testing::{SampleOptions, sample_pdf_with};

    fn load(options: &SampleOptions, pages: &[&str]) -> Document {
        Document::load_mem(&sample_pdf_with(pages, options)).unwrap()
    }

    fn page_id(doc: &Document, number: u32) -> ObjectId {
        doc.get_pages()[&number]
    }

    fn stream_ops(doc: &Document, object: &Object) -> Vec<Operation> {
        let stream = resolve(doc, object).unwrap().as_stream().unwrap();
        Content::decode(&stream.get_plain_content().unwrap()).unwrap().operations
    }

    fn operators_of(ops: &[Operation]) -> Vec<&str> {
        ops.iter().map(|op| op.operator.as_str()).collect()
    }

    fn numbers(op: &Operation) -> Vec<f64> {
        op.operands.iter().map(|n| f64::from(n.as_float().unwrap())).collect()
    }

    /// The page's `/Contents` items, and the overlay `XObject`'s operators.
    fn applied(
        doc: &mut Document,
        number: u32,
        fields: &[Filled<'_>],
    ) -> (Vec<Object>, Vec<Operation>) {
        apply(doc, fields).unwrap();
        let page = doc.get_dictionary(page_id(doc, number)).unwrap();
        let contents = page.get(b"Contents").unwrap().as_array().unwrap().clone();
        let resources = page.get(b"Resources").unwrap().as_dict().unwrap();
        let xobjects = resources.get(b"XObject").unwrap().as_dict().unwrap();
        let form = xobjects.get(b"BizkitFields").unwrap().clone();
        let form_stream = resolve(doc, &form).unwrap().as_stream().unwrap();
        assert_eq!(form_stream.dict.get(b"Subtype").unwrap().as_name().unwrap(), b"Form");
        let ops = stream_ops(doc, &form);
        (contents, ops)
    }

    const RECT: Rect = Rect { x: 0.1, y: 0.2, width: 0.25, height: 0.05 };

    #[test]
    fn rotation_follows_pdf_js() {
        for (degrees, rotation) in [
            (0, Rotation::R0),
            (90, Rotation::R90),
            (180, Rotation::R180),
            (270, Rotation::R270),
            (-90, Rotation::R270),
            (450, Rotation::R90),
            (720, Rotation::R0),
        ] {
            assert_eq!(Rotation::from_degrees(degrees), Some(rotation), "{degrees}");
        }
        assert_eq!(Rotation::from_degrees(45), None);
    }

    #[test]
    fn boxes_are_inherited_and_cropped() {
        let options = SampleOptions {
            crop_box: Some([50.0, 100.0, 562.0, 742.0]),
            tree_rotate: Some(-90),
            rotate: vec![None, Some(450)],
            ..SampleOptions::default()
        };
        let doc = load(&options, &["One", "Two"]);
        let first = page_box(&doc, page_id(&doc, 1)).unwrap();
        assert_eq!(
            first,
            PageBox { x0: 50.0, y0: 100.0, x1: 562.0, y1: 742.0, rotate: Rotation::R270 }
        );
        assert_eq!(page_box(&doc, page_id(&doc, 2)).unwrap().rotate, Rotation::R90);

        let outside =
            SampleOptions { crop_box: Some([700.0, 0.0, 800.0, 10.0]), ..SampleOptions::default() };
        let off_page = load(&outside, &["One"]);
        let page = page_box(&off_page, page_id(&off_page, 1)).unwrap();
        assert_eq!(
            (page.x0, page.y0, page.x1, page.y1),
            (0.0, 0.0, 612.0, 792.0),
            "a crop off the page"
        );

        let odd = SampleOptions { rotate: vec![Some(45)], ..SampleOptions::default() };
        let turned = load(&odd, &["One"]);
        assert!(page_box(&turned, page_id(&turned, 1)).is_err());
    }

    #[test]
    fn the_matrix_turns_the_field_back_for_each_rotation() {
        // A 612 x 792 page with a CropBox offset by (50, 100).
        let page = |rotate| PageBox { x0: 50.0, y0: 100.0, x1: 562.0, y1: 742.0, rotate };
        let close = |a: [f64; 6], b: [f64; 6]| a.iter().zip(&b).all(|(a, b)| (a - b).abs() < 1e-3);
        // Unrotated: displayed 512 x 642. The field is 128 x 32.1 points, its
        // left edge 51.2 in and its bottom 160.5 down from the top.
        let r0 = field_matrix(&page(Rotation::R0), &RECT);
        assert!(close(r0, [1.0, 0.0, 0.0, 1.0, 101.2, 581.5]), "{r0:?}");
        let (width, height) = field_size(&page(Rotation::R0), &RECT);
        assert!((width - 128.0).abs() < 1e-3 && (height - 32.1).abs() < 1e-3);
        // Turned 90 clockwise: displayed 642 x 512. Display right is user up
        // and display down is user right, so the top left is (x0, y0).
        let r90 = field_matrix(&page(Rotation::R90), &RECT);
        assert!(close(r90, [0.0, 1.0, -1.0, 0.0, 50.0 + 128.0, 100.0 + 64.2]), "{r90:?}");
        let r180 = field_matrix(&page(Rotation::R180), &RECT);
        assert!(close(r180, [-1.0, 0.0, 0.0, -1.0, 562.0 - 51.2, 100.0 + 160.5]), "{r180:?}");
        let r270 = field_matrix(&page(Rotation::R270), &RECT);
        assert!(close(r270, [0.0, -1.0, 1.0, 0.0, 562.0 - 128.0, 742.0 - 64.2]), "{r270:?}");
        for rotate in [Rotation::R90, Rotation::R270] {
            let (across, down) = field_size(&page(rotate), &RECT);
            assert!((across - 160.5).abs() < 1e-3 && (down - 25.6).abs() < 1e-3);
        }
    }

    #[test]
    fn the_overlay_draws_in_its_own_xobject_after_the_wrapped_original() {
        let mut doc =
            load(&SampleOptions { rotate: vec![Some(90)], ..SampleOptions::default() }, &["One"]);
        let original =
            doc.get_dictionary(page_id(&doc, 1)).unwrap().get(b"Contents").unwrap().clone();
        let strokes = vec![vec![[0, 0], [1000, 300]]];
        let fields = [
            Filled { page: 1, rect: RECT, mark: Mark::Text("Acme (Ltd)") },
            Filled { page: 1, rect: RECT, mark: Mark::Typed("Ada") },
            Filled { page: 1, rect: RECT, mark: Mark::Date("2026-11-24") },
            Filled { page: 1, rect: RECT, mark: Mark::Strokes(&strokes) },
            Filled { page: 1, rect: RECT, mark: Mark::Check },
        ];
        let (contents, ops) = applied(&mut doc, 1, &fields);
        assert_eq!(contents.len(), 4);
        assert_eq!(contents[1], original, "the original stays between q and Q");
        assert_eq!(operators_of(&stream_ops(&doc, &contents[0])), ["q"]);
        assert_eq!(operators_of(&stream_ops(&doc, &contents[2])), ["Q"]);
        let call = stream_ops(&doc, &contents[3]);
        assert_eq!(operators_of(&call), ["Do"]);
        assert_eq!(call[0].operands[0].as_name().unwrap(), b"BizkitFields");

        let matrices: Vec<Vec<f64>> =
            ops.iter().filter(|op| op.operator == "cm").map(numbers).collect();
        assert_eq!(matrices.len(), 5);
        let expected = field_matrix(&page_box(&doc, page_id(&doc, 1)).unwrap(), &RECT);
        let same = |m: &Vec<f64>| m.iter().zip(&expected).all(|(a, b)| (a - b).abs() < 1e-3);
        assert!(matrices.iter().all(same), "{matrices:?}");
        let texts: Vec<(&[u8], &StringFormat)> = ops
            .iter()
            .filter(|op| op.operator == "Tj")
            .map(|op| {
                let Object::String(bytes, format) = &op.operands[0] else { panic!("{op:?}") };
                (bytes.as_slice(), format)
            })
            .collect();
        let strings: Vec<&[u8]> = texts.iter().map(|(bytes, _)| *bytes).collect();
        assert_eq!(strings, [&b"Acme (Ltd)"[..], b"Ada", b"2026-11-24"]);
        assert!(texts.iter().all(|(_, format)| **format == StringFormat::Hexadecimal));
        let fonts: Vec<&[u8]> = ops
            .iter()
            .filter(|op| op.operator == "Tf")
            .map(|op| op.operands[0].as_name().unwrap())
            .collect();
        assert_eq!(fonts, [&b"F1"[..], b"F3", b"F1"], "typed signatures are oblique");
        assert!(ops.iter().filter(|op| op.operator == "S").count() >= 2, "strokes and the tick");

        let mut saved = Vec::new();
        doc.save_to(&mut saved).unwrap();
        let hex = saved.windows(22).any(|w| w == b"<41636D6520284C746429>");
        assert!(hex, "text goes out as a hex string");
    }

    #[test]
    fn shared_and_inherited_resources_are_copied_not_written_through() {
        let options = SampleOptions {
            direct_resources: true,
            shared_xobject: true,
            ..SampleOptions::default()
        };
        let mut doc = load(&options, &["One", "Two"]);
        let pages_id = doc.catalog().unwrap().get(b"Pages").unwrap().as_reference().unwrap();
        let tree_before = doc.get_dictionary(pages_id).unwrap().clone();
        let xobject_id = tree_before
            .get(b"Resources")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"XObject")
            .unwrap()
            .as_reference()
            .unwrap();
        let shared_before = doc.get_dictionary(xobject_id).unwrap().clone();

        let (_, ops) = applied(&mut doc, 2, &[Filled { page: 2, rect: RECT, mark: Mark::Check }]);
        assert!(operators_of(&ops).contains(&"S"));
        assert_eq!(
            doc.get_dictionary(pages_id).unwrap(),
            &tree_before,
            "the Pages node is unchanged"
        );
        assert_eq!(
            doc.get_dictionary(xobject_id).unwrap(),
            &shared_before,
            "the shared XObject dict too"
        );
        let first = doc.get_dictionary(page_id(&doc, 1)).unwrap();
        assert!(first.get(b"Resources").is_err(), "page 1 still inherits");
        let second = doc.get_dictionary(page_id(&doc, 2)).unwrap();
        let resources = second.get(b"Resources").unwrap().as_dict().unwrap();
        assert!(resources.has(b"Font"), "the inherited fonts come along");
        let xobjects = resources.get(b"XObject").unwrap().as_dict().unwrap();
        assert!(xobjects.has(b"Logo") && xobjects.has(b"BizkitFields"), "{xobjects:?}");
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let mut doc =
            load(&SampleOptions { shared_xobject: true, ..SampleOptions::default() }, &["One"]);
        let pages_id = doc.catalog().unwrap().get(b"Pages").unwrap().as_reference().unwrap();
        let resources_id = doc
            .get_dictionary(pages_id)
            .unwrap()
            .get(b"Resources")
            .unwrap()
            .as_reference()
            .unwrap();
        let xobject_id = doc
            .get_dictionary(resources_id)
            .unwrap()
            .get(b"XObject")
            .unwrap()
            .as_reference()
            .unwrap();
        let logo = doc.get_dictionary(xobject_id).unwrap().get(b"Logo").unwrap().clone();
        doc.get_dictionary_mut(xobject_id).unwrap().set("BizkitFields", logo);
        apply(&mut doc, &[Filled { page: 1, rect: RECT, mark: Mark::Check }]).unwrap();
        let page = doc.get_dictionary(page_id(&doc, 1)).unwrap();
        let resources = page.get(b"Resources").unwrap().as_dict().unwrap();
        let xobjects = resources.get(b"XObject").unwrap().as_dict().unwrap();
        assert!(xobjects.has(b"BizkitFields1"));
        let contents = page.get(b"Contents").unwrap().as_array().unwrap();
        let call = stream_ops(&doc, contents.last().unwrap());
        assert_eq!(call[0].operands[0].as_name().unwrap(), b"BizkitFields1");
    }

    #[test]
    fn contents_through_a_reference_to_an_array_are_resolved() {
        let mut doc =
            load(&SampleOptions { contents_array_ref: true, ..SampleOptions::default() }, &["One"]);
        let page = doc.get_dictionary(page_id(&doc, 1)).unwrap();
        let array_id = page.get(b"Contents").unwrap().as_reference().unwrap();
        let original = doc.get_object(array_id).unwrap().as_array().unwrap().clone();
        assert_eq!(original.len(), 2);
        let (contents, _) =
            applied(&mut doc, 1, &[Filled { page: 1, rect: RECT, mark: Mark::Check }]);
        assert_eq!(contents.len(), 5);
        assert_eq!(contents[1..3], original[..], "both original streams, in order");
        assert_eq!(
            doc.get_object(array_id).unwrap().as_array().unwrap(),
            &original,
            "the array is unchanged"
        );
    }

    #[test]
    fn an_unbalanced_q_is_closed_before_the_overlay() {
        let mut doc =
            load(&SampleOptions { unbalanced_q: true, ..SampleOptions::default() }, &["One"]);
        let (contents, _) =
            applied(&mut doc, 1, &[Filled { page: 1, rect: RECT, mark: Mark::Check }]);
        assert_eq!(operators_of(&stream_ops(&doc, &contents[0])), ["q"]);
        assert_eq!(
            operators_of(&stream_ops(&doc, &contents[2])),
            ["Q", "Q"],
            "ours and the original's"
        );
    }

    #[test]
    fn an_extra_q_close_cant_pop_ours() {
        let streams = [Object::Reference((1, 0))];
        let mut doc = Document::with_version("1.7");
        doc.objects.insert(
            (1, 0),
            Object::Stream(Stream::new(dictionary! {}, b"Q Q q 1 0 0 1 5 5 cm".to_vec())),
        );
        assert_eq!(
            unbalanced(&doc, &streams),
            (1, 2),
            "two pops past the start, then one push left open"
        );
    }

    #[test]
    fn a_page_without_fields_is_left_alone() {
        let mut doc = load(&SampleOptions::default(), &["One", "Two"]);
        let before = doc.get_dictionary(page_id(&doc, 1)).unwrap().clone();
        apply(&mut doc, &[Filled { page: 2, rect: RECT, mark: Mark::Check }]).unwrap();
        assert_eq!(doc.get_dictionary(page_id(&doc, 1)).unwrap(), &before);
        assert!(apply(&mut doc, &[Filled { page: 3, rect: RECT, mark: Mark::Check }]).is_err());
    }
}
