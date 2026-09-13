//! Shared load-time policy for lopdf consumers; callers bound encoded input.

use std::io::Read;

use anyhow::Result;
use lopdf::{Document, LoadOptions};

use super::execution::ExecutionControl;

#[derive(Clone, Copy)]
struct Limits {
    stream_bytes: usize,
    objects: u64,
}

impl Limits {
    fn current() -> Self {
        Self {
            stream_bytes: super::limits::max_pdf_decompressed_stream_bytes().min(isize::MAX as u64)
                as usize,
            objects: super::limits::max_pdf_objects(),
        }
    }

    fn options(self) -> LoadOptions {
        LoadOptions {
            // Lenient mode silently skips an oversized object stream.
            strict: true,
            max_decompressed_size: Some(self.stream_bytes),
            ..LoadOptions::default()
        }
    }
}

pub(crate) fn from_bytes(
    bytes: &[u8],
    operation: &str,
    execution: &ExecutionControl,
) -> Result<Document> {
    load(Limits::current(), operation, execution, |options| {
        Document::load_mem_with_options(bytes, options)
    })
}

pub(crate) fn from_reader(
    reader: impl Read,
    operation: &str,
    execution: &ExecutionControl,
) -> Result<Document> {
    load(Limits::current(), operation, execution, |options| {
        Document::load_from_with_options(reader, options)
    })
}

fn load(
    limits: Limits,
    operation: &str,
    execution: &ExecutionControl,
    parse: impl FnOnce(LoadOptions) -> lopdf::Result<Document>,
) -> Result<Document> {
    execution.checkpoint()?;
    let document = parse(limits.options()).map_err(|error| load_error(operation, error))?;
    execution.checkpoint()?;
    if document.objects.len() as u64 > limits.objects {
        anyhow::bail!(
            "{operation}: loaded PDF has {} objects, exceeds IRONFLOW_MAX_PDF_OBJECTS ({})",
            document.objects.len(),
            limits.objects
        );
    }
    if document.is_encrypted() {
        anyhow::bail!(
            "{operation}: PDF requires a password; password-protected input is unsupported"
        );
    }

    // lopdf 0.45 may recover from a failed xref decode or suppress encrypted
    // object-stream errors even in strict mode. Recheck retained streams on
    // those paths so oversized retained streams cannot escape admission.
    for object in document.objects.values() {
        execution.checkpoint()?;
        if let Ok(stream) = object.as_stream()
            && (stream.dict.has_type(b"XRef")
                || (document.was_encrypted() && stream.dict.has_type(b"ObjStm")))
        {
            stream
                .get_plain_content_with_limit(limits.stream_bytes)
                .map_err(|error| load_error(operation, error))?;
            execution.checkpoint()?;
        }
    }
    Ok(document)
}

fn load_error(operation: &str, error: lopdf::Error) -> anyhow::Error {
    if let lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded { limit }) = error {
        anyhow::anyhow!(
            "{operation}: PDF stream exceeds IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES ({limit})"
        )
    } else {
        anyhow::anyhow!("{operation}: failed to parse PDF: {error}")
    }
}

#[cfg(test)]
mod tests;
