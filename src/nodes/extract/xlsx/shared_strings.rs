//! Bounded preflight for calamine's eagerly loaded shared-string table.

use std::io::{BufReader, Read, Seek};
use std::path::Path;

use anyhow::Result;
use quick_xml::Reader;
use quick_xml::events::{BytesRef, BytesStart, Event};

use crate::util::execution::ExecutionControl;

const PART_SUFFIX: &str = "/sharedstrings.xml";

/// Validate every resource dimension that calamine's `read_shared_strings`
/// trusts before `open_workbook` can reserve or decode the table.
pub(super) fn check<R: Read + Seek>(
    reader: R,
    path: &Path,
    max_strings: u64,
    max_bytes: u64,
    execution: Option<&ExecutionControl>,
) -> Result<()> {
    let mut archive = zip::ZipArchive::new(BufReader::new(reader)).map_err(|error| {
        anyhow::anyhow!(
            "extract_xlsx: '{}' is not a readable workbook: {error}",
            path.display()
        )
    })?;

    let mut shared_index = None;
    for index in 0..archive.len() {
        checkpoint(execution)?;
        let entry = archive.by_index_raw(index).map_err(|error| {
            anyhow::anyhow!(
                "extract_xlsx: '{}' has an unreadable zip entry: {error}",
                path.display()
            )
        })?;
        let normalized = entry.name().replace('\\', "/").to_ascii_lowercase();
        if normalized == "sharedstrings.xml" || normalized.ends_with(PART_SUFFIX) {
            if shared_index.replace(index).is_some() {
                anyhow::bail!(
                    "extract_xlsx: '{}' contains multiple sharedStrings.xml parts",
                    path.display()
                );
            }
            check_declared_sizes(&entry, max_bytes)?;
        }
    }

    let Some(index) = shared_index else {
        return Ok(());
    };
    checkpoint(execution)?;
    let entry = archive.by_index(index).map_err(|error| {
        anyhow::anyhow!(
            "extract_xlsx: '{}' has an unreadable sharedStrings.xml: {error}",
            path.display()
        )
    })?;
    inspect(entry, max_strings, max_bytes, execution)
}

fn check_declared_sizes<R: Read>(entry: &zip::read::ZipFile<'_, R>, max_bytes: u64) -> Result<()> {
    if entry.compressed_size() > max_bytes {
        anyhow::bail!(
            "extract_xlsx: sharedStrings.xml compressed size {} exceeds \
             IRONFLOW_MAX_XLSX_OUTPUT_BYTES ({max_bytes})",
            entry.compressed_size()
        );
    }
    if entry.size() > max_bytes {
        anyhow::bail!(
            "extract_xlsx: sharedStrings.xml declared uncompressed size {} exceeds \
             IRONFLOW_MAX_XLSX_OUTPUT_BYTES ({max_bytes})",
            entry.size()
        );
    }
    Ok(())
}

fn inspect<R: Read>(
    entry: R,
    max_strings: u64,
    max_bytes: u64,
    execution: Option<&ExecutionControl>,
) -> Result<()> {
    let mut xml = Reader::from_reader(BufReader::with_capacity(
        8 * 1024,
        CappedRead::new(entry, max_bytes),
    ));
    let config = xml.config_mut();
    config.check_end_names = false;
    config.check_comments = false;
    config.expand_empty_elements = true;
    config.trim_text(false);

    let mut buffer = Vec::with_capacity(8 * 1024);
    let mut saw_sst = false;
    let mut ended_sst = false;
    let mut si_depth = 0_u64;
    let mut string_count = 0_u64;
    let mut decoded_bytes = 0_u64;

    loop {
        checkpoint(execution)?;
        buffer.clear();
        match xml.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) if !saw_sst && event.local_name().as_ref() == b"sst" => {
                saw_sst = true;
                if let Some(count) = unique_count(&event)? {
                    check_string_count(count, max_strings, "declared uniqueCount")?;
                }
            }
            Ok(Event::Start(event))
                if saw_sst && !ended_sst && event.local_name().as_ref() == b"si" =>
            {
                string_count = string_count.saturating_add(1);
                check_string_count(string_count, max_strings, "actual shared-string count")?;
                si_depth = si_depth.saturating_add(1);
            }
            Ok(Event::End(event)) if si_depth > 0 && event.local_name().as_ref() == b"si" => {
                si_depth -= 1;
            }
            Ok(Event::End(event)) if saw_sst && event.local_name().as_ref() == b"sst" => {
                ended_sst = true;
            }
            Ok(Event::Text(text)) if si_depth > 0 => {
                charge_decoded(
                    &mut decoded_bytes,
                    text.xml10_content()?.len() as u64,
                    max_bytes,
                )?;
            }
            Ok(Event::CData(text)) if si_depth > 0 => {
                charge_decoded(
                    &mut decoded_bytes,
                    text.xml10_content()?.len() as u64,
                    max_bytes,
                )?;
            }
            Ok(Event::GeneralRef(reference)) if si_depth > 0 => {
                charge_decoded(
                    &mut decoded_bytes,
                    reference_output_bytes(&reference)?,
                    max_bytes,
                )?;
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                anyhow::bail!("extract_xlsx: invalid or oversized sharedStrings.xml: {error}")
            }
        }
    }

    if !saw_sst || !ended_sst {
        anyhow::bail!("extract_xlsx: sharedStrings.xml is missing a complete sst element");
    }
    Ok(())
}

