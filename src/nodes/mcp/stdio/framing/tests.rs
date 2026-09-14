use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::poll;
use serde_json::json;
use tokio::io::{AsyncRead, BufReader, ReadBuf, duplex};

use super::*;

fn error_kind(error: StdioTransportError) -> io::ErrorKind {
    match error {
        StdioTransportError::Io(error) => error.kind(),
        other => panic!("expected I/O error, got {other}"),
    }
}

#[tokio::test]
async fn cancelled_reads_preserve_every_split_including_utf8_and_crlf() {
    let payload = "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"text\":\"caf\u{e9}\"}}";
    let wire = format!("{payload}\r\n").into_bytes();
    for split in 1..wire.len() {
        let (mut server, client) = duplex(1024);
        let mut frames = FrameReader::new(BufReader::new(client), wire.len());
        server.write_all(&wire[..split]).await.unwrap();
        assert!(poll!(Box::pin(frames.read())).is_pending());
        assert!(frames.reader.buffer().is_empty(), "prefix was not consumed");
        server.write_all(&wire[split..]).await.unwrap();
        server.write_all(b"{}\n").await.unwrap();
        assert_eq!(
            frames.read().await.unwrap().unwrap(),
            payload.as_bytes(),
            "split {split}"
        );
        assert_eq!(frames.read().await.unwrap().unwrap(), b"{}");
    }
}

#[tokio::test]
async fn outgoing_ping_reply_does_not_discard_a_pending_response() {
    let (server, client) = duplex(1024);
    let mut server = BufReader::new(server);
    let (input, mut output) = tokio::io::split(client);
    let mut frames = FrameReader::new(BufReader::new(input), 1024);
    let wire = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n";
    let reply =
        serde_json::from_value(json!({"jsonrpc": "2.0", "id": "ping", "result": {}})).unwrap();
    server.write_all(&wire[..12]).await.unwrap();
    // Force the read to consume the prefix before the outgoing branch wins.
    tokio::select! {
        biased;
        result = frames.read() => panic!("partial response completed: {result:?}"),
        () = std::future::ready(()) => write_frame(&mut output, &reply, 1024).await.unwrap(),
    }
    let mut actual_reply = Vec::new();
    server.read_until(b'\n', &mut actual_reply).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&actual_reply).unwrap()["id"],
        "ping"
    );
    server.write_all(&wire[12..]).await.unwrap();
    assert_eq!(
        frames.read().await.unwrap().unwrap(),
        &wire[..wire.len() - 1]
    );
}

#[tokio::test]
async fn repeated_cancellation_cannot_reset_the_frame_limit() {
    let (mut server, client) = duplex(1024);
    let mut frames = FrameReader::new(BufReader::new(client), 8);
    for byte in b"12345678" {
        server.write_all(&[*byte]).await.unwrap();
        assert!(poll!(Box::pin(frames.read())).is_pending());
    }
    server.write_all(b"\n").await.unwrap();
    assert_eq!(
        error_kind(frames.read().await.unwrap_err()),
        io::ErrorKind::InvalidData
    );
    assert_eq!(frames.frame.len(), 9);
    server
        .write_all(b"more bytes must not be consumed")
        .await
        .unwrap();
    assert_eq!(
        error_kind(frames.read().await.unwrap_err()),
        io::ErrorKind::InvalidData
    );
    assert_eq!(frames.frame.len(), 9);
}

#[tokio::test]
async fn eof_after_a_cancelled_prefix_is_not_clean_eof() {
    let (mut server, client) = duplex(1024);
    let mut frames = FrameReader::new(BufReader::new(client), 1024);
    server.write_all(b"partial").await.unwrap();
    assert!(poll!(Box::pin(frames.read())).is_pending());
    drop(server);
    assert_eq!(
        error_kind(frames.read().await.unwrap_err()),
        io::ErrorKind::UnexpectedEof
    );
}

#[tokio::test]
async fn complete_lines_keep_existing_delimiter_limit_and_eof_rules() {
    for (wire, limit, expected) in [
        (b"ab\n".as_slice(), 3, b"ab".as_slice()),
        (b"ab\r\n", 4, b"ab"),
        (b"\n", 1, b""),
    ] {
        let mut frames = FrameReader::new(wire, limit);
        assert_eq!(frames.read().await.unwrap().unwrap(), expected);
        assert!(frames.read().await.unwrap().is_none());
    }
    for wire in [b"ab\n".as_slice(), b"ab\r\n", b"oversized"] {
        let mut frames = FrameReader::new(wire, 2);
        assert_eq!(
            error_kind(frames.read().await.unwrap_err()),
            io::ErrorKind::InvalidData
        );
    }
    let mut frames = FrameReader::new(b"ab".as_slice(), 2);
    assert_eq!(
        error_kind(frames.read().await.unwrap_err()),
        io::ErrorKind::UnexpectedEof
    );
    let mut frames = FrameReader::new(b"".as_slice(), 0);
    assert!(frames.read().await.unwrap().is_none());
}

struct FailedInput;

impl AsyncRead for FailedInput {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::new(
            io::ErrorKind::ConnectionReset,
            "injected input failure",
        )))
    }
}

#[tokio::test]
async fn input_io_errors_are_propagated_after_partial_data() {
    let input = b"prefix".as_slice().chain(FailedInput);
    let mut frames = FrameReader::new(BufReader::new(input), 1024);
    assert_eq!(
        error_kind(frames.read().await.unwrap_err()),
        io::ErrorKind::ConnectionReset
    );
}

#[tokio::test]
async fn outgoing_limit_is_checked_before_writing_any_bytes() {
    let message =
        serde_json::from_value(json!({"jsonrpc": "2.0", "id": "ping", "result": {}})).unwrap();
    let serialized = serde_json::to_vec(&message).unwrap();
    let mut output = Vec::new();
    assert_eq!(
        error_kind(
            write_frame(&mut output, &message, serialized.len() - 1)
                .await
                .unwrap_err()
        ),
        io::ErrorKind::InvalidData
    );
    assert!(output.is_empty());
    write_frame(&mut output, &message, serialized.len())
        .await
        .unwrap();
    assert_eq!(output, [serialized, vec![b'\n']].concat());
}
