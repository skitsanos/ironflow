use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use lopdf::{Document, Object, Stream, dictionary};

pub(super) fn write_compressed(output: &Path) -> Result<PathBuf> {
    let path = output.join("compressed-text.pdf");
    let mut document = Document::new();
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica"
    });
    let resources_id = document.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id }
    });
    let mut pages = Vec::new();
    for page in 0..24 {
        let text = format!("benchmark page {page} ").repeat(1_500);
        let mut stream = Stream::new(
            dictionary! {},
            format!("BT /F1 10 Tf 10 100 Td ({text}) Tj ET").into_bytes(),
        );
        stream.compress()?;
        let content_id = document.add_object(stream);
        pages.push(document.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Resources" => resources_id,
            "Contents" => content_id, "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()]
        }));
    }
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => pages.iter().copied().map(Object::Reference).collect::<Vec<_>>(),
            "Count" => pages.len() as u32
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    document.save(&path)?;
    Ok(path)
}

pub(super) fn write_cid_unicode(output: &Path) -> Result<PathBuf> {
    let path = output.join("cid-unicode.pdf");
    let fragments = [
        "Activ", "ă", " ", "Bucure", "ș", "ti", " ", "Ț", "ară", " ", "Ș", "tiin", "ț", "ă",
    ];
    let codes = character_codes(&fragments)?;
    let cmap = to_unicode_cmap(&codes);

    let mut document = Document::new();
    let pages_id = document.new_object_id();
    let to_unicode_id = document.add_object(Stream::new(dictionary! {}, cmap.into_bytes()));
    let descriptor_id = document.add_object(dictionary! {
        "Type" => "FontDescriptor", "FontName" => "IronFlowCID", "Flags" => 4,
        "FontBBox" => vec![0.into(), (-200).into(), 1000.into(), 900.into()],
        "ItalicAngle" => 0, "Ascent" => 800, "Descent" => -200,
        "CapHeight" => 700, "StemV" => 80
    });
    let cid_info = dictionary! {
        "Registry" => Object::string_literal("Adobe"),
        "Ordering" => Object::string_literal("Identity"), "Supplement" => 0
    };
    let descendant_id = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "IronFlowCID",
        "CIDSystemInfo" => cid_info, "FontDescriptor" => descriptor_id,
        "CIDToGIDMap" => "Identity", "DW" => 1000
    });
    let font_id = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "IronFlowCID",
        "Encoding" => "Identity-H", "DescendantFonts" => vec![descendant_id.into()],
        "ToUnicode" => to_unicode_id
    });
    let resources_id = document.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id }
    });
    let content_id = document.add_object(Stream::new(
        dictionary! {},
        fragmented_content(&fragments, &codes)?,
    ));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Resources" => resources_id,
        "Contents" => content_id, "MediaBox" => vec![0.into(), 0.into(), 600.into(), 200.into()]
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    document.save(&path)?;
    Ok(path)
}

fn character_codes(fragments: &[&str]) -> Result<BTreeMap<char, u16>> {
    let mut codes = BTreeMap::new();
    for character in fragments.concat().chars() {
        let next = u16::try_from(codes.len() + 1)?;
        codes.entry(character).or_insert(next);
    }
    Ok(codes)
}

fn to_unicode_cmap(codes: &BTreeMap<char, u16>) -> String {
    let mut mappings = codes
        .iter()
        .map(|(character, code)| (*code, *character))
        .collect::<Vec<_>>();
    mappings.sort_unstable_by_key(|(code, _)| *code);
    let mut cmap = format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /IronFlow-UCS def\n/CMapType 2 def\n1 begincodespacerange\n\
         <0000><ffff>\nendcodespacerange\n{} beginbfchar\n",
        mappings.len()
    );
    for (code, character) in mappings {
        let mut encoded = [0_u16; 2];
        let target = character
            .encode_utf16(&mut encoded)
            .iter()
            .map(|value| format!("{value:04x}"))
            .collect::<String>();
        cmap.push_str(&format!("<{code:04x}><{target}>\n"));
    }
    cmap.push_str("endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    cmap
}

fn fragmented_content(fragments: &[&str], codes: &BTreeMap<char, u16>) -> Result<Vec<u8>> {
    let mut operations = vec![
        lopdf::content::Operation::new("BT", vec![]),
        lopdf::content::Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
        lopdf::content::Operation::new("Td", vec![20.into(), 100.into()]),
    ];
    for fragment in fragments {
        let encoded = fragment
            .chars()
            .flat_map(|character| codes[&character].to_be_bytes())
            .collect::<Vec<_>>();
        operations.push(lopdf::content::Operation::new(
            "TJ",
            vec![Object::Array(vec![Object::String(
                encoded,
                lopdf::StringFormat::Hexadecimal,
            )])],
        ));
    }
    operations.push(lopdf::content::Operation::new("ET", vec![]));
    Ok(lopdf::content::Content { operations }.encode()?)
}
