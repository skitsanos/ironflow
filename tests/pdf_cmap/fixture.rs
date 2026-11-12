//! Generated Identity-H PDFs whose ToUnicode CMaps probe font-decoding bounds (IF-178).

use std::path::{Path, PathBuf};

use ironflow::{engine::types::Context, nodes::NodeRegistry};
use lopdf::{
    Document, Object, ObjectId, Stream, StringFormat,
    content::{Content, Operation},
    dictionary,
};
use serde_json::json;

/// A complete ToUnicode CMap around one codespace line and one mapping section.
pub fn to_unicode(codespace: &str, section: &str) -> String {
    format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n{codespace}\nendcodespacerange\n{section}\n\
         endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
    )
}

/// One `bfrange` from `low` to `high` that starts at `A`, over the full
/// codespace of the same code width.
pub fn range(low: &str, high: &str) -> String {
    let codespace = format!("<{}> <{}>", "0".repeat(low.len()), "F".repeat(high.len()));
    to_unicode(
        &codespace,
        &format!("1 beginbfrange\n<{low}> <{high}> <0041>\nendbfrange"),
    )
}

/// A one-byte font whose code `01` maps to `units` copies of `A`.
#[allow(dead_code)] // The serial binary only needs the range fixtures.
pub fn long_target(units: usize) -> String {
    to_unicode(
        "<00> <FF>",
        &format!("1 beginbfchar\n<01> <{}>\nendbfchar", "0041".repeat(units)),
    )
}

/// Select font `F<font>` and show one hexadecimal glyph string.
pub fn show(font: usize, codes: Vec<u8>) -> Vec<Operation> {
    vec![
        Operation::new(
            "Tf",
            vec![Object::Name(format!("F{font}").into_bytes()), 12.into()],
        ),
        Operation::new("Tj", vec![Object::String(codes, StringFormat::Hexadecimal)]),
    ]
}

fn type0_font(doc: &mut Document, cmap: &str) -> ObjectId {
    let mut to_unicode = Stream::new(dictionary! {}, cmap.as_bytes().to_vec());
    to_unicode.compress().unwrap();
    let to_unicode = doc.add_object(to_unicode);
    let descendant = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "IronFlowCID",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"), "Supplement" => 0
        },
        "CIDToGIDMap" => "Identity", "DW" => 1000
    });
    doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "IronFlowCID",
        "Encoding" => "Identity-H", "DescendantFonts" => vec![descendant.into()],
        "ToUnicode" => to_unicode
    })
}

/// Every page shares one resource dictionary holding fonts `F1..Fn`, one per
/// CMap, so a font used on several pages is one font object.
pub fn document(cmaps: &[String], pages: Vec<Vec<Operation>>) -> Document {
    let mut doc = Document::new();
    let parent = doc.new_object_id();
    let mut fonts = lopdf::Dictionary::new();
    for (index, cmap) in cmaps.iter().enumerate() {
        let font = type0_font(&mut doc, cmap);
        fonts.set(format!("F{}", index + 1), font);
    }
    let resources = doc.add_object(dictionary! { "Font" => fonts });
    let mut kids = Vec::new();
    for mut operations in pages {
        operations.insert(0, Operation::new("BT", vec![]));
        operations.push(Operation::new("ET", vec![]));
        let mut content = Stream::new(dictionary! {}, Content { operations }.encode().unwrap());
        content.compress().unwrap();
        let content = doc.add_object(content);
        kids.push(doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => parent, "Resources" => resources,
            "Contents" => content,
            "MediaBox" => vec![0.into(), 0.into(), 600.into(), 200.into()]
        }));
    }
    doc.objects.insert(
        parent,
        dictionary! {
            "Type" => "Pages",
            "Kids" => kids.iter().copied().map(Object::Reference).collect::<Vec<_>>(),
            "Count" => kids.len() as u32
        }
        .into(),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => parent });
    doc.trailer.set("Root", catalog);
    doc
}

/// The three one-page PDFs of the finding: `(low, high, glyph string)`.
pub fn range_pdf(low: &str, high: &str, codes: Vec<u8>) -> Document {
    document(&[range(low, high)], vec![show(1, codes)])
}

pub fn save(doc: &mut Document, directory: &Path) -> PathBuf {
    let path = directory.join("source.pdf");
    doc.save(&path).unwrap();
    path
}

pub async fn extract(doc: &mut Document) -> anyhow::Result<String> {
    let temp = tempfile::tempdir()?;
    let path = save(doc, temp.path());
    let output = NodeRegistry::with_builtins()
        .get("extract_pdf")
        .unwrap()
        .execute(&json!({"path": path}), &Context::new())
        .await?;
    Ok(output["content"].as_str().unwrap().to_owned())
}
