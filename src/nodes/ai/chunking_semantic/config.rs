use anyhow::Result;

use crate::engine::types::Context;
use crate::util::node_config::{config_f64, config_f64_or, config_usize_strict};

pub(super) struct SemanticChunkParams {
    pub(super) timeout_s: f64,
    pub(super) sim_window: usize,
    pub(super) sg_window: usize,
    pub(super) poly_order: usize,
    pub(super) threshold: f64,
    pub(super) min_distance: usize,
}

impl SemanticChunkParams {
    pub(super) fn from_config(config: &serde_json::Value, ctx: &Context) -> Result<Self> {
        let sim_window = config_usize_strict(config, "sim_window", ctx)?.unwrap_or(3);
        let sim_window = odd_at_least_three(sim_window, "sim_window")?;
        let sg_window = config_usize_strict(config, "sg_window", ctx)?.unwrap_or(11);
        let sg_window = odd(sg_window, "sg_window")?;

        Ok(Self {
            timeout_s: config_f64_or(config, "timeout", ctx, 120.0)?,
            sim_window,
            sg_window,
            poly_order: config_usize_strict(config, "poly_order", ctx)?.unwrap_or(3),
            threshold: config_f64(config, "threshold", ctx)
                .unwrap_or(0.5)
                .clamp(0.0, 1.0),
            min_distance: config_usize_strict(config, "min_distance", ctx)?.unwrap_or(2),
        })
    }
}

fn odd_at_least_three(value: usize, key: &str) -> Result<usize> {
    odd(value.max(3), key)
}

fn odd(value: usize, key: &str) -> Result<usize> {
    if value.is_multiple_of(2) {
        value
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("'{}' is too large", key))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx_with(key: &str, value: serde_json::Value) -> Context {
        let mut ctx = Context::new();
        ctx.insert(key.to_string(), value);
        ctx
    }

    #[test]
    fn threshold_resolves_from_interpolated_context() {
        let ctx = ctx_with("threshold", json!(0.3));
        let config = json!({ "threshold": "${ctx.threshold}" });
        assert_eq!(
            SemanticChunkParams::from_config(&config, &ctx)
                .unwrap()
                .threshold,
            0.3
        );
    }

    #[test]
    fn threshold_defaults_when_absent() {
        assert_eq!(
            SemanticChunkParams::from_config(&json!({}), &Context::new())
                .unwrap()
                .threshold,
            0.5
        );
    }

    #[test]
    fn threshold_is_clamped_to_unit_range() {
        assert_eq!(
            SemanticChunkParams::from_config(&json!({ "threshold": 4.2 }), &Context::new())
                .unwrap()
                .threshold,
            1.0
        );
    }

    #[test]
    fn sim_window_is_forced_odd_and_at_least_three() {
        assert_eq!(
            SemanticChunkParams::from_config(&json!({ "sim_window": 1 }), &Context::new())
                .unwrap()
                .sim_window,
            3
        );
        assert_eq!(
            SemanticChunkParams::from_config(&json!({ "sim_window": 6 }), &Context::new())
                .unwrap()
                .sim_window,
            7
        );
    }

    #[test]
    fn sg_window_is_forced_odd() {
        assert_eq!(
            SemanticChunkParams::from_config(&json!({ "sg_window": 10 }), &Context::new())
                .unwrap()
                .sg_window,
            11
        );
    }

    #[test]
    fn windows_resolve_from_interpolated_context() {
        let ctx = ctx_with("window", json!(15));
        let config = json!({ "sg_window": "${ctx.window}", "min_distance": "${ctx.window}" });
        let params = SemanticChunkParams::from_config(&config, &ctx).unwrap();
        assert_eq!(params.sg_window, 15);
        assert_eq!(params.min_distance, 15);
    }

    #[test]
    fn timeout_resolves_from_interpolated_context() {
        let ctx = ctx_with("timeout", json!(45));
        let config = json!({ "timeout": "${ctx.timeout}" });
        assert_eq!(
            SemanticChunkParams::from_config(&config, &ctx)
                .unwrap()
                .timeout_s,
            45.0
        );
    }
}
