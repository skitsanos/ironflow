//! Font encodings for one extraction: admitted, decoded once, then reused.

use std::collections::BTreeMap;

use anyhow::{Result, bail, ensure};
use lopdf::{Dictionary, Document, Encoding, Object, ObjectId};

use super::super::resource::Budget;
use super::{cmap_scan, text_output::Output};

/// UTF-8 bytes one decoded UTF-16 unit can become. lopdf sniffs a byte-order
/// mark in the decoded units, and a UTF-8 mark makes it replace each of a
/// unit's two bytes with a three-byte U+FFFD.
const UTF8_PER_UNIT: usize = 6;
/// UTF-8 bytes one byte of a single-byte or UTF-16 encoded string can become.
const UTF8_PER_BYTE: usize = 3;
/// A glyph string whose worst-case text fits this is decoded in one call.
const DIRECT_DECODE_BYTES: usize = 1024 * 1024;

pub(super) struct Fonts<'a> {
    doc: &'a Document,
    decoders: Vec<Decoder<'a>>,
    /// Font dictionaries by address: stable while the document is borrowed.
    by_font: BTreeMap<usize, usize>,
    max_cmap_entries: u64,
    cmap_entries: u64,
}

impl<'a> Fonts<'a> {
    pub(super) fn new(doc: &'a Document) -> Self {
        Self {
            doc,
            decoders: Vec::new(),
            by_font: BTreeMap::new(),
            max_cmap_entries: crate::util::limits::max_pdf_extract_cmap_entries(),
            cmap_entries: 0,
        }
    }

    /// The decoders of one page's fonts by resource name. A font object is
    /// admitted and decoded the first time any page uses it.
    pub(super) fn page(
        &mut self,
        page: ObjectId,
        stream_limit: usize,
        budget: &mut Budget<'_>,
    ) -> Result<BTreeMap<Vec<u8>, usize>> {
        let mut names = BTreeMap::new();
        for (name, font) in self.doc.get_page_fonts(page)? {
            budget.charge_item("PDF fonts")?;
            let key = std::ptr::from_ref(font).addr();
            let index = match self.by_font.get(&key) {
                Some(index) => *index,
                None => {
                    let decoder = self.decode(font, stream_limit, budget)?;
                    self.decoders.push(decoder);
                    self.by_font.insert(key, self.decoders.len() - 1);
                    self.decoders.len() - 1
                }
            };
            names.insert(name, index);
        }
        Ok(names)
    }

    pub(super) fn get(&self, index: usize) -> Option<&Decoder<'a>> {
        self.decoders.get(index)
    }

    fn decode(
        &mut self,
        font: &'a Dictionary,
        stream_limit: usize,
        budget: &Budget<'_>,
    ) -> Result<Decoder<'a>> {
        let mut longest_target = cmap_scan::MAX_TARGET_UNITS;
        // A stream that fails to decode here fails identically inside lopdf,
        // which then reports the limit or falls back without parsing a CMap.
        if let Ok(stream) = font
            .get_deref(b"ToUnicode", self.doc)
            .and_then(Object::as_stream)
            && let Ok(bytes) = stream.get_plain_content_with_limit(stream_limit)
        {
            let cost = cmap_scan::scan(&bytes, || budget.checkpoint())?;
            self.cmap_entries = self.cmap_entries.saturating_add(cost.entries);
            if self.cmap_entries > self.max_cmap_entries {
                bail!(
                    "PDF ToUnicode mappings exceed IRONFLOW_MAX_PDF_EXTRACT_CMAP_ENTRIES ({})",
                    self.max_cmap_entries
                );
            }
            longest_target = cost.longest_target;
        }
        // lopdf cannot be interrupted while it builds the maps admitted above.
        budget.checkpoint()?;
        let encoding = font.get_font_encoding_with_limit(self.doc, stream_limit)?;
        budget.checkpoint()?;
        Ok(Decoder::new(encoding, longest_target))
    }
}

pub(super) struct Decoder<'a> {
    encoding: Encoding<'a>,
    /// Upper bound on the UTF-8 bytes one glyph-string byte decodes to.
    expansion: usize,
    /// Each byte decodes on its own, so the string can be decoded in pieces.
    bytewise: bool,
}

impl<'a> Decoder<'a> {
    fn new(encoding: Encoding<'a>, longest_target: usize) -> Self {
        let bytewise = !matches!(
            encoding,
            Encoding::UnicodeMapEncoding(_)
                | Encoding::SimpleEncoding(b"UniGB-UCS2-H" | b"UniGB-UTF16-H")
        );
        Self {
            expansion: expansion(&encoding, longest_target),
            encoding,
            bytewise,
        }
    }

    /// Append the text of one glyph string. lopdf materializes a whole operand
    /// before any limit is compared, so an operand is decoded in one call only
    /// when its worst case is small or fits the remaining output budget.
    pub(super) fn show(
        &self,
        bytes: &[u8],
        output: &mut Output,
        budget: &Budget<'_>,
    ) -> Result<()> {
        let worst_case = bytes.len().saturating_mul(self.expansion);
        if worst_case <= output.remaining().max(DIRECT_DECODE_BYTES) {
            return output.push(&self.encoding.bytes_to_string(bytes)?, budget);
        }
        ensure!(
            self.bytewise,
            "PDF glyph string of {} bytes can decode to {worst_case} bytes, \
             exceeds IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES",
            bytes.len()
        );
        for chunk in bytes.chunks((DIRECT_DECODE_BYTES / self.expansion).max(1)) {
            output.push(&self.encoding.bytes_to_string(chunk)?, budget)?;
        }
        Ok(())
    }
}

fn expansion(encoding: &Encoding<'_>, longest_target: usize) -> usize {
    match encoding {
        Encoding::UnicodeMapEncoding(_) => UTF8_PER_UNIT.saturating_mul(longest_target.max(1)),
        // Bytes without a difference are decoded one at a time by the base.
        Encoding::Differences(differences) => {
            expansion(&differences.base, longest_target).max(UTF8_PER_BYTE)
        }
        _ => UTF8_PER_BYTE,
    }
}

#[cfg(test)]
mod tests;
