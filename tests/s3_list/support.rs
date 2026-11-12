//! A scripted ListObjectsV2 endpoint: each test supplies the page it wants
//! returned for a request, and the server records what was asked.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::Response;
use serde_json::Value;

/// The parts of one list request that the node controls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListQuery {
    pub max_keys: Option<usize>,
    pub continuation_token: Option<String>,
}

type Responder = dyn Fn(&ListQuery) -> String + Send + Sync;

struct Shared {
    responder: Box<Responder>,
    queries: Mutex<Vec<ListQuery>>,
}

pub struct ListServer {
    endpoint: String,
    shared: Arc<Shared>,
    task: tokio::task::JoinHandle<()>,
}

impl ListServer {
    pub async fn start(responder: impl Fn(&ListQuery) -> String + Send + Sync + 'static) -> Self {
        let shared = Arc::new(Shared {
            responder: Box::new(responder),
            queries: Mutex::new(Vec::new()),
        });
        let app = Router::new().fallback(handle).with_state(shared.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            endpoint,
            shared,
            task,
        }
    }

    pub fn queries(&self) -> Vec<ListQuery> {
        self.shared.queries.lock().unwrap().clone()
    }

    /// Run `flow` with the CLI against this endpoint.
    pub async fn run(&self, flow: &str, env: &[(&str, &str)]) -> Output {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flow.lua");
        std::fs::write(&path, flow).unwrap();
        let mut command = self.command(directory.path());
        command.envs(env.iter().copied()).arg("run").arg(&path);
        let output = tokio::time::timeout(Duration::from_secs(30), command.output())
            .await
            .expect("s3_list_objects did not settle")
            .unwrap();
        Output(output)
    }

    fn command(&self, directory: &Path) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
        command
            .env_clear()
            .current_dir(directory)
            .kill_on_drop(true)
            .env("AWS_ACCESS_KEY_ID", "fixture")
            .env("AWS_SECRET_ACCESS_KEY", "fixture")
            .env("AWS_REGION", "us-east-1")
            .env("AWS_ENDPOINT_URL", &self.endpoint)
            .env("AWS_S3_FORCE_PATH_STYLE", "true")
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .env("AWS_MAX_ATTEMPTS", "1")
            .env("AWS_CONFIG_FILE", directory.join("no-config"))
            .env(
                "AWS_SHARED_CREDENTIALS_FILE",
                directory.join("no-credentials"),
            );
        command
    }
}

impl Drop for ListServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub struct Output(std::process::Output);

impl Output {
    /// The final context of a successful run.
    pub fn context(&self) -> Value {
        let stdout = String::from_utf8_lossy(&self.0.stdout);
        assert!(self.0.status.success(), "{stdout}\n{}", self.stderr());
        let (_, encoded) = stdout
            .split_once("\nContext:\n")
            .expect("CLI context missing");
        serde_json::from_str(encoded.trim()).unwrap()
    }

    /// Everything a failed run printed.
    pub fn failure(&self) -> String {
        let stdout = String::from_utf8_lossy(&self.0.stdout);
        assert!(!self.0.status.success(), "run succeeded: {stdout}");
        format!("{stdout}\n{}", self.stderr())
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.0.stderr).into_owned()
    }
}

async fn handle(State(shared): State<Arc<Shared>>, request: Request) -> Response<Body> {
    let url = url::Url::parse(&format!("http://fixture{}", request.uri())).unwrap();
    let parameter = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };
    let query = ListQuery {
        max_keys: parameter("max-keys").map(|value| value.parse().unwrap()),
        continuation_token: parameter("continuation-token"),
    };
    let body = (shared.responder)(&query);
    shared.queries.lock().unwrap().push(query);
    Response::builder()
        .status(200)
        .header("content-type", "application/xml")
        .body(Body::from(body))
        .unwrap()
}

/// One ListObjectsV2 page.
#[derive(Default)]
pub struct Page {
    pub keys: Vec<String>,
    pub common_prefixes: Vec<String>,
    pub truncated: bool,
    pub next_token: Option<String>,
}

impl Page {
    pub fn xml(&self) -> String {
        let contents: String = self
            .keys
            .iter()
            .map(|key| {
                format!(
                    "<Contents><Key>{key}</Key><Size>3</Size><ETag>\"etag\"</ETag>\
                     <StorageClass>STANDARD</StorageClass></Contents>"
                )
            })
            .collect();
        let prefixes: String = self
            .common_prefixes
            .iter()
            .map(|prefix| format!("<CommonPrefixes><Prefix>{prefix}</Prefix></CommonPrefixes>"))
            .collect();
        let token = self
            .next_token
            .as_ref()
            .map(|token| format!("<NextContinuationToken>{token}</NextContinuationToken>"))
            .unwrap_or_default();
        format!(
            "<ListBucketResult><IsTruncated>{}</IsTruncated>{token}{contents}{prefixes}\
             </ListBucketResult>",
            self.truncated
        )
    }
}

/// A bucket of `total` keys (`key-0000`, ...) served as S3 does: at most
/// `max-keys` per page (1,000 when absent) and a token naming the next index.
pub fn bucket_of(total: usize) -> impl Fn(&ListQuery) -> String + Send + Sync + 'static {
    move |query| {
        let start = query.continuation_token.as_deref().map_or(0, |token| {
            token.strip_prefix("next-").unwrap().parse().unwrap()
        });
        let end = total.min(start + query.max_keys.unwrap_or(1_000));
        Page {
            keys: (start..end)
                .map(|index| format!("key-{index:04}"))
                .collect(),
            truncated: end < total,
            next_token: (end < total).then(|| format!("next-{end}")),
            ..Page::default()
        }
        .xml()
    }
}