fn unique_count(event: &BytesStart<'_>) -> Result<Option<u64>> {
    for attribute in event.attributes() {
        let attribute = attribute.map_err(|error| {
            anyhow::anyhow!("extract_xlsx: invalid sharedStrings.xml attribute: {error}")
        })?;
        if attribute.key.as_ref() == b"uniqueCount" {
            let value = std::str::from_utf8(attribute.value.as_ref())?
                .parse::<u64>()
                .map_err(|_| {
                    anyhow::anyhow!(
                        "extract_xlsx: sharedStrings.xml uniqueCount must be an unsigned integer"
                    )
                })?;
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn check_string_count(count: u64, max_strings: u64, source: &str) -> Result<()> {
    if count > max_strings {
        anyhow::bail!(
            "extract_xlsx: sharedStrings.xml {source} {count} exceeds the safe table bound \
             derived from IRONFLOW_MAX_XLSX_CELLS ({max_strings})"
        );
    }
    Ok(())
}

fn charge_decoded(total: &mut u64, added: u64, max_bytes: u64) -> Result<()> {
    *total = total.saturating_add(added);
    if *total > max_bytes {
        anyhow::bail!(
            "extract_xlsx: sharedStrings.xml decoded text exceeds \
             IRONFLOW_MAX_XLSX_OUTPUT_BYTES ({max_bytes})"
        );
    }
    Ok(())
}

fn reference_output_bytes(reference: &BytesRef<'_>) -> Result<u64> {
    if let Some(character) = reference.resolve_char_ref()? {
        return Ok(character.len_utf8() as u64);
    }
    let name = reference.decode()?;
    Ok(match name.as_ref() {
        "lt" | "gt" | "amp" | "apos" | "quot" => 1,
        // Calamine rejects unknown entities. Counting their literal width is
        // conservative until it does so and prevents the preflight itself
        // from treating them as free.
        other => other.len().saturating_add(2) as u64,
    })
}

fn checkpoint(execution: Option<&ExecutionControl>) -> Result<()> {
    if let Some(execution) = execution {
        execution.checkpoint()?;
    }
    Ok(())
}

struct CappedRead<R> {
    inner: R,
    remaining: u64,
    max_bytes: u64,
}

impl<R> CappedRead<R> {
    fn new(inner: R, max_bytes: u64) -> Self {
        Self {
            inner,
            remaining: max_bytes,
            max_bytes,
        }
    }
}

impl<R: Read> Read for CappedRead<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            let mut proof = [0_u8; 1];
            return match self.inner.read(&mut proof)? {
                0 => Ok(0),
                _ => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "sharedStrings.xml actual uncompressed bytes exceed \
                         IRONFLOW_MAX_XLSX_OUTPUT_BYTES ({})",
                        self.max_bytes
                    ),
                )),
            };
        }

        let allowed = self.remaining.min(buffer.len() as u64) as usize;
        let read = self.inner.read(&mut buffer[..allowed])?;
        self.remaining -= read as u64;
        Ok(read)
    }
}

#[cfg(test)]
#[path = "shared_strings/tests.rs"]
mod tests;
