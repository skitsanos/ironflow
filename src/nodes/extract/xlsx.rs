//! Read `.xlsx` workbooks into typed rows.

mod cells;
mod guard;
mod headers;
mod sheets;

use anyhow::{Result, bail};
use async_trait::async_trait;
use calamine::{Reader, Xlsx, open_workbook};
use serde_json::Value;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::node_config::{config_bool, get_path};

use sheets::{CellBudget, sheet_rows};

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
        let path = get_path(config, ctx, "extract_xlsx")?;
        let has_header = config_bool(config, "has_header", ctx).unwrap_or(true);
        let output_key = config
            .get("output_key")
            .and_then(|value| value.as_str())
            .unwrap_or("content");

        let file = std::path::PathBuf::from(&path);
        guard::check_archive_size(
            &file,
            crate::util::limits::max_zip_uncompressed_bytes(),
            crate::util::limits::max_zip_entries(),
        )?;

        let mut workbook: Xlsx<_> = open_workbook(&file).map_err(|error| {
            anyhow::anyhow!("extract_xlsx: cannot read '{}': {error}", file.display())
        })?;
        let available = workbook.sheet_names().to_vec();
        let selected = select_sheets(config.get("sheet"), &available)?;

        let mut budget = CellBudget::new(crate::util::limits::max_xlsx_cells());
        let mut sheets_out = serde_json::Map::new();
        for name in &selected {
            let range = workbook.worksheet_range(name).map_err(|error| {
                anyhow::anyhow!("extract_xlsx: cannot read sheet '{name}': {error}")
            })?;
            let rows = sheet_rows(
                name,
                &range,
                has_header,
                crate::util::limits::max_xlsx_rows(),
                &mut budget,
            )?;
            sheets_out.insert(name.clone(), Value::Array(rows));
        }

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), Value::Object(sheets_out));
        output.insert(
            format!("{output_key}_sheet_names"),
            Value::Array(selected.into_iter().map(Value::String).collect()),
        );
        Ok(output)
    }
}

/// Resolve the `sheet` parameter to the sheets to extract, in workbook order.
///
/// The JSON type decides: a string is a name, a number is a 0-based index. A
/// workbook containing a sheet literally named `0` therefore stays reachable by
/// passing the string `"0"`.
fn select_sheets(selector: Option<&Value>, available: &[String]) -> Result<Vec<String>> {
    let Some(selector) = selector else {
        return Ok(available.to_vec());
    };

    match selector {
        Value::String(name) => {
            if available.iter().any(|candidate| candidate == name) {
                Ok(vec![name.clone()])
            } else {
                bail!(
                    "extract_xlsx: no sheet named '{name}'. This workbook has: {}",
                    available.join(", ")
                )
            }
        }
        Value::Number(number) => {
            let index = number.as_u64().ok_or_else(|| {
                anyhow::anyhow!("extract_xlsx: sheet index must be a whole number")
            })?;
            available
                .get(index as usize)
                .map(|name| vec![name.clone()])
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "extract_xlsx: sheet index {index} is out of range; this workbook has {} sheet(s)",
                        available.len()
                    )
                })
        }
        _ => {
            bail!("extract_xlsx: `sheet` must be a sheet name (string) or a 0-based index (number)")
        }
    }
}
