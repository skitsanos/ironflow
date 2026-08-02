//! Read `.xlsx` workbooks into typed rows.

pub(super) mod archive_preflight;
mod budget;
mod cell_admission;
mod cells;
mod diagnostics;
mod guard;
mod headers;
mod output_budget;
mod shared_strings;
mod sheets;
mod stream;
mod workbook;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::execution::run_tracked_blocking_step;
use crate::util::file_source::get_file_source;
use crate::util::node_config::config_bool_or;

use super::common::string_or;

pub struct ExtractXlsxNode;

#[async_trait]
impl Node for ExtractXlsxNode {
    fn node_type(&self) -> &str {
        "extract_xlsx"
    }

    fn description(&self) -> &str {
        "Extract typed rows from an Excel (.xlsx) workbook, one or every sheet"
    }

    async fn execute(&self, config: &Value, ctx: &Context) -> Result<NodeOutput> {
        let source = get_file_source(config, ctx, "extract_xlsx")?;
        let has_header = config_bool_or(config, "has_header", ctx, true)?;
        let output_key = string_or(config, "output_key", "content", "extract_xlsx")?.to_string();
        let selector = config.get("sheet").cloned();

        let limits = workbook::Limits {
            max_zip_bytes: crate::util::limits::max_zip_uncompressed_bytes(),
            max_zip_entries: crate::util::limits::max_zip_entries(),
            max_archive_metadata_bytes: crate::util::limits::max_xlsx_archive_metadata_bytes(),
            max_rows: crate::util::limits::max_xlsx_rows(),
            max_cells: crate::util::limits::max_xlsx_cells(),
            max_output_bytes: crate::util::limits::max_xlsx_output_bytes(),
        };
        let extracted = run_tracked_blocking_step(move |execution| {
            workbook::extract(&source, selector.as_ref(), has_header, limits, execution)
        })
        .await?;

        let mut output = NodeOutput::new();
        output.insert(output_key.clone(), Value::Object(extracted.sheets));
        output.insert(
            format!("{output_key}_sheet_names"),
            Value::Array(
                extracted
                    .sheet_names
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );
        Ok(output)
    }
}
