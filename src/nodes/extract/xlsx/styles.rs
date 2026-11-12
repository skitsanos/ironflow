//! Bounded preflight for the cell-format table calamine builds at open.
//!
//! Since calamine 0.38, `Xlsx::new` turns every `<xf>` of `cellXfs` into an
//! owned `Style`: a fixed-size record plus private copies of the font name
//! and number-format code the record refers to. `styles.xml` stores each
//! string once and the open copies it once per record, so a workbook of a few
//! kilobytes can retain gigabytes before the first cell is read (IF-173). The
//! archive guard cannot see this: it bounds the bytes of the part, not the
//! product calamine derives from them.
//!
//! The bound is deliberately coarser than calamine's own bookkeeping. It
//! counts every record-producing element wherever it appears and multiplies
//! by the longest strings in the part, so it stays an upper bound however the
//! reader resolves font and format references.

use std::io::{BufReader, Read, Seek};
use std::path::Path;

use anyhow::{Result, bail};
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::util::execution::ExecutionControl;
use crate::util::zip_preflight::Admitted;

const PART_NAME: &str = "styles.xml";
const PART_SUFFIX: &str = "/styles.xml";

/// Charged for each font, fill, border and cell-format element. calamine's
/// `Style` is 152 bytes; the rest covers vector growth, the built-in format
/// code a record copies when it names no custom one, and allocator overhead.
const RECORD_BYTES: u64 = 320;

/// Refuse a workbook whose style table calamine would expand beyond
/// `max_bytes` when `open_workbook` reads it.
///
/// calamine reads `styles.xml` from the directory of the workbook part and
/// matches part names without regard to case or separator. Every part that
/// could be that one is checked, so the result does not depend on which
/// directory the package relationships name.
pub(super) fn check<R: Read + Seek>(
    reader: R,
    path: &Path,
    admitted: Admitted,
    max_bytes: u64,
    execution: Option<&ExecutionControl>,
) -> Result<()> {
    let mut archive = zip::ZipArchive::new(BufReader::new(reader)).map_err(|error| {
        anyhow::anyhow!(
            "extract_xlsx: '{}' is not a readable workbook: {error}",
            path.display()
        )
    })?;
    // A separate open of the same bytes: hold it to the admitted directory.
    admitted.confirm(&archive, "extract_xlsx")?;

    let mut parts = Vec::new();
    for index in 0..archive.len() {
        checkpoint(execution)?;
        let entry = archive.by_index_raw(index).map_err(|error| {
            anyhow::anyhow!(
                "extract_xlsx: '{}' has an unreadable zip entry: {error}",
                path.display()
            )
        })?;
        let normalized = entry.name()?.replace('\\', "/").to_ascii_lowercase();
        if normalized == PART_NAME || normalized.ends_with(PART_SUFFIX) {
            parts.push(index);
        }
    }

    for index in parts {
        checkpoint(execution)?;
        let entry = archive.by_index(index).map_err(|error| {
            anyhow::anyhow!(
                "extract_xlsx: '{}' has an unreadable styles.xml: {error}",
                path.display()
            )
        })?;
        inspect(entry, max_bytes, execution)?;
    }
    Ok(())
}

fn inspect<R: Read>(part: R, max_bytes: u64, execution: Option<&ExecutionControl>) -> Result<()> {
    // One byte past the limit tells an oversized part from one ending at it.
    let capped = part.take(max_bytes.saturating_add(1));
    let mut xml = Reader::from_reader(BufReader::with_capacity(8 * 1024, capped));
    // calamine's reader configuration, so both parsers see the same events.
    let config = xml.config_mut();
    config.check_end_names = false;
    config.check_comments = false;
    config.expand_empty_elements = true;
    config.trim_text(false);

    let mut buffer = Vec::with_capacity(8 * 1024);
    let mut table = StyleTable::default();
    let mut font_depth = 0_u64;
    loop {
        checkpoint(execution)?;
        buffer.clear();
        match xml.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) => {
                // The whole tag bounds any attribute value inside it, with no
                // need to agree with calamine on how attributes are parsed.
                let tag_bytes = element.len() as u64;
                match element.local_name().as_ref() {
                    "xf" => table.formats = table.formats.saturating_add(1),
                    "font" => {
                        font_depth = font_depth.saturating_add(1);
                        table.other_records = table.other_records.saturating_add(1);
                    }
                    "fill" | "border" => {
                        table.other_records = table.other_records.saturating_add(1);
                    }
                    "numFmt" => table.longest_code = table.longest_code.max(tag_bytes),
                    _ if font_depth > 0 => {
                        table.longest_name = table.longest_name.max(tag_bytes);
                    }
                    _ => {}
                }
                table.check(max_bytes)?;
            }
            Ok(Event::End(element))
                if font_depth > 0 && element.local_name().as_ref() == "font" =>
            {
                font_depth -= 1;
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => bail!("extract_xlsx: invalid styles.xml: {error}"),
        }
    }

    if xml.buffer_position() > max_bytes {
        bail!(
            "extract_xlsx: styles.xml actual uncompressed bytes exceed \
             IRONFLOW_MAX_XLSX_OUTPUT_BYTES ({max_bytes})"
        );
    }
    Ok(())
}

/// What one `styles.xml` asks calamine to build.
#[derive(Default)]
struct StyleTable {
    /// `<xf>` elements: each becomes a retained `Style`.
    formats: u64,
    /// `<font>`, `<fill>` and `<border>` elements: records built while parsing.
    other_records: u64,
    /// Longest tag inside any `<font>`, an upper bound on a font name.
    longest_name: u64,
    /// Longest `<numFmt>` tag, an upper bound on a custom format code.
    longest_code: u64,
}

impl StyleTable {
    /// An upper bound on the bytes calamine retains for this table: every
    /// record at a fixed cost, and the longest name and code once per format.
    fn retained_bytes(&self) -> u64 {
        let records = self.formats.saturating_add(self.other_records);
        let strings = self.longest_name.saturating_add(self.longest_code);
        records
            .saturating_mul(RECORD_BYTES)
            .saturating_add(self.formats.saturating_mul(strings))
    }

    fn check(&self, max_bytes: u64) -> Result<()> {
        let retained = self.retained_bytes();
        if retained > max_bytes {
            bail!(
                "extract_xlsx: styles.xml holds {} style records that may expand to {retained} \
                 bytes when the workbook opens, exceeding IRONFLOW_MAX_XLSX_OUTPUT_BYTES \
                 ({max_bytes}). Raise that variable to read it.",
                self.formats.saturating_add(self.other_records)
            );
        }
        Ok(())
    }
}

fn checkpoint(execution: Option<&ExecutionControl>) -> Result<()> {
    if let Some(execution) = execution {
        execution.checkpoint()?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "styles/tests.rs"]
mod tests;
