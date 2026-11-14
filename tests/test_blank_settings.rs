//! IF-198: a setting that is present but empty is a startup error, never a
//! silent fall-through to "not configured".
//!
//! Every case runs `ironflow serve` as a child with an explicit environment,
//! so no variable of the test process or a local dotenv file is involved.
#![cfg(unix)]

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

/// A made-up key; it only has to differ from every error message.
const YAML_KEY: &str = "if198-yaml-key-0123456789";

enum Startup {
    Failed(String),
    Serving(tokio::process::Child, String),
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

/// Start `ironflow serve` on a free loopback port in `directory` and report
/// whether it refused to start or began serving.
async fn start(directory: &Path, config: &str, environment: &[(&str, &str)]) -> Startup {
    std::fs::write(directory.join("ironflow.yaml"), config).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let log_path = directory.join("server.log");
    let mut server = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .current_dir(directory)
        .env_clear()
        .envs(environment.iter().copied())
        .args(["serve", "--host", "127.0.0.1", "--port"])
        .arg(address.port().to_string())
        .args(["--store-dir", "runs"])
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&log_path).unwrap())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let base = format!("http://{address}");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if server.try_wait().unwrap().is_some() {
            return Startup::Failed(std::fs::read_to_string(&log_path).unwrap());
        }
        let live = client()
            .get(format!("{base}/health/live"))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success());
        if live {
            return Startup::Serving(server, base);
        }
        assert!(Instant::now() < deadline, "server startup deadline");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn runs_status(base: &str, key: Option<&str>) -> u16 {
    let mut request = client().get(format!("{base}/runs"));
    if let Some(key) = key {
        request = request.header("X-API-Key", key);
    }
    request.send().await.unwrap().status().as_u16()
}

/// Assert that startup failed, that the diagnostic names `source`, and that it
/// does not disclose the configured key. The store URL variables keep their
/// own established diagnostic, so only the name is asserted.
async fn assert_refused(config: &str, environment: &[(&str, &str)], source: &str) {
    let directory = tempfile::tempdir().unwrap();
    match start(directory.path(), config, environment).await {
        Startup::Failed(log) => {
            assert!(log.contains(source), "{source} not named in: {log}");
            assert!(!log.contains(YAML_KEY), "key disclosed in: {log}");
        }
        Startup::Serving(_server, base) => panic!(
            "server started with {environment:?}; GET /runs without a key returned {}",
            runs_status(&base, None).await
        ),
    }
}

fn keyed_config() -> String {
    format!("api_key: \"{YAML_KEY}\"\n")
}

#[tokio::test]
async fn blank_api_key_variable_does_not_disable_the_configured_key() {
    for blank in ["", " ", " \t "] {
        assert_refused(
            &keyed_config(),
            &[("IRONFLOW_API_KEY", blank)],
            "IRONFLOW_API_KEY",
        )
        .await;
    }
}

#[tokio::test]
async fn blank_api_key_in_the_configuration_file_fails_startup() {
    for config in ["api_key: \"\"\n", "api_key: \"   \"\n"] {
        assert_refused(config, &[], "api_key").await;
    }
}

#[tokio::test]
async fn unset_api_key_variable_falls_back_to_the_configured_key() {
    let directory = tempfile::tempdir().unwrap();
    let Startup::Serving(_server, base) = start(directory.path(), &keyed_config(), &[]).await
    else {
        panic!("server did not start with only the configured key");
    };
    assert_eq!(runs_status(&base, None).await, 401);
    assert_eq!(runs_status(&base, Some("wrong-key")).await, 401);
    assert_eq!(runs_status(&base, Some(YAML_KEY)).await, 200);
}

#[tokio::test]
async fn explicit_opt_in_still_serves_without_a_key() {
    let cases: [(&str, &[(&str, &str)]); 3] = [
        ("", &[("IRONFLOW_ALLOW_UNAUTHENTICATED_API", "true")]),
        (
            "",
            &[
                ("IRONFLOW_API_KEY", ""),
                ("IRONFLOW_ALLOW_UNAUTHENTICATED_API", "true"),
            ],
        ),
        (
            "allow_unauthenticated_api: true\n",
            &[("IRONFLOW_API_KEY", "")],
        ),
    ];
    for (config, environment) in cases {
        let directory = tempfile::tempdir().unwrap();
        let Startup::Serving(_server, base) = start(directory.path(), config, environment).await
        else {
            panic!("explicit opt-in did not start with {environment:?}");
        };
        assert_eq!(runs_status(&base, None).await, 200);
    }
}

#[tokio::test]
async fn blank_opt_in_variable_is_not_read_as_false_or_true() {
    for blank in ["", " "] {
        let directory = tempfile::tempdir().unwrap();
        let environment = [("IRONFLOW_ALLOW_UNAUTHENTICATED_API", blank)];
        let Startup::Failed(log) = start(directory.path(), &keyed_config(), &environment).await
        else {
            panic!("server started with a blank opt-in variable");
        };
        assert!(log.contains("IRONFLOW_ALLOW_UNAUTHENTICATED_API"), "{log}");
    }
}

#[tokio::test]
async fn blank_variables_do_not_replace_other_configured_settings() {
    let config = format!(
        "{}cors_origins: [\"https://app.example.com\"]\nsql_table_prefix: \"tenant_a_\"\n",
        keyed_config()
    );
    for (name, blank) in [
        ("IRONFLOW_CORS_ORIGINS", ""),
        ("IRONFLOW_CORS_ORIGINS", " , "),
        ("IRONFLOW_SQL_TABLE_PREFIX", ""),
        ("IRONFLOW_SQL_TABLE_PREFIX", "  "),
        ("IRONFLOW_STORE", ""),
        ("IRONFLOW_EVENT_STORE", " "),
        ("IRONFLOW_STORE_URL", ""),
        ("IRONFLOW_EVENT_STORE_URL", ""),
    ] {
        let environment: &[(&str, &str)] = if name == "IRONFLOW_EVENT_STORE_URL" {
            &[("IRONFLOW_EVENT_STORE", "sqlite"), (name, blank)]
        } else {
            &[(name, blank)]
        };
        assert_refused(&config, environment, name).await;
    }
}

/// The table prefix that the configuration file selects is the one in use
/// when the variable is unset: the fallback itself is unchanged.
#[tokio::test]
async fn unset_table_prefix_variable_keeps_the_configured_prefix() {
    let directory = tempfile::tempdir().unwrap();
    let config = format!("{}sql_table_prefix: \"tenant_a_\"\n", keyed_config());
    let Startup::Serving(mut server, base) = start(directory.path(), &config, &[]).await else {
        panic!("server did not start with a configured table prefix");
    };
    assert_eq!(runs_status(&base, Some(YAML_KEY)).await, 200);
    server.kill().await.unwrap();
    let database = std::fs::read(directory.path().join("runs/ironflow.sqlite")).unwrap();
    let needle = b"tenant_a_runs";
    assert!(
        database
            .windows(needle.len())
            .any(|window| window == needle)
    );
}
