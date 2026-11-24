//! Sample PDFs for tests and the demo seed.

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};

/// A small PDF with one page per entry of `pages`, each showing its text.
///
/// # Panics
///
/// Never: the document is built in memory from fixed parts.
#[must_use]
pub fn sample_pdf(pages: &[&str]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });
    let mut kids = Vec::new();
    for text in pages {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 14.into()]),
                Operation::new("Td", vec![72.into(), 720.into()]),
                Operation::new("Tj", vec![Object::string_literal(*text)]),
                Operation::new("ET", vec![]),
            ],
        };
        let encoded = content.encode().unwrap_or_default();
        let stream = doc.add_object(Stream::new(dictionary! {}, encoded));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => stream,
        });
        kids.push(Object::Reference(page));
    }
    let count = i64::try_from(kids.len()).unwrap_or(0);
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => count, "Resources" => resources,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    // Writing to a Vec can't fail.
    let _ignored = doc.save_to(&mut out);
    out
}
