//! IF-212: the process-wide outbound destination policy covers every node
//! that connects to a flow-chosen destination.
//!
//! Every case runs `ironflow run` as a child with an explicit environment, so
//! no variable of the test process is involved. The destination is a loopback
//! listener that counts accepted connections: a refusal must leave it at
//! zero. Only made-up credentials are used and nothing leaves the machine.
#![cfg(unix)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const ON: (&str, &str) = ("IRONFLOW_BLOCK_PRIVATE_NETWORK", "true");
const ALLOWLIST: &str = "IRONFLOW_PRIVATE_NETWORK_ALLOWLIST";

/// A loopback listener that counts connections and answers each with a small
/// JSON document, which is enough for a client to have connected.
struct Listener {
    port: u16,
    hits: Arc<AtomicUsize>,
}

impl Listener {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                counter.fetch_add(1, Ordering::SeqCst);
                std::thread::spawn(move || {
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
                    let mut buffer = [0_u8; 16 * 1024];
                    let _ = stream.read(&mut buffer);
                    let body = r#"{"ok":true}"#;
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                });
            }
        });
        Self { port, hits }
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

/// One covered client: a single step whose destination is the listener.
/// `%URL%` is `http://<host>:<port>`, `%HOST%` and `%PORT%` its parts.
struct Case {
    name: &'static str,
    step: &'static str,
    /// False where the default build has no driver that would connect.
    connects: bool,
}

const EMAIL: &str =
    r#"from = "a@example.com", to = "b@example.com", subject = "s", text = "t", timeout = 5"#;

fn case(name: &'static str, step: &'static str) -> Case {
    Case {
        name,
        step,
        connects: true,
    }
}

fn cases() -> Vec<Case> {
    let mut cases = http_cases();
    cases.extend(provider_cases());
    cases.extend(mcp_cases());
    cases
}

/// The `http_*` nodes.
fn http_cases() -> Vec<Case> {
    vec![
        case(
            "http_get",
            r#"nodes.http_get({ url = "%URL%/x", timeout = 5 })"#,
        ),
        case(
            "http_request",
            r#"nodes.http_request({ method = "POST", url = "%URL%/x", timeout = 5 })"#,
        ),
    ]
}

/// Nodes behind the shared provider client.
fn provider_cases() -> Vec<Case> {
    vec![
        case(
            "slack_notification",
            r#"nodes.slack_notification({ webhook_url = "%URL%/hook", text = "hi", timeout = 5 })"#,
        ),
        case(
            "send_email resend",
            r#"nodes.send_email({ provider = "resend", api_key = "re_made_up_key",
                api_url = "%URL%/emails", %EMAIL% })"#,
        ),
        case(
            "llm custom base_url",
            r#"nodes.llm({ provider = "custom", base_url = "%URL%", model = "m",
                prompt = "hi", timeout = 5 })"#,
        ),
        case(
            "llm absolute chat_path",
            r#"nodes.llm({ provider = "custom", base_url = "https://provider.example",
                chat_path = "%URL%/v1/chat", model = "m", prompt = "hi", timeout = 5 })"#,
        ),
        case(
            "llm absolute responses_path",
            r#"nodes.llm({ provider = "custom", mode = "responses",
                base_url = "https://provider.example", responses_path = "%URL%/v1/responses",
                model = "m", prompt = "hi", timeout = 5 })"#,
        ),
        case(
            "llm absolute path",
            r#"nodes.llm({ provider = "custom", base_url = "https://provider.example",
                path = "%URL%/v1/any", model = "m", prompt = "hi", timeout = 5 })"#,
        ),
        case(
            "llm openai_compatible",
            r#"nodes.llm({ provider = "openai_compatible", base_url = "%URL%/v1",
                api_key = "sk-made-up", model = "m", prompt = "hi", timeout = 5 })"#,
        ),
        case(
            "ai_embed ollama",
            r#"nodes.ai_embed({ provider = "ollama", ollama_host = "%URL%", model = "m",
                input_key = "texts", timeout = 5, retries = 0 })"#,
        ),
        case(
            "ai_embed oauth token_url",
            r#"nodes.ai_embed({ provider = "oauth", token_url = "%URL%/token",
                base_url = "https://provider.example/v1", client_id = "id-%PORT%",
                client_secret = "made-up-secret", model = "m", input_key = "texts",
                timeout = 5, retries = 0 })"#,
        ),
        case(
            "ai_chunk_semantic",
            r#"nodes.ai_chunk_semantic({ provider = "ollama", ollama_host = "%URL%", model = "m",
                source_key = "text", timeout = 5, retries = 0 })"#,
        ),
        case(
            "transcribe",
            r#"nodes.transcribe({ provider = "openai", base_url = "%URL%/v1",
                api_key = "sk-made-up", path = "%AUDIO%", timeout = 5 })"#,
        ),
        case(
            "arangodb_aql",
            r#"nodes.arangodb_aql({ url = "%URL%", database = "db", query = "RETURN 1",
                timeout = 5 })"#,
        ),
    ]
}

