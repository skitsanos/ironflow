use anyhow::Result;
use async_trait::async_trait;
use lopdf::{
    Document, Object, Stream,
    content::{Content, Operation},
    dictionary, xobject,
};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

use super::common::load_image_bytes;
use super::image_sources::resolve_image_sources;

pub(crate) struct ImageToPdfNode;

#[async_trait]
impl Node for ImageToPdfNode {
    fn node_type(&self) -> &str {
        "image_to_pdf"
    }

    fn description(&self) -> &str {
        "Convert one or more images to a PDF file"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let sources = resolve_image_sources(config, ctx)?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("pdf_path");
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_to_pdf requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, ctx);

        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let mut page_ids = Vec::new();

        if sources.is_empty() {
            anyhow::bail!("image_to_pdf requires at least one image in 'sources'");
        }

        for source in sources {
            let loaded = load_image_bytes(source)?;
            if loaded.image.width() == 0 || loaded.image.height() == 0 {
                anyhow::bail!("image_to_pdf: image dimensions must be > 0");
            }

            let image_stream = xobject::image_from(loaded.bytes).map_err(|e| {
                anyhow::anyhow!(
                    "image_to_pdf: failed to parse image '{}': {:?}",
                    loaded.label,
                    e
                )
            })?;
            let image_id = doc.add_object(image_stream);
            let image_name = format!("X{}", image_id.0);
            let width = loaded.image.width();
            let height = loaded.image.height();

            let media_box = vec![
                0.into(),
                0.into(),
                i64::from(width).into(),
                i64::from(height).into(),
            ];
            let mut content = Content { operations: vec![] };
            content.operations.push(Operation::new("q", vec![]));
            content.operations.push(Operation::new(
                "cm",
                vec![
                    width.into(),
                    0.into(),
                    0.into(),
                    height.into(),
                    0.into(),
                    0.into(),
                ],
            ));
            content.operations.push(Operation::new(
                "Do",
                vec![Object::Name(image_name.clone().into_bytes())],
            ));
            content.operations.push(Operation::new("Q", vec![]));

            let content_id = doc.add_object(Stream::new(
                dictionary! {},
                content.encode().map_err(|e| {
                    anyhow::anyhow!("image_to_pdf failed to encode content stream: {:?}", e)
                })?,
            ));

            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "MediaBox" => media_box,
            });

            doc.add_xobject(page_id, image_name.as_bytes(), image_id)
                .map_err(|e| {
                    anyhow::anyhow!("image_to_pdf failed to add image resource: {:?}", e)
                })?;
            page_ids.push(page_id);
        }

        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.iter().map(|id| lopdf::Object::Reference(*id)).collect::<Vec<_>>(),
            "Count" => page_ids.len() as u32,
        };
        doc.objects
            .insert(pages_id, lopdf::Object::Dictionary(pages));

        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });

        doc.trailer.set("Root", catalog_id);
        doc.compress();
        doc.save(&output_path).map_err(|e| {
            anyhow::anyhow!(
                "image_to_pdf: failed to save PDF '{}': {:?}",
                output_path,
                e
            )
        })?;

        let mut out = NodeOutput::new();
        out.insert(output_key.to_string(), serde_json::json!(output_path));
        out.insert("image_count".to_string(), serde_json::json!(page_ids.len()));
        out.insert(
            format!("{}_count", output_key),
            serde_json::json!(page_ids.len()),
        );
        out.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(out)
    }
}
