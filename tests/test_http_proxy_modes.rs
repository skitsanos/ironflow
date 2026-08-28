use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

struct Environment(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl Environment {
    fn proxy(url: &str) -> Self {
        let values = [
            ("HTTP_PROXY", url),
            ("http_proxy", url),
            ("ALL_PROXY", url),
            ("all_proxy", url),
            ("NO_PROXY", ""),
            ("no_proxy", ""),
        ];
        let originals = values
            .iter()
            .map(|(name, value)| {
                let original = std::env::var_os(name);
                // SAFETY: this integration-test process contains one test.
                unsafe { std::env::set_var(name, value) };
                (*name, original)
            })
            .collect();
        Self(originals)
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        // SAFETY: this integration-test process contains one test.
        unsafe {
            for (name, original) in self.0.iter().rev() {
                match original {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}

fn spawn_server(
    response_body: &'static str,
) -> (String, Arc<AtomicUsize>, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let server_hits = hits.clone();
    let task = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request);
        server_hits.fetch_add(1, Ordering::SeqCst);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
            response_body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    (format!("http://{address}"), hits, task)
}

async fn get(url: &str, proxy_mode: &str) -> serde_json::Value {
    NodeRegistry::with_builtins()
        .get("http_get")
        .unwrap()
        .execute(
            &serde_json::json!({
                "url": url,
                "proxy_mode": proxy_mode,
                "output_key": "response"
            }),
            &Context::new(),
        )
        .await
        .unwrap()["response_data"]
        .clone()
}

#[tokio::test]
async fn system_mode_uses_environment_proxy_while_direct_mode_bypasses_it() {
    let (target_url, target_hits, target_task) = spawn_server("{\"via\":\"target\"}");
    let (proxy_url, proxy_hits, proxy_task) = spawn_server("{\"via\":\"proxy\"}");
    let _environment = Environment::proxy(&proxy_url);

    assert_eq!(
        get(&target_url, "system").await,
        serde_json::json!({"via": "proxy"})
    );
    assert_eq!(
        get(&target_url, "direct").await,
        serde_json::json!({"via": "target"})
    );

    proxy_task.join().unwrap();
    target_task.join().unwrap();
    assert_eq!(proxy_hits.load(Ordering::SeqCst), 1);
    assert_eq!(target_hits.load(Ordering::SeqCst), 1);
}