/// `mcp_client` over Streamable HTTP.
fn mcp_cases() -> Vec<Case> {
    vec![case(
        "mcp_client streamable_http",
        r#"nodes.mcp_client({ transport = "streamable_http", url = "%URL%/mcp",
                action = "initialize", timeout = 5 })"#,
    )]
}

struct Outcome {
    success: bool,
    text: String,
}

/// Run one step under `environment` and an optional `ironflow.yaml`.
fn run_step(
    step: &str,
    host: &str,
    port: u16,
    environment: &[(&str, &str)],
    yaml: &str,
) -> Outcome {
    let directory = tempfile::tempdir().unwrap();
    let audio = directory.path().join("audio.wav");
    std::fs::write(&audio, b"RIFF made-up audio").unwrap();
    let step = step
        .replace("%EMAIL%", EMAIL)
        .replace("%URL%", &format!("http://{host}:{port}"))
        .replace("%HOST%", host)
        .replace("%PORT%", &port.to_string())
        .replace("%AUDIO%", audio.to_str().unwrap());
    let flow =
        format!("local flow = Flow.new(\"outbound\")\nflow:step(\"call\", {step})\nreturn flow\n");
    std::fs::write(directory.path().join("flow.lua"), flow).unwrap();
    if !yaml.is_empty() {
        std::fs::write(directory.path().join("ironflow.yaml"), yaml).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .current_dir(directory.path())
        .env_clear()
        .env("HOME", directory.path())
        .env("AWS_ACCESS_KEY_ID", "AKIAMADEUPKEY")
        .env("AWS_SECRET_ACCESS_KEY", "made-up-secret")
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .env("AWS_MAX_ATTEMPTS", "1")
        .envs(environment.iter().copied())
        .args(["run", "flow.lua", "--store-dir", "runs", "--context"])
        .arg(r#"{"texts":["one"],"text":"One sentence. Another sentence. A third one."}"#)
        .output()
        .unwrap();
    Outcome {
        success: output.status.success(),
        text: format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

#[test]
fn the_policy_refuses_every_covered_client_before_it_connects() {
    // Every client is tried, so one run names all that still connect.
    let mut failures = Vec::new();
    for case in cases() {
        let listener = Listener::start();
        let outcome = run_step(case.step, "127.0.0.1", listener.port, &[ON], "");
        let refused = !outcome.success && outcome.text.contains("private network");
        if !refused || listener.hits() != 0 {
            failures.push(format!(
                "{}: refused={refused} connections={}",
                case.name,
                listener.hits()
            ));
        }
    }
    assert!(failures.is_empty(), "not refused: {failures:#?}");
}

#[test]
fn without_the_policy_every_client_still_connects() {
    for case in cases() {
        let listener = Listener::start();
        let outcome = run_step(case.step, "127.0.0.1", listener.port, &[], "");
        assert!(
            !outcome.text.contains("private network"),
            "{}: {}",
            case.name,
            outcome.text
        );
        if case.connects {
            assert!(listener.hits() > 0, "{}: {}", case.name, outcome.text);
        }
        // An explicit `false` is the default.
        let off = [("IRONFLOW_BLOCK_PRIVATE_NETWORK", "false")];
        let outcome = run_step(case.step, "127.0.0.1", listener.port, &off, "");
        assert!(!outcome.text.contains("private network"), "{}", case.name);
    }
}

#[test]
fn an_allowlisted_address_is_reachable_under_the_policy() {
    for case in cases() {
        let listener = Listener::start();
        let entry = format!("127.0.0.1:{}", listener.port);
        let environment = [ON, (ALLOWLIST, entry.as_str())];
        let outcome = run_step(case.step, "127.0.0.1", listener.port, &environment, "");
        assert!(
            !outcome.text.contains("private network"),
            "{}: {}",
            case.name,
            outcome.text
        );
        if case.connects {
            assert!(listener.hits() > 0, "{}: {}", case.name, outcome.text);
        }
        // The entry is bound to its port: another listener stays unreachable.
        let other = Listener::start();
        let outcome = run_step(case.step, "127.0.0.1", other.port, &environment, "");
        assert!(outcome.text.contains("private network"), "{}", case.name);
        assert_eq!(other.hits(), 0, "{}", case.name);
    }
}

#[test]
fn a_host_name_entry_admits_that_name_on_that_port_only() {
    let step = r#"nodes.http_get({ url = "%URL%/x", timeout = 5 })"#;
    let listener = Listener::start();
    let entry = format!("localhost:{}", listener.port);
    let environment = [ON, (ALLOWLIST, entry.as_str())];

    let outcome = run_step(step, "localhost", listener.port, &environment, "");
    assert!(outcome.success, "{}", outcome.text);
    assert_eq!(listener.hits(), 1);

    // The name entry is not a licence for the address behind it, nor for the
    // same name on another port.
    let outcome = run_step(step, "127.0.0.1", listener.port, &environment, "");
    assert!(outcome.text.contains("private network"), "{}", outcome.text);
    let other = Listener::start();
    let outcome = run_step(step, "localhost", other.port, &environment, "");
    assert!(
        outcome.text.contains("other ports only"),
        "{}",
        outcome.text
    );
    assert_eq!((listener.hits(), other.hits()), (1, 0));
}

#[test]
fn a_node_cannot_loosen_the_process_policy() {
    let listener = Listener::start();
    for step in [
        r#"nodes.http_get({ url = "%URL%/x", block_private_network = false })"#,
        r#"nodes.http_request({ url = "%URL%/x", block_private_network = "false" })"#,
        r#"nodes.http_get({ url = "%URL%/x", proxy_mode = "direct" })"#,
    ] {
        let outcome = run_step(step, "127.0.0.1", listener.port, &[ON], "");
        assert!(!outcome.success, "{}", outcome.text);
        assert!(outcome.text.contains("private network"), "{}", outcome.text);
    }
    assert_eq!(listener.hits(), 0);

    // Off by default, the per-node option still tightens on its own.
    let step = r#"nodes.http_get({ url = "%URL%/x", block_private_network = true })"#;
    let outcome = run_step(step, "127.0.0.1", listener.port, &[], "");
    assert!(outcome.text.contains("block_private_network is enabled"));
    assert_eq!(listener.hits(), 0);
}

#[test]
fn the_config_file_keys_apply_below_the_environment() {
    let step = r#"nodes.http_get({ url = "%URL%/x", timeout = 5 })"#;
    let listener = Listener::start();
    let port = listener.port;

    let outcome = run_step(
        step,
        "127.0.0.1",
        port,
        &[],
        "block_private_network: true\n",
    );
    assert!(outcome.text.contains("private network"), "{}", outcome.text);
    assert_eq!(listener.hits(), 0);

    let yaml = format!(
        "block_private_network: true\nprivate_network_allowlist:\n  - \"127.0.0.1:{port}\"\n"
    );
    let outcome = run_step(step, "127.0.0.1", port, &[], &yaml);
    assert!(outcome.success, "{}", outcome.text);
    assert_eq!(listener.hits(), 1);

    // The environment wins over both keys.
    let narrower = [(ALLOWLIST, "10.9.9.9")];
    let outcome = run_step(step, "127.0.0.1", port, &narrower, &yaml);
    assert!(outcome.text.contains("private network"), "{}", outcome.text);
    let off = [("IRONFLOW_BLOCK_PRIVATE_NETWORK", "false")];
    let outcome = run_step(
        step,
        "127.0.0.1",
        port,
        &off,
        "block_private_network: true\n",
    );
    assert!(outcome.success, "{}", outcome.text);
    assert_eq!(listener.hits(), 2);
}

/// The environment, the `ironflow.yaml` text and the source the error names.
type MalformedCase = (
    &'static [(&'static str, &'static str)],
    &'static str,
    &'static str,
);

#[test]
fn a_malformed_setting_fails_before_the_flow_runs() {
    let step = r#"nodes.http_get({ url = "%URL%/x", timeout = 5 })"#;
    let listener = Listener::start();
    const SECRET_ENTRY: &str = "10.44.55.66/8";
    let cases: &[MalformedCase] = &[
        (
            &[("IRONFLOW_BLOCK_PRIVATE_NETWORK", "yes")],
            "",
            "IRONFLOW_BLOCK_PRIVATE_NETWORK",
        ),
        (
            &[("IRONFLOW_BLOCK_PRIVATE_NETWORK", "")],
            "",
            "IRONFLOW_BLOCK_PRIVATE_NETWORK",
        ),
        (
            &[("IRONFLOW_BLOCK_PRIVATE_NETWORK", "1")],
            "",
            "IRONFLOW_BLOCK_PRIVATE_NETWORK",
        ),
        (&[(ALLOWLIST, "")], "", ALLOWLIST),
        (&[(ALLOWLIST, " , ")], "", ALLOWLIST),
        (&[(ALLOWLIST, SECRET_ENTRY)], "", ALLOWLIST),
        (
            &[(ALLOWLIST, "http://ollama.internal:11434")],
            "",
            ALLOWLIST,
        ),
        (&[(ALLOWLIST, "*.internal")], "", ALLOWLIST),
        (
            &[],
            "private_network_allowlist:\n  - \"10.44.55.66/8\"\n",
            "private_network_allowlist",
        ),
    ];
    for (environment, yaml, source) in cases {
        let outcome = run_step(step, "127.0.0.1", listener.port, environment, yaml);
        assert!(!outcome.success, "{environment:?}: {}", outcome.text);
        assert!(
            outcome.text.contains(source),
            "{environment:?}: {}",
            outcome.text
        );
        assert!(!outcome.text.contains("Status:"), "{}", outcome.text);
        assert!(!outcome.text.contains("10.44.55.66"), "{}", outcome.text);
    }
    assert_eq!(listener.hits(), 0);
}
