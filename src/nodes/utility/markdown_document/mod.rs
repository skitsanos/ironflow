use crate::{
    engine::types::{Context, NodeOutput},
    nodes::{Node, NodeRegistry, file::RootedDir},
    util::execution::run_tracked_blocking_step,
};
use anyhow::Result;
use async_trait::async_trait;
use config::Config;
use output::CappedWriter;
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    sync::{Arc, OnceLock},
};
use tokio::sync::Semaphore;

mod assets;
mod config;
mod docx;
mod language;
mod model;
mod output;
mod parser;
mod pdf;

#[derive(Clone, Copy)]
enum Format {
    Pdf,
    Docx,
}

struct MarkdownDocumentNode(Format);

pub(super) fn register(registry: &mut NodeRegistry) {
    registry.register(Arc::new(MarkdownDocumentNode(Format::Pdf)));
    registry.register(Arc::new(MarkdownDocumentNode(Format::Docx)));
}

#[async_trait]
impl Node for MarkdownDocumentNode {
    fn node_type(&self) -> &str {
        match self.0 {
            Format::Pdf => "markdown_to_pdf",
            Format::Docx => "markdown_to_docx",
        }
    }
    fn description(&self) -> &str {
        "Convert Markdown into a bounded, locally generated document"
    }

    async fn execute(&self, config: &Value, ctx: &Context) -> Result<NodeOutput> {
        static ADMISSION: OnceLock<Arc<Semaphore>> = OnceLock::new();
        let permit = ADMISSION
            .get_or_init(|| Arc::new(Semaphore::new(2)))
            .clone()
            .acquire_owned()
            .await?;
        let config = Config::parse(config, ctx, self.node_type())?;
        let operation = match self.0 {
            Format::Pdf => "markdown_to_pdf",
            Format::Docx => "markdown_to_docx",
        };
        let format = self.0;
        run_tracked_blocking_step(move |execution| {
            let _permit = permit;
            let blocks = parser::parse(&config.input, &execution)?;
            let pictures = assets::load(&config, &blocks, &execution)?;
            let parent = config.path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
            let leaf = config.path.file_name().ok_or_else(|| anyhow::anyhow!("output_path must name a file"))?;
            let root = RootedDir::prepare(parent, operation, &execution)?;
            let mut staged = root.stage_file(Path::new(leaf), config.overwrite, &execution)?;
            let writer = CappedWriter { file: staged.writer(), execution: &execution, position: 0 };
            let pages = match format {
                Format::Pdf => {
                    let (bytes, pages) = pdf::render(&config, &blocks, &pictures, &execution)?;
                    let mut writer = writer;
                    writer.write_all(&bytes)?;
                    Some(pages)
                }
                Format::Docx => { docx::render(writer, &config, &blocks, &pictures, &execution)?; None }
            };
            staged.writer().sync_all()?;
            let bytes = staged.writer().metadata()?.len();
            execution.checkpoint()?;
            staged.commit()?;
            let mut output = NodeOutput::new();
            output.insert(config.output_key.clone(), json!(config.path));
            output.insert(format!("{}_success", config.output_key), json!(true));
            output.insert(format!("{}_metadata", config.output_key), json!({ "format": match format { Format::Pdf => "pdf", Format::Docx => "docx" }, "bytes": bytes, "pages": pages, "blocks": blocks.len(), "images": pictures.len() }));
            Ok(output)
        }).await
    }
}
