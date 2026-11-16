//! A loopback listener for guard tests: it counts accepted connections and
//! answers each with a fixed HTTP response.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

pub(crate) struct Listener {
    pub(crate) port: u16,
    hits: Arc<AtomicUsize>,
}

impl Listener {
    /// Answer every connection with `200` and a small JSON body.
    pub(crate) fn ok() -> Self {
        Self::respond("200 OK", "")
    }

    /// Answer every connection with a redirect to `location`.
    pub(crate) fn redirect(status: &'static str, location: &str) -> Self {
        Self::respond(status, &format!("location: {location}\r\n"))
    }

    fn respond(status: &'static str, headers: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let headers = headers.to_owned();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                counter.fetch_add(1, Ordering::SeqCst);
                let headers = headers.clone();
                std::thread::spawn(move || {
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
                    let mut buffer = [0_u8; 16 * 1024];
                    let _ = stream.read(&mut buffer);
                    let body = r#"{"ok":true}"#;
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status}\r\n{headers}content-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                });
            }
        });
        Self { port, hits }
    }

    pub(crate) fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }

    pub(crate) fn url(&self, host: &str) -> String {
        format!("http://{host}:{}", self.port)
    }
}
