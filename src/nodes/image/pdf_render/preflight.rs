//! Decoded-size admission for the streams one rendered page can reach.
//!
//! The shared loader bounds object and cross-reference streams. Rendering
//! also decodes content, form, pattern, font and image streams, so each one
//! reachable from a page is decoded here under the per-stream limit, and the
//! streams of one execution share a cumulative budget.

use std::collections::HashSet;

use anyhow::Result;
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::util::execution::ExecutionControl;

/// Cumulative decoded bytes admitted for one execution, as a multiple of the
/// per-stream limit.
const CUMULATIVE_STREAMS: u64 = 8;

pub(super) struct StreamBudget {
    per_stream: u64,
    remaining: u64,
    visited: HashSet<ObjectId>,
}

impl StreamBudget {
    pub(super) fn current() -> Self {
        Self::new(crate::util::limits::max_pdf_decompressed_stream_bytes())
    }

    fn new(per_stream: u64) -> Self {
        Self {
            per_stream,
            remaining: per_stream.saturating_mul(CUMULATIVE_STREAMS),
            visited: HashSet::new(),
        }
    }

    /// Admit every stream reachable from `page`, visiting each object once
    /// per execution.
    pub(super) fn admit_page(
        &mut self,
        document: &Document,
        page: ObjectId,
        operation: &str,
        execution: &ExecutionControl,
    ) -> Result<()> {
        let mut pending = vec![page];
        while let Some(id) = pending.pop() {
            if !self.visited.insert(id) {
                continue;
            }
            execution.checkpoint()?;
            let Ok(object) = document.get_object(id) else {
                continue;
            };
            if let Object::Stream(stream) = object {
                self.admit_stream(stream, operation)?;
            }
            collect_references(object, &mut pending);
        }
        Ok(())
    }

    fn admit_stream(&mut self, stream: &lopdf::Stream, operation: &str) -> Result<()> {
        let limit = self.per_stream.min(self.remaining);
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        match stream.get_plain_content_with_limit(limit) {
            Ok(decoded) => {
                self.remaining = self.remaining.saturating_sub(decoded.len() as u64);
                Ok(())
            }
            Err(lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded {
                ..
            })) => anyhow::bail!(
                "{operation}: PDF streams needed for rendering exceed \
                 IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES ({} per stream, {} per execution)",
                self.per_stream,
                self.per_stream.saturating_mul(CUMULATIVE_STREAMS)
            ),
            // A filter this decoder does not implement (image codecs) or a
            // damaged stream: the renderer bounds images by pixel count and
            // skips what it cannot decode.
            Err(_) => Ok(()),
        }
    }
}

fn collect_references(object: &Object, pending: &mut Vec<ObjectId>) {
    match object {
        Object::Reference(id) => pending.push(*id),
        Object::Array(items) => items
            .iter()
            .for_each(|item| collect_references(item, pending)),
        Object::Dictionary(dictionary) => collect_dictionary(dictionary, pending),
        Object::Stream(stream) => collect_dictionary(&stream.dict, pending),
        _ => {}
    }
}

/// `Parent` and `P` point back up the page tree and would pull in every
/// other page of the document.
fn collect_dictionary(dictionary: &Dictionary, pending: &mut Vec<ObjectId>) {
    for (key, value) in dictionary.iter() {
        if key.as_slice() != b"Parent" && key.as_slice() != b"P" {
            collect_references(value, pending);
        }
    }
}

#[cfg(test)]
mod tests {
    use lopdf::{Stream, dictionary};

    use super::*;

    fn page_with_streams(count: usize, bytes: usize) -> (Document, ObjectId) {
        let mut document = Document::with_version("1.7");
        let forms: Vec<Object> = (0..count)
            .map(|_| {
                let mut stream = Stream::new(dictionary! {}, vec![b' '; bytes]);
                stream.compress().unwrap();
                document.add_object(stream).into()
            })
            .collect();
        let page = document.add_object(dictionary! { "Type" => "Page", "Contents" => forms });
        // A parent link must not pull other pages into this page's budget.
        let sibling = document.add_object(Stream::new(dictionary! {}, vec![b' '; bytes * 64]));
        let parent = document.add_object(dictionary! { "Kids" => vec![sibling.into()] });
        document
            .get_dictionary_mut(page)
            .unwrap()
            .set("Parent", parent);
        (document, page)
    }

    #[tokio::test]
    async fn streams_are_bounded_one_by_one_and_together() {
        crate::util::execution::run_tracked_blocking_step(|execution| {
            let (document, page) = page_with_streams(8, 1024);
            let mut budget = StreamBudget::new(1024);
            budget.admit_page(&document, page, "test", &execution)?;
            assert_eq!(budget.remaining, 0);
            // A page already admitted costs nothing the second time.
            budget.admit_page(&document, page, "test", &execution)?;

            let (document, page) = page_with_streams(9, 1024);
            let error = StreamBudget::new(1024)
                .admit_page(&document, page, "test", &execution)
                .unwrap_err();
            assert!(error.to_string().contains("8192 per execution"), "{error}");

            let (document, page) = page_with_streams(1, 1025);
            let error = StreamBudget::new(1024)
                .admit_page(&document, page, "test", &execution)
                .unwrap_err();
            assert!(error.to_string().contains("1024 per stream"), "{error}");
            Ok(())
        })
        .await
        .unwrap();
    }
}
