use anyhow::{Result, bail};
use std::sync::OnceLock;

const DEFAULT_PREFIX: &str = "ironflow/artifacts";
const DEFAULT_MAX_BYTES: u64 = 50 * 1024 * 1024;

pub(super) struct RemoteConfig {
    pub(super) client: aws_sdk_s3::Client,
    pub(super) bucket: String,
    pub(super) prefix: String,
    pub(super) max_bytes: u64,
}

impl RemoteConfig {
    pub(super) fn from_env() -> Result<Self> {
        let bucket = required_env("IRONFLOW_ARTIFACT_S3_BUCKET")?;
        let prefix = std::env::var("IRONFLOW_ARTIFACT_S3_PREFIX")
            .unwrap_or_else(|_| DEFAULT_PREFIX.to_owned());
        let prefix = validate_prefix(&prefix)?;
        let max_bytes = positive_u64_env("IRONFLOW_MAX_ARTIFACT_BYTES", DEFAULT_MAX_BYTES)?;
        let endpoint = optional_env("IRONFLOW_ARTIFACT_S3_ENDPOINT_URL")?;
        let region = optional_env("IRONFLOW_ARTIFACT_S3_REGION")?;
        let force_path_style = strict_bool_env("IRONFLOW_ARTIFACT_S3_FORCE_PATH_STYLE", false)?;

        let runtime = runtime()?;
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
        if let Some(region) = region {
            loader = loader.region(aws_sdk_s3::config::Region::new(region));
        }
        let shared = runtime.block_on(loader.load());
        let mut builder = aws_sdk_s3::config::Builder::from(&shared)
            .request_checksum_calculation(
                aws_sdk_s3::config::RequestChecksumCalculation::WhenRequired,
            )
            .force_path_style(force_path_style);
        if let Some(endpoint) = endpoint {
            builder = builder.endpoint_url(endpoint);
        }
        Ok(Self {
            client: aws_sdk_s3::Client::from_conf(builder.build()),
            bucket,
            prefix,
            max_bytes,
        })
    }
}

pub(super) fn runtime() -> Result<&'static tokio::runtime::Runtime> {
    static RUNTIME: OnceLock<std::result::Result<tokio::runtime::Runtime, String>> =
        OnceLock::new();
    match RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ironflow-artifacts")
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
    }) {
        Ok(runtime) => Ok(runtime),
        Err(error) => Err(anyhow::anyhow!(
            "failed to create artifact transfer runtime: {error}"
        )),
    }
}

fn required_env(name: &str) -> Result<String> {
    optional_env(name)?.ok_or_else(|| anyhow::anyhow!("{name} is required for S3 artifacts"))
}

fn optional_env(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) if value.trim() == value && !value.is_empty() => Ok(Some(value)),
        Ok(_) => bail!("{name} must be a non-empty value without surrounding whitespace"),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(_) => bail!("{name} is not valid UTF-8"),
    }
}

fn strict_bool_env(name: &str, default: bool) -> Result<bool> {
    match optional_env(name)? {
        None => Ok(default),
        Some(value) if value == "true" => Ok(true),
        Some(value) if value == "false" => Ok(false),
        Some(_) => bail!("{name} must be 'true' or 'false'"),
    }
}

fn positive_u64_env(name: &str, default: u64) -> Result<u64> {
    match optional_env(name)? {
        None => Ok(default),
        Some(value) => match value.parse::<u64>() {
            Ok(value) if value > 0 => Ok(value),
            _ => bail!("{name} must be a positive integer"),
        },
    }
}

fn validate_prefix(prefix: &str) -> Result<String> {
    let prefix = prefix.trim_matches('/');
    if prefix.is_empty()
        || prefix.len() > 512
        || prefix
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
        || prefix.bytes().any(|byte| byte.is_ascii_control())
    {
        bail!("IRONFLOW_ARTIFACT_S3_PREFIX must be a safe 1 to 512 byte key prefix");
    }
    Ok(prefix.to_owned())
}

#[cfg(test)]
mod tests {
    use super::validate_prefix;

    #[test]
    fn prefix_validation_normalizes_edges_and_rejects_ambiguous_segments() {
        assert_eq!(
            validate_prefix("/tenant/artifacts/").unwrap(),
            "tenant/artifacts"
        );
        for invalid in ["", "/", "a//b", "a/../b", "a/./b", "a\nb"] {
            assert!(validate_prefix(invalid).is_err(), "accepted {invalid:?}");
        }
    }
}
