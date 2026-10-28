use super::super::resource::Budget;
use anyhow::{Context as _, Result, ensure};
use lopdf::Object;
use std::sync::LazyLock;

// Derive byte lengths from lopdf's PDFDocEncoding table without duplicating it.
#[allow(clippy::expect_used)] // Infallible: decoding a one-byte PDFDocEncoding string is total for every byte value.
static PDF_DOC_LENGTHS: LazyLock<[usize; 256]> = LazyLock::new(|| {
    std::array::from_fn(|byte| {
        lopdf::decode_text_string(&Object::String(
            vec![byte as u8],
            lopdf::StringFormat::Literal,
        ))
        .expect("a single PDFDocEncoding byte is a valid text string")
        .len()
    })
});

pub(super) struct Output {
    text: String,
    limit: usize,
}

impl Output {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            text: String::new(),
            limit,
        }
    }

    fn admit(&self, bytes: usize) -> Result<()> {
        ensure!(
            bytes <= self.limit.saturating_sub(self.text.len()),
            "PDF text exceeds IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES"
        );
        Ok(())
    }

    pub(super) fn push(&mut self, text: &str, budget: &Budget<'_>) -> Result<()> {
        budget.checkpoint()?;
        self.admit(text.len())?;
        self.text
            .try_reserve(text.len())
            .context("cannot reserve bounded PDF text output")?;
        self.text.push_str(text);
        Ok(())
    }

    pub(super) fn newline(&mut self, budget: &Budget<'_>) -> Result<()> {
        if !self.text.ends_with('\n') {
            self.push("\n", budget)?;
        }
        Ok(())
    }

    pub(super) fn actual(
        &mut self,
        value: &Object,
        hidden: bool,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        let bytes = value
            .as_str()
            .context("PDF ActualText is not a text string")?;
        budget.charge_items(
            (bytes.len() as u64).div_ceil(4096),
            "PDF ActualText input chunks",
        )?;
        let length = if let Some(bytes) = bytes.strip_prefix(&[0xfe, 0xff]) {
            ensure!(
                bytes.len() % 2 == 0,
                "PDF ActualText has odd UTF-16 byte length"
            );
            let mut size = 0usize;
            for (index, c) in char::decode_utf16(
                bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| u16::from_be_bytes(*b)),
            )
            .enumerate()
            {
                if index % 1024 == 0 {
                    budget.checkpoint()?;
                }
                size = size
                    .checked_add(c.context("invalid UTF-16 PDF ActualText")?.len_utf8())
                    .context("PDF ActualText length overflow")?;
                if !hidden {
                    self.admit(size)?;
                }
            }
            size
        } else if let Some(bytes) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
            std::str::from_utf8(bytes)
                .context("invalid UTF-8 PDF ActualText")?
                .len()
        } else {
            let mut size = 0usize;
            for chunk in bytes.chunks(1024) {
                budget.checkpoint()?;
                for byte in chunk {
                    size = size
                        .checked_add(PDF_DOC_LENGTHS[*byte as usize])
                        .context("PDF ActualText length overflow")?;
                }
                if !hidden {
                    self.admit(size)?;
                }
            }
            size
        };
        if !hidden {
            self.admit(length)?;
            let text = lopdf::decode_text_string(value).context("cannot decode PDF ActualText")?;
            self.push(&text, budget)?;
        }
        Ok(())
    }

    pub(super) fn finish(self) -> String {
        self.text
    }
}
