use anyhow::{Result, ensure};
use serde_json::Value;

use crate::engine::types::Context;
use crate::util::node_config::config_usize_strict;

pub(in crate::nodes::ai) struct BatchOptions {
    pub(super) size: usize,
    pub(super) max_bytes: usize,
    pub(super) retries: usize,
}

impl BatchOptions {
    pub(in crate::nodes::ai) fn from_config(config: &Value, ctx: &Context) -> Result<Self> {
        let size = config_usize_strict(config, "batch_size", ctx)?.unwrap_or(512);
        let max_bytes = config_usize_strict(config, "batch_max_bytes", ctx)?.unwrap_or(262_144);
        let retries = config_usize_strict(config, "batch_retries", ctx)?.unwrap_or(2);
        ensure!(
            (1..=2048).contains(&size),
            "embedding batch_size must be between 1 and 2048"
        );
        ensure!(
            (1..=50 * 1024 * 1024).contains(&max_bytes),
            "embedding batch_max_bytes must be between 1 and 52428800"
        );
        ensure!(
            retries <= 5,
            "embedding batch_retries must be between 0 and 5"
        );
        Ok(Self {
            size,
            max_bytes,
            retries,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_all_batch_options_and_interpolates_numbers() {
        let ctx = Context::from([("size".into(), json!(64))]);
        let options =
            BatchOptions::from_config(&json!({"batch_size": "${ctx.size}"}), &ctx).unwrap();
        assert_eq!(options.size, 64);
        for (key, values) in [
            (
                "batch_size",
                vec![
                    json!(0),
                    json!(2049),
                    json!(-1),
                    json!(1.5),
                    json!(true),
                    json!("${ctx.missing}"),
                ],
            ),
            (
                "batch_max_bytes",
                vec![json!(0), json!(52428801), json!("bad")],
            ),
            ("batch_retries", vec![json!(6), json!(-1), json!(null)]),
        ] {
            for value in values {
                assert!(
                    BatchOptions::from_config(&json!({key: value}), &ctx).is_err(),
                    "{key}"
                );
            }
        }
    }
}
