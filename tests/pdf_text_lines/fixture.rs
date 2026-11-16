//! A page laid out the way the macOS Quartz PDF writer does it: every run of
//! one font is its own `BT … Tm … Tf … Tj … ET` block, a simple TrueType font
//! carries `/Widths`, and a glyph outside its encoding comes from a Type0
//! Identity-H font with `/W` and `/DW`.

use ironflow::{engine::types::Context, nodes::NodeRegistry};
use lopdf::{
    Document, Object, Stream,
    content::{Content, Operation},
    dictionary,
};
use serde_json::json;

/// Every glyph of the simple font is half an em wide.
pub const GLYPH: f32 = 500.0;

const TO_UNICODE: &str = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap \
    /CMapName /Adobe-Identity-UCS def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> \
    endcodespacerange 2 beginbfchar <0001> <0103> <0002> <0219> endbfchar endcmap \
    CMapName currentdict /CMap defineresource pop end end";

pub fn operation(name: &str, operands: Vec<Object>) -> Operation {
    Operation::new(name, operands)
}

/// `F1` is the simple font, `F2` the composite one.
pub fn font(name: &str, size: f32) -> Operation {
    operation(
        "Tf",
        vec![Object::Name(name.as_bytes().to_vec()), size.into()],
    )
}

pub fn show(text: &str) -> Operation {
    operation("Tj", vec![Object::string_literal(text)])
}

/// Show glyphs of the composite font by CID.
pub fn show_cids(cids: &[u16]) -> Operation {
    let bytes = cids.iter().flat_map(|cid| cid.to_be_bytes()).collect();
    operation(
        "Tj",
        vec![Object::String(bytes, lopdf::StringFormat::Hexadecimal)],
    )
}

/// One Quartz-style block: a text matrix of scale `size` at (`x`, `y`), a
/// font of size 1 and one string.
pub fn block(x: f32, y: f32, size: f32, font_name: &str, text: Operation) -> Vec<Operation> {
    vec![
        operation("BT", vec![]),
        operation(
            "Tm",
            vec![
                size.into(),
                0.into(),
                0.into(),
                size.into(),
                x.into(),
                y.into(),
            ],
        ),
        font(font_name, 1.0),
        text,
        operation("ET", vec![]),
    ]
}

pub fn document(operations: Vec<Operation>) -> Document {
    let table = vec![
        1.into(),
        Object::Array(vec![GLYPH.into()]),
        2.into(),
        2.into(),
        250.into(),
    ];
    document_with(table, operations)
}

/// `table` is the `/W` array of the composite font.
pub fn document_with(table: Vec<Object>, operations: Vec<Operation>) -> Document {
    let mut doc = Document::new();
    let parent = doc.new_object_id();
    let widths = doc.add_object(Object::Array(vec![GLYPH.into(); 95]));
    let simple = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "AAAAAB+Sample",
        "Encoding" => "MacRomanEncoding", "FirstChar" => 32, "LastChar" => 126,
        "Widths" => widths,
    });
    let cid_widths = doc.add_object(Object::Array(table));
    let descendant = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType0", "BaseFont" => "AAAAAC+Sample",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"), "Supplement" => 0,
        },
        "W" => cid_widths, "DW" => 1000,
    });
    let to_unicode = doc.add_object(Stream::new(dictionary! {}, TO_UNICODE.as_bytes().to_vec()));
    let composite = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "AAAAAC+Sample",
        "Encoding" => "Identity-H", "DescendantFonts" => vec![Object::Reference(descendant)],
        "ToUnicode" => to_unicode,
    });
    // No `/Widths`: the advance of this font's text is unknown.
    let unmeasured = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let resources = doc.add_object(dictionary! {
        "Font" => dictionary! {"F1" => simple, "F2" => composite, "F3" => unmeasured},
    });
    let bytes = Content { operations }.encode().unwrap();
    let content = doc.add_object(Stream::new(dictionary! {}, bytes));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => parent, "Resources" => resources,
        "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()], "Contents" => content,
    });
    doc.objects.insert(
        parent,
        dictionary! {"Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1}
            .into(),
    );
    let catalog = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => parent});
    doc.trailer.set("Root", catalog);
    doc
}

pub async fn extract(operations: Vec<Operation>) -> String {
    extract_document(document(operations)).await
}

pub async fn extract_document(mut document: Document) -> String {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lines.pdf");
    document.save(&path).unwrap();
    let output = NodeRegistry::with_builtins()
        .get("extract_pdf")
        .unwrap()
        .execute(&json!({"path": path}), &Context::new())
        .await
        .unwrap();
    output["content"].as_str().unwrap().to_owned()
}
