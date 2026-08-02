use std::io::{Read, Write};

use anyhow::Result;

use crate::util::execution::ExecutionControl;

const COPY_CHUNK_BYTES: usize = 64 * 1024;

pub(super) fn copy_with_control<R, W>(
    reader: &mut R,
    writer: &mut W,
    execution: &ExecutionControl,
    max_bytes: u64,
    operation: &str,
) -> Result<u64>
where
    R: Read,
    W: Write,
{
    let mut copied = 0u64;
    let mut buffer = [0_u8; COPY_CHUNK_BYTES];

    loop {
        execution.checkpoint()?;
        let read = reader.read(&mut buffer)?;
        execution.checkpoint()?;
        if read == 0 {
            return Ok(copied);
        }

        copied = copied.saturating_add(read as u64);
        if copied > max_bytes {
            anyhow::bail!(
                "{operation}: actual data exceeds the remaining uncompressed byte limit {max_bytes}"
            );
        }
        writer.write_all(&buffer[..read])?;
        execution.checkpoint()?;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    struct EndlessReader {
        started: Option<mpsc::Sender<()>>,
    }

    impl Read for EndlessReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if let Some(started) = self.started.take() {
                let _ = started.send(());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
            buffer.fill(b'x');
            Ok(buffer.len())
        }
    }

    #[tokio::test]
    async fn dropped_waiter_stops_chunk_copy() {
        let (started_tx, started_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let waiter = tokio::spawn(crate::util::execution::run_blocking_step(
            move |execution| {
                let result = copy_with_control(
                    &mut EndlessReader {
                        started: Some(started_tx),
                    },
                    &mut std::io::sink(),
                    &execution,
                    u64::MAX,
                    "test",
                );
                let _ = finished_tx.send(result.map_err(|error| error.to_string()));
                Ok(())
            },
        ));

        tokio::task::spawn_blocking(move || started_rx.recv())
            .await
            .unwrap()
            .unwrap();
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::task::spawn_blocking(move || finished_rx.recv()),
        )
        .await
        .expect("ZIP copy worker ignored cancellation")
        .unwrap()
        .unwrap();
        let error = result.unwrap_err();
        assert!(error.contains("step execution cancelled"), "{error}");
    }
}
