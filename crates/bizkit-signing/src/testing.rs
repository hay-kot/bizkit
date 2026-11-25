//! Sample PDFs for tests and the demo seed.

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, Stream, dictionary};

/// A small PDF with one page per entry of `pages`, each showing its text.
///
/// # Panics
///
/// Never: the document is built in memory from fixed parts.
#[must_use]
pub fn sample_pdf(pages: &[&str]) -> Vec<u8> {
    sample_pdf_with(pages, &SampleOptions::default())
}

/// [`sample_pdf`] at least `min_len` bytes long, for size limits. The
/// padding is an unused stream of spaces.
///
/// # Panics
///
/// Never: the document is built in memory from fixed parts.
#[must_use]
pub fn sample_pdf_padded(pages: &[&str], min_len: usize) -> Vec<u8> {
    sample_pdf_with(pages, &SampleOptions { min_len, ..SampleOptions::default() })
}

/// The page tree shapes [`sample_pdf_with`] can build.
///
/// They are the cases that trip up drawing onto existing pages. The
/// default is [`sample_pdf`]: a US Letter `MediaBox` and an indirect
/// `/Resources` on the Pages node, inherited by every page.
#[derive(Debug, Clone, Default)]
#[expect(clippy::struct_excessive_bools, reason = "each flag is one independent test shape")]
pub struct SampleOptions {
    /// `/Rotate` on each page, by index; pages past the end get none.
    pub rotate: Vec<Option<i64>>,
    /// `/Rotate` on the Pages node, inherited by pages without their own.
    pub tree_rotate: Option<i64>,
    /// A `CropBox` on the Pages node.
    pub crop_box: Option<[f32; 4]>,
    /// The Pages node's `/Resources` as a direct dict instead of a reference.
    pub direct_resources: bool,
    /// An indirect `/XObject` dict in the shared resources, holding one
    /// form named `Logo` that no page draws.
    pub shared_xobject: bool,
    /// Each page's `/Contents` as a reference to an array of two streams.
    pub contents_array_ref: bool,
    /// Each page's content changes the matrix and then opens a `q` it
    /// never closes.
    pub unbalanced_q: bool,
    /// Pad the file to at least this many bytes with an unused stream.
    pub min_len: usize,
}

/// A sample PDF shaped by `options`, one page per entry of `pages`.
///
/// # Panics
///
/// Never: the document is built in memory from fixed parts.
#[must_use]
pub fn sample_pdf_with(pages: &[&str], options: &SampleOptions) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let mut resources = dictionary! { "Font" => dictionary! { "F1" => font } };
    if options.shared_xobject {
        let logo = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 10.into(), 10.into()],
            },
            b"0 0 10 10 re f".to_vec(),
        ));
        let xobjects = doc.add_object(dictionary! { "Logo" => logo });
        resources.set("XObject", xobjects);
    }
    let resources = if options.direct_resources {
        Object::Dictionary(resources)
    } else {
        Object::Reference(doc.add_object(resources))
    };
    let mut kids = Vec::new();
    for (at, text) in pages.iter().enumerate() {
        let mut operations = Vec::new();
        if options.unbalanced_q {
            operations.extend([
                Operation::new(
                    "cm",
                    vec![0.5.into(), 0.into(), 0.into(), 0.5.into(), 0.into(), 0.into()],
                ),
                Operation::new("q", vec![]),
            ]);
        }
        operations.extend([
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 14.into()]),
            Operation::new("Td", vec![72.into(), 720.into()]),
            Operation::new("Tj", vec![Object::string_literal(*text)]),
            Operation::new("ET", vec![]),
        ]);
        let encoded = Content { operations }.encode().unwrap_or_default();
        let stream = doc.add_object(Stream::new(dictionary! {}, encoded));
        let contents = if options.contents_array_ref {
            let second = doc.add_object(Stream::new(dictionary! {}, b"0 0 m".to_vec()));
            Object::Reference(
                doc.add_object(vec![Object::Reference(stream), Object::Reference(second)]),
            )
        } else {
            Object::Reference(stream)
        };
        let mut page =
            dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => contents };
        if let Some(Some(rotate)) = options.rotate.get(at) {
            page.set("Rotate", *rotate);
        }
        kids.push(Object::Reference(doc.add_object(page)));
    }
    let count = i64::try_from(kids.len()).unwrap_or(0);
    let mut tree: Dictionary = dictionary! {
        "Type" => "Pages", "Kids" => kids, "Count" => count, "Resources" => resources,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    };
    if let Some(rotate) = options.tree_rotate {
        tree.set("Rotate", rotate);
    }
    if let Some(crop) = options.crop_box {
        tree.set("CropBox", crop.iter().map(|n| Object::Real(*n)).collect::<Vec<_>>());
    }
    doc.objects.insert(pages_id, Object::Dictionary(tree));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    if options.min_len > 0 {
        doc.add_object(Stream::new(dictionary! {}, vec![b' '; options.min_len]));
    }
    let mut out = Vec::new();
    // Writing to a Vec can't fail.
    let _ignored = doc.save_to(&mut out);
    out
}
