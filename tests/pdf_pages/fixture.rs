use lopdf::{Document, Object, ObjectId, Stream, dictionary};

pub fn inherited_document() -> Document {
    let mut document = Document::new();
    let root = document.new_object_id();
    let branch = document.new_object_id();
    let root_resources = resources(&mut document, "Times-Roman", "ROOT_ONLY_RESOURCE");
    let inherited_resources = resources(&mut document, "Helvetica", "SELECTED_RESOURCE");
    let private_resources = resources(&mut document, "Courier", "PRIVATE_RESOURCE_TWO");
    let mut pages = Vec::new();
    for label in ["SELECTED_PAGE_ONE", "PRIVATE_PAGE_TWO"] {
        let content = document.add_object(Stream::new(
            dictionary! {},
            format!("0.1 0.5 0.8 rg 30 30 70 40 re f q 20 0 0 20 120 30 cm /Im1 Do Q 0 g BT /F1 9 Tf 20 110 Td ({label}) Tj ET")
                .into_bytes(),
        ));
        pages.push(document.add_object(dictionary! {
            "Type" => "Page", "Parent" => branch, "Contents" => content,
        }));
    }
    let second = document.get_dictionary_mut(pages[1]).unwrap();
    second.set("Resources", private_resources);
    second.set("MediaBox", rectangle(300, 200));
    second.set("CropBox", rectangle(300, 200));
    second.set("Rotate", 0);
    document.objects.insert(
        branch,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Parent" => root,
            "Kids" => pages.iter().copied().map(Object::Reference).collect::<Vec<_>>(),
            "Count" => 2, "Resources" => inherited_resources, "Rotate" => 90,
        }),
    );
    document.objects.insert(
        root,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![Object::Reference(branch)], "Count" => 2,
            "Resources" => root_resources, "MediaBox" => rectangle(240, 160),
            "CropBox" => vec![10.into(), 10.into(), 230.into(), 150.into()], "Rotate" => 180,
        }),
    );
    let metadata = document.add_object(Stream::new(
        dictionary! {},
        b"DOCUMENT_ONLY_SECRET".to_vec(),
    ));
    let catalog = document.add_object(dictionary! {
        "Type" => "Catalog", "Pages" => root, "Metadata" => metadata,
    });
    document.trailer.set("Root", catalog);
    document
}

pub fn rectangle(width: i64, height: i64) -> Object {
    Object::Array(vec![0.into(), 0.into(), width.into(), height.into()])
}

fn resources(document: &mut Document, font: &str, marker: &str) -> ObjectId {
    let font = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => font,
        "FixtureMarker" => Object::string_literal(marker),
    });
    let image = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
            "FixtureMarker" => Object::string_literal(marker),
        },
        vec![40, 170, 80],
    ));
    document.add_object(dictionary! {
        "Font" => dictionary! {"F1" => font}, "XObject" => dictionary! {"Im1" => image},
    })
}
