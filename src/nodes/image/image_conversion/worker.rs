use anyhow::Result;
use lopdf::{
    Document, Object, Stream,
    content::{Content, Operation},
    dictionary,
};

use crate::engine::types::NodeOutput;
use crate::util::execution::ExecutionControl;

use super::super::common::load_image_for_pdf;
use super::super::resource::ImageToPdfBudget;
use super::{Request, pdf_image};

pub(super) fn convert(request: Request, execution: &ExecutionControl) -> Result<NodeOutput> {
    let Request {
        sources,
        output_key,
        output_path,
        limits,
    } = request;
    execution.checkpoint()?;
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let mut page_ids = Vec::with_capacity(sources.len());
    let mut budget = ImageToPdfBudget::new(limits);

    for source in sources {
        execution.checkpoint()?;
        let loaded = load_image_for_pdf(
            source,
            limits.decode,
            budget.remaining_encoded_bytes(),
            execution,
        )?;
        let encoded_bytes = u64::try_from(loaded.bytes.len()).unwrap_or(u64::MAX);
        budget.admit(encoded_bytes, loaded.pixels)?;
        let width = loaded.width;
        let height = loaded.height;
        let image_stream = pdf_image::image_stream(loaded, limits.decode, execution)?;
        add_page(
            &mut document,
            pages_id,
            &mut page_ids,
            image_stream,
            width,
            height,
        )?;
    }

    finish_document(&mut document, pages_id, &page_ids, execution)?;
    document.save(&output_path).map_err(|error| {
        anyhow::anyhow!(
            "image_to_pdf: failed to save PDF '{}': {error:?}",
            output_path
        )
    })?;
    execution.checkpoint()?;
    Ok(output(&output_key, &output_path, page_ids.len()))
}

fn add_page(
    document: &mut Document,
    pages_id: lopdf::ObjectId,
    page_ids: &mut Vec<lopdf::ObjectId>,
    image_stream: Stream,
    width: u32,
    height: u32,
) -> Result<()> {
    let image_id = document.add_object(image_stream);
    let image_name = format!("X{}", image_id.0);
    let media_box = vec![
        0.into(),
        0.into(),
        i64::from(width).into(),
        i64::from(height).into(),
    ];
    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    width.into(),
                    0.into(),
                    0.into(),
                    height.into(),
                    0.into(),
                    0.into(),
                ],
            ),
            Operation::new("Do", vec![Object::Name(image_name.clone().into_bytes())]),
            Operation::new("Q", vec![]),
        ],
    };
    let content = content
        .encode()
        .map_err(|error| anyhow::anyhow!("image_to_pdf: failed to encode page: {error:?}"))?;
    let content_id = document.add_object(Stream::new(dictionary! {}, content));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => media_box,
    });
    document
        .add_xobject(page_id, image_name.as_bytes(), image_id)
        .map_err(|error| anyhow::anyhow!("image_to_pdf: failed to add image: {error:?}"))?;
    page_ids.push(page_id);
    Ok(())
}

fn finish_document(
    document: &mut Document,
    pages_id: lopdf::ObjectId,
    page_ids: &[lopdf::ObjectId],
    execution: &ExecutionControl,
) -> Result<()> {
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
        "Count" => page_ids.len() as u32,
    };
    document.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    execution.checkpoint()?;
    document.compress();
    execution.checkpoint()
}

fn output(output_key: &str, output_path: &str, image_count: usize) -> NodeOutput {
    let mut output = NodeOutput::new();
    output.insert(output_key.to_owned(), serde_json::json!(output_path));
    output.insert("image_count".to_owned(), serde_json::json!(image_count));
    output.insert(
        format!("{output_key}_count"),
        serde_json::json!(image_count),
    );
    output.insert(
        format!("{output_key}_success"),
        serde_json::Value::Bool(true),
    );
    output
}
