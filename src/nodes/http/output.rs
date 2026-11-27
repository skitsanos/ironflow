use anyhow::Result;
use serde_json::Value;

pub(crate) const DEFAULT_PREFIX: &str = "http";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResponseMode {
    Inline,
    Artifact,
}

impl ResponseMode {
    pub(crate) fn parse(config: &Value) -> Result<Self> {
        match config.get("response_mode") {
            None => Ok(Self::Inline),
            Some(Value::String(value)) => match value.as_str() {
                "inline" => Ok(Self::Inline),
                "artifact" => Ok(Self::Artifact),
                _ => anyhow::bail!("HTTP response_mode must be 'inline' or 'artifact'"),
            },
            Some(_) => anyhow::bail!("HTTP response_mode must be a string"),
        }
    }

    pub(crate) fn body_field(self) -> Field {
        match self {
            Self::Inline => Field::Data,
            Self::Artifact => Field::Artifact,
        }
    }

    pub(crate) fn fields(self) -> [Field; 5] {
        [
            Field::Status,
            Field::Headers,
            Field::Success,
            Field::Attempts,
            self.body_field(),
        ]
    }
}

pub(crate) fn prefix(config: &Value) -> &str {
    config
        .get("output_key")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_PREFIX)
}

#[derive(Clone, Copy)]
pub(crate) enum Field {
    Status,
    Headers,
    Success,
    Attempts,
    Data,
    Artifact,
}

impl Field {
    pub(crate) fn key(self, prefix: &str) -> String {
        format!("{prefix}_{}", self.suffix())
    }

    pub(crate) fn suffix(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Headers => "headers",
            Self::Success => "success",
            Self::Attempts => "attempts",
            Self::Data => "data",
            Self::Artifact => "artifact",
        }
    }
}
