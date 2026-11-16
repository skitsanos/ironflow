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

/// Widest similarity window. The distances cost sentences x window x
/// embedding dimension.
const MAX_SIM_WINDOW: usize = 101;
/// Widest smoothing window. Each of the three filter passes costs
/// sentences x window; with the window as long as the document that is
/// quadratic (5 s for 50,000 sentences, optimized).
const MAX_SG_WINDOW: usize = 1_001;
/// Highest polynomial order. Fitting is cubic in the order: order 1,600 took
/// 19 s and order 3,200 took 330 s (optimized), on the async worker.
const MAX_POLY_ORDER: usize = 16;

impl SemanticChunkParams {
    pub(super) fn from_config(config: &serde_json::Value, ctx: &Context) -> Result<Self> {
        let sim_window = config_usize_strict(config, "sim_window", ctx)?.unwrap_or(3);
        let sim_window = odd_at_least_three(at_most(sim_window, MAX_SIM_WINDOW, "sim_window")?);
        let sg_window = config_usize_strict(config, "sg_window", ctx)?.unwrap_or(11);
        let sg_window = odd(at_most(sg_window, MAX_SG_WINDOW, "sg_window")?);
        let poly_order = config_usize_strict(config, "poly_order", ctx)?.unwrap_or(3);

        Ok(Self {
            timeout_s: config_f64_or(config, "timeout", ctx, 120.0)?,
            sim_window,
            sg_window,
            poly_order: at_most(poly_order, MAX_POLY_ORDER, "poly_order")?,
            threshold: config_f64(config, "threshold", ctx)
                .unwrap_or(0.5)
                .clamp(0.0, 1.0),
            min_distance: config_usize_strict(config, "min_distance", ctx)?.unwrap_or(2),
        })
    }
}

/// Refuse `value` above `ceiling` (IF-215). The ceilings are odd, so
/// forcing an accepted window odd never passes them.
fn at_most(value: usize, ceiling: usize, key: &str) -> Result<usize> {
    if value > ceiling {
        anyhow::bail!("ai_chunk_semantic: '{key}' must be at most {ceiling}, got {value}");
    }
    Ok(value)
}

fn odd_at_least_three(value: usize) -> usize {
    odd(value.max(3))
}

fn odd(value: usize) -> usize {
    if value.is_multiple_of(2) {
        value + 1
    } else {
        value
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

    fn rejected(config: serde_json::Value) -> String {
        match SemanticChunkParams::from_config(&config, &Context::new()) {
            Ok(_) => panic!("{config} must be rejected"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn window_and_order_ceilings_admit_their_own_value() {
        let params = SemanticChunkParams::from_config(
            &json!({ "sim_window": 101, "sg_window": 1001, "poly_order": 16 }),
            &Context::new(),
        )
        .unwrap();
        assert_eq!(
            (params.sim_window, params.sg_window, params.poly_order),
            (101, 1001, 16)
        );
        // An even value at the ceiling is still forced odd above it.
        assert_eq!(
            rejected(json!({ "sg_window": 1002 })),
            "ai_chunk_semantic: 'sg_window' must be at most 1001, got 1002"
        );
    }

    #[test]
    fn windows_and_order_above_their_ceilings_are_rejected() {
        assert_eq!(
            rejected(json!({ "sim_window": 103 })),
            "ai_chunk_semantic: 'sim_window' must be at most 101, got 103"
        );
        assert_eq!(
            rejected(json!({ "sg_window": 100_001 })),
            "ai_chunk_semantic: 'sg_window' must be at most 1001, got 100001"
        );
        assert_eq!(
            rejected(json!({ "poly_order": 17 })),
            "ai_chunk_semantic: 'poly_order' must be at most 16, got 17"
        );
        assert_eq!(
            rejected(json!({ "poly_order": 3200 })),
            "ai_chunk_semantic: 'poly_order' must be at most 16, got 3200"
        );
    }
}
