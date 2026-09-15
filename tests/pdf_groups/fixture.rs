use lopdf::{Document, Object, Stream, dictionary};

pub fn document(count: u32) -> Document {
    let mut document = Document::new();
    let root = document.new_object_id();
    let font = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let image = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
        },
        vec![40, 170, 80],
    ));
    let resources = document.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font },
        "XObject" => dictionary! { "Im1" => image },
    });
    let mut pages = Vec::new();
    for number in 1..=count {
        let content = document.add_object(Stream::new(
            dictionary! {},
            format!("q 20 0 0 20 20 20 cm /Im1 Do Q BT /F1 12 Tf 20 100 Td (Page {number}) Tj ET")
                .into_bytes(),
        ));
        pages.push(document.add_object(dictionary! {
            "Type" => "Page", "Parent" => root, "Contents" => content,
        }));
    }
    document.objects.insert(root, Object::Dictionary(dictionary! {
        "Type" => "Pages", "Kids" => pages.into_iter().map(Object::Reference).collect::<Vec<_>>(),
        "Count" => count, "Resources" => resources,
        "MediaBox" => vec![0.into(), 0.into(), 240.into(), 160.into()],
        "CropBox" => vec![10.into(), 10.into(), 230.into(), 150.into()], "Rotate" => 90,
    }));
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => root });
    document.trailer.set("Root", catalog);
    document
}
