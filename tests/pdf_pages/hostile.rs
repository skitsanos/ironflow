//! Small PDFs that ask a renderer for unbounded work or memory.

use lopdf::{Document, Object, ObjectId, Stream, dictionary};

fn single_page(document: &mut Document, resources: lopdf::Dictionary, content: &[u8]) -> Vec<u8> {
    let pages = document.new_object_id();
    let content = document.add_object(Stream::new(dictionary! {}, content.to_vec()));
    let page = document.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages, "Contents" => content, "Resources" => resources,
        "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
    });
    document.objects.insert(
        pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1,
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

fn form(document: &mut Document, child: Option<ObjectId>) -> ObjectId {
    let mut dictionary = dictionary! {
        "Type" => "XObject", "Subtype" => "Form",
        "BBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
    };
    let content: &[u8] = match child {
        Some(child) => {
            dictionary.set(
                "Resources",
                dictionary! { "XObject" => dictionary! { "N" => child } },
            );
            b"/N Do /N Do"
        }
        None => b"0 0 5 5 re f",
    };
    document.add_object(Stream::new(dictionary, content.to_vec()))
}

/// `levels` nested forms, each drawing the next one twice: 2^levels fills
/// from a file of a few kilobytes.
pub fn exponential_forms(levels: usize) -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let mut child = None;
    for _ in 0..levels {
        child = Some(form(&mut document, child));
    }
    let resources = dictionary! { "XObject" => dictionary! { "N" => child.unwrap() } };
    single_page(&mut document, resources, b"/N Do")
}

/// A tiling pattern whose cell is a million points wide with a step of a
/// hundredth of a point: one fill asks for 10^16 tile copies.
pub fn dense_tiling() -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pattern = document.add_object(Stream::new(
        dictionary! {
            "Type" => "Pattern", "PatternType" => 1, "PaintType" => 1, "TilingType" => 1,
            "BBox" => vec![0.into(), 0.into(), 1_000_000.into(), 1_000_000.into()],
            "XStep" => Object::Real(0.01), "YStep" => Object::Real(0.01),
            "Resources" => dictionary! {},
        },
        b"0 0 1 1 re f".to_vec(),
    ));
    let resources = dictionary! { "Pattern" => dictionary! { "P" => pattern } };
    single_page(
        &mut document,
        resources,
        b"/Pattern cs /P scn 0 0 200 200 re f",
    )
}

/// An image that declares `side` by `side` RGB pixels over a tiny stream.
pub fn declared_image(side: i64) -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let image = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => side, "Height" => side,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
        },
        vec![0x7f; 64],
    ));
    let resources = dictionary! { "XObject" => dictionary! { "I" => image } };
    single_page(&mut document, resources, b"q 200 0 0 200 0 0 cm /I Do Q")
}

/// A page whose compressed content stream decodes to `decoded` bytes.
pub fn content_bomb(decoded: usize) -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pages = document.new_object_id();
    let mut stream = Stream::new(dictionary! {}, vec![b' '; decoded]);
    stream.compress().unwrap();
    let content = document.add_object(stream);
    let page = document.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages, "Contents" => content,
        "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
    });
    document.objects.insert(
        pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1,
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}
