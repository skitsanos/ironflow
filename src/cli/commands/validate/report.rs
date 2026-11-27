use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::lua::analysis::LuaDiagnostic;

#[derive(Serialize)]
pub(super) struct Diagnostic {
    pub code: String,
    pub severity: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<usize>,
}

impl Diagnostic {
    pub(super) fn new(code: &str, severity: &'static str, message: String) -> Self {
        Self {
            code: code.into(),
            severity,
            message,
            step: None,
            line: None,
            column: None,
        }
    }

    pub(super) fn warning(code: &str, message: String, step: &str) -> Self {
        Self {
            step: Some(step.into()),
            ..Self::new(code, "warning", message)
        }
    }
}

impl From<LuaDiagnostic> for Diagnostic {
    fn from(lua: LuaDiagnostic) -> Self {
        Self {
            code: lua.code,
            severity: "warning",
            message: lua.message,
            step: lua.step,
            line: Some(lua.line),
            column: Some(lua.column),
        }
    }
}

pub(super) fn print_json(path: &Path, valid: bool, diagnostics: &[Diagnostic]) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "file": path,
            "valid": valid,
            "diagnostics": diagnostics,
        }))?
    );
    Ok(())
}
