use ironflow::{engine::types::Context, nodes::NodeRegistry};
use lopdf::{
    Document, Object, ObjectId, Stream,
    content::{Content, Operation},
    dictionary,
};
use serde_json::json;

pub fn operation(name: &str, operands: Vec<Object>) -> Operation {
    Operation::new(name, operands)
}

pub fn show(text: &str) -> Operation {
    operation("Tj", vec![Object::string_literal(text)])
}

pub fn marked(value: Object) -> Operation {
    operation(
        "BDC",
        vec![
            Object::Name(b"Span".to_vec()),
            Object::Dictionary(dictionary! {"ActualText" => value}),
        ],
    )
}

pub fn document(mut operations: Vec<Operation>) -> (Document, ObjectId, ObjectId) {
    let mut doc = Document::new();
    let parent = doc.new_object_id();
    let font = doc.add_object(dictionary! {"Type"=>"Font", "Subtype"=>"Type1", "BaseFont"=>"Helvetica", "Encoding"=>"WinAnsiEncoding"});
    let resources = doc.add_object(dictionary! {"Font"=>dictionary! {"F1"=>font}});
    operations.insert(
        0,
        operation("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
    );
    operations.insert(0, operation("BT", vec![]));
    operations.push(operation("ET", vec![]));
    let bytes = Content { operations }.encode().unwrap();
    let content = doc.add_object(Stream::new(dictionary! {}, bytes));
    let page = doc.add_object(dictionary! {"Type"=>"Page", "Parent"=>parent, "Resources"=>resources, "MediaBox"=>vec![0.into(),0.into(),600.into(),800.into()], "Contents"=>content});
    doc.objects.insert(
        parent,
        dictionary! {"Type"=>"Pages", "Kids"=>vec![Object::Reference(page)], "Count"=>1}.into(),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>parent});
    doc.trailer.set("Root", catalog);
    (doc, page, resources)
}

pub async fn extract(doc: &mut Document) -> anyhow::Result<String> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("source.pdf");
    doc.save(&path)?;
    let output = NodeRegistry::with_builtins()
        .get("extract_pdf")
        .unwrap()
        .execute(&json!({"path":path}), &Context::new())
        .await?;
    Ok(output["content"].as_str().unwrap().to_owned())
}
