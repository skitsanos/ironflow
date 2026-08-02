use anyhow::Result;

pub(super) struct Base64Admission {
    per_source: u64,
    cumulative: u64,
    used: u64,
    report_cumulative: bool,
}

impl Base64Admission {
    pub(super) fn new(per_source: u64, cumulative: u64, report_cumulative: bool) -> Self {
        Self {
            per_source,
            cumulative,
            used: 0,
            report_cumulative,
        }
    }

    pub(super) fn admit(&mut self, data: &str) -> Result<()> {
        let decoded = preflight_base64_bytes(data, self.per_source)?;
        let next = self.used.saturating_add(decoded);
        if next > self.cumulative {
            let variable = if self.report_cumulative {
                "IRONFLOW_MAX_IMAGE_TO_PDF_ENCODED_BYTES"
            } else {
                "IRONFLOW_MAX_IMAGE_ENCODED_BYTES"
            };
            anyhow::bail!(
                "image_to_pdf: cumulative decoded Base64 input exceeds {variable} ({})",
                self.cumulative
            );
        }
        self.used = next;
        Ok(())
    }
}

pub(crate) fn preflight_base64_bytes(data: &str, maximum: u64) -> Result<u64> {
    let length = data.len();
    let complete = length / 4;
    let remainder = length % 4;
    if remainder == 1 {
        anyhow::bail!("base64 image has an invalid encoded length");
    }
    let mut decoded = complete
        .checked_mul(3)
        .and_then(|bytes| bytes.checked_add([0, 0, 1, 2][remainder]))
        .and_then(|bytes| u64::try_from(bytes).ok())
        .unwrap_or(u64::MAX);
    if remainder == 0 {
        let padding = data
            .as_bytes()
            .iter()
            .rev()
            .take(2)
            .take_while(|byte| **byte == b'=')
            .count() as u64;
        decoded = decoded.saturating_sub(padding);
    }
    if decoded > maximum {
        anyhow::bail!(
            "base64 image exceeds IRONFLOW_MAX_IMAGE_ENCODED_BYTES ({maximum} decoded bytes)"
        );
    }
    Ok(decoded)
}
