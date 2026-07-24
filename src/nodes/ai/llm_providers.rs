mod config;
mod provider;
mod request;

pub(super) use config::{
    LlmMode, Provider, optional_u64_config, parse_mode, parse_timeout, resolve_model,
    resolve_tool_choice, resolve_tools,
};
pub(super) use provider::resolve_provider_config;
pub(super) use request::{LlmBodyInput, build_body, resolve_messages, resolve_prompt};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::Context;
    use serde_json::json;

    #[test]
    fn chat_reasoning_model_omits_temperature_and_maps_max_output_tokens() {
        let config = json!({
            "temperature": 0.2,
            "max_output_tokens": 123,
        });
        let ctx = Context::new();
        let body = build_body(&LlmBodyInput {
            mode: LlmMode::Chat,
            model: "gpt-5",
            messages: None,
            prompt: "hello",
            config: &config,
            ctx: &ctx,
            tools: None,
            tool_choice: None,
        })
        .unwrap();

        assert!(body.get("temperature").is_none());
        assert_eq!(body.get("max_completion_tokens"), Some(&json!(123)));
        assert!(body.get("max_output_tokens").is_none());
    }

    #[test]
    fn chat_non_reasoning_model_keeps_temperature() {
        let config = json!({
            "temperature": 0.2,
            "max_output_tokens": 123,
        });
        let ctx = Context::new();
        let body = build_body(&LlmBodyInput {
            mode: LlmMode::Chat,
            model: "gpt-4o-mini",
            messages: None,
            prompt: "hello",
            config: &config,
            ctx: &ctx,
            tools: None,
            tool_choice: None,
        })
        .unwrap();

        assert_eq!(body.get("temperature"), Some(&json!(0.2)));
        assert_eq!(body.get("max_completion_tokens"), Some(&json!(123)));
    }

    #[test]
    fn responses_mode_uses_max_output_tokens() {
        let config = json!({
            "temperature": 0.2,
            "max_output_tokens": 123,
        });
        let ctx = Context::new();
        let body = build_body(&LlmBodyInput {
            mode: LlmMode::Responses,
            model: "gpt-5",
            messages: None,
            prompt: "hello",
            config: &config,
            ctx: &ctx,
            tools: None,
            tool_choice: None,
        })
        .unwrap();

        assert!(body.get("temperature").is_none());
        assert_eq!(body.get("max_output_tokens"), Some(&json!(123)));
        assert!(body.get("max_completion_tokens").is_none());
    }
}
