mod config;
mod provider;
mod response;
mod response_json;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::duration::positive_duration;
use crate::util::limits::{max_audio_bytes, max_transcribe_response_bytes};

pub struct TranscribeNode;

/// Remove every occurrence of the caller's own API key from `text`,
/// replacing it with `[REDACTED]`.
///
/// `redact_sensitive_text` (applied upstream in `provider::send` and
/// `response::interpret`, and still called there) only recognises specific
/// phrasings such as `credential: <value>` or `key=<value>`. A provider is
/// free to word things differently: OpenAI's real error text is "Incorrect
/// API key provided: sk-...", where "provided" is not a keyword any
/// pattern-based redactor recognises, so the key would otherwise pass
/// through untouched into the run's error, its persisted state, and any log
/// of it. Since this node knows exactly which key it sent, it can strip
/// that exact string regardless of how the provider phrases the message.
/// This is defence in depth: it runs in addition to, not instead of, the
/// pattern-based redaction.
///
/// This function used to skip redaction for any key shorter than 8
/// characters, on the theory that a short "key" was more likely a
/// placeholder and that blindly replacing a short string risked mangling
/// unrelated words. That guard failed open: this node accepts an arbitrary
/// OpenAI-compatible `base_url`, so a short credential is entirely plausible
/// against a custom or internal test server, and it would have sailed
/// through unredacted into the run's error and persisted state -- exactly
/// the leak this function exists to prevent. The only case where skipping
/// redaction is actually correct is an empty key: there is no secret to
/// remove, and `str::replace` treats an empty pattern as matching between
/// *every* character, so without this guard `text.replace("", "[REDACTED]")`
/// would shred the message into confetti (`"[REDACTED]s[REDACTED]o..."`)
/// rather than leave it alone. For every non-empty key, however short, we
/// redact. A noisier diagnostic message from a short key coincidentally
/// matching unrelated text is a cosmetic cost; a leaked credential is a
/// security incident. Do not reintroduce a minimum-length threshold here --
/// that is the fail-open bug this comment is guarding against.
fn redact_own_key(text: &str, key: &str) -> String {
    if key.is_empty() {
        return text.to_string();
    }
    text.replace(key, "[REDACTED]")
}

#[async_trait]
impl Node for TranscribeNode {
    fn node_type(&self) -> &str {
        "transcribe"
    }

    fn description(&self) -> &str {
        "Transcribe an audio or video file to VTT, SRT, text, or JSON"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let resolved = config::resolve(config, ctx)?;
        let timeout = positive_duration(resolved.timeout_s, "transcribe timeout")?;

        let file_name = resolved.source.file_name();
        let audio_source = resolved.source.clone();
        let audio = crate::util::execution::run_tracked_blocking_step(move |execution| {
            let (file, label) = audio_source.open("transcribe", &execution)?.into_parts();
            let declared = file.metadata()?.len();
            let maximum = max_audio_bytes();
            if declared > maximum {
                anyhow::bail!(
                    "transcribe input '{label}' is {declared} bytes, exceeds IRONFLOW_MAX_AUDIO_BYTES limit ({maximum})"
                );
            }
            crate::util::bounded_read::read_capped_controlled(
                file,
                maximum,
                &format!("transcribe input '{label}'"),
                &execution,
            )
        })
        .await?;

        let client = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(provider::same_origin_redirect_policy())
            .build()
            .map_err(|error| {
                anyhow::anyhow!("transcribe: failed to build HTTP client: {}", error)
            })?;

        // Every fallible call below this point runs after the provider has
        // seen `resolved.api_key`, so its error text is exactly where a
        // provider could echo the key back in a phrasing the shared
        // pattern-based redactor doesn't recognise. Wrapping both call
        // sites (the transport-error path inside `provider::send`, and the
        // provider-error/parse path inside `response::interpret`) is what
        // guarantees every error path out of this node gets the positional
        // scrub, not just one branch of it.
        let (status, body) = provider::send(
            &client,
            &resolved,
            audio,
            &file_name,
            max_transcribe_response_bytes(),
        )
        .await
        .map_err(|error| {
            anyhow::anyhow!("{}", redact_own_key(&error.to_string(), &resolved.api_key))
        })?;
        let transcript = response::interpret(status, &body, resolved.format).map_err(|error| {
            anyhow::anyhow!("{}", redact_own_key(&error.to_string(), &resolved.api_key))
        })?;

        let mut output = NodeOutput::new();
        let key = &resolved.output_key;

        if let Some(destination) = &resolved.output_file {
            tokio::fs::write(destination, body.as_bytes())
                .await
                .map_err(|error| {
                    anyhow::anyhow!("transcribe: failed to write '{}': {}", destination, error)
                })?;
            output.insert(
                format!("{}_path", key),
                serde_json::Value::String(destination.clone()),
            );
        }

        output.insert(key.clone(), transcript);
        output.insert(
            format!("{}_format", key),
            serde_json::Value::String(resolved.format.as_label().to_string()),
        );
        output.insert(
            format!("{}_model", key),
            serde_json::Value::String(resolved.model.clone()),
        );
        output.insert(format!("{}_success", key), serde_json::Value::Bool(true));

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_every_occurrence_of_a_real_length_key_regardless_of_phrasing() {
        // This is OpenAI's actual error phrasing: the key follows "provided:",
        // a word the shared pattern-based redactor does not recognise as a
        // key/credential marker.
        let text =
            "Incorrect API key provided: sentinel-key-abc123. Find your key at https://example.com";
        let redacted = redact_own_key(text, "sentinel-key-abc123");
        assert!(!redacted.contains("sentinel-key-abc123"), "{redacted}");
        assert!(redacted.contains("[REDACTED]"), "{redacted}");
        assert!(
            redacted.contains("Incorrect API key provided"),
            "{redacted}"
        );
    }

    #[test]
    fn strips_repeated_occurrences_of_the_key() {
        let text = "key sk-longenoughkey rejected; retry without sk-longenoughkey";
        let redacted = redact_own_key(text, "sk-longenoughkey");
        assert!(!redacted.contains("sk-longenoughkey"), "{redacted}");
        assert_eq!(redacted.matches("[REDACTED]").count(), 2);
    }

    #[test]
    fn redacts_a_short_key_because_it_is_still_a_secret() {
        // A short key is still a secret: this node accepts an arbitrary
        // OpenAI-compatible `base_url`, so a short credential against a
        // custom or internal test server is plausible. Redaction must fail
        // closed here even though the replacement is noisier -- a noisy
        // error is cosmetic, a leaked credential is not.
        let text = "failed for user k in region k-west";
        let redacted = redact_own_key(text, "k");
        assert_eq!(
            redacted,
            "failed for user [REDACTED] in region [REDACTED]-west"
        );
        assert_eq!(redacted.matches("[REDACTED]").count(), 2);
    }

    #[test]
    fn leaves_text_alone_when_the_key_is_empty() {
        let text = "some diagnostic text";
        let redacted = redact_own_key(text, "");
        assert_eq!(redacted, text);
    }
}
