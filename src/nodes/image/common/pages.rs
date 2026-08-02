use anyhow::{Context, Result};

/// Parse a page specification into zero-based indices without allocating more
/// entries than the caller's operation-specific ceiling.
pub(crate) fn parse_pages_spec(
    spec: &str,
    page_count: usize,
    max_pages: u64,
    operation: &str,
    limit_variable: &str,
) -> Result<Vec<usize>> {
    let max_pages = usize::try_from(max_pages).unwrap_or(usize::MAX);
    if spec == "all" {
        admit(0, page_count, max_pages, operation, limit_variable)?;
        return Ok((0..page_count).collect());
    }

    let mut indices = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if let Some((start, end)) = part.split_once('-') {
            let start = parse_page(start)?;
            let end = parse_page(end)?;
            if start > end {
                anyhow::bail!("Invalid page range: {start}-{end}");
            }
            if end > page_count {
                anyhow::bail!("Page {end} exceeds document page count ({page_count})");
            }
            let count = end
                .checked_sub(start)
                .and_then(|count| count.checked_add(1))
                .context("page range length overflow")?;
            admit(indices.len(), count, max_pages, operation, limit_variable)?;
            indices.try_reserve_exact(count)?;
            indices.extend((start..=end).map(|page| page - 1));
        } else {
            let page = parse_page(part)?;
            if page > page_count {
                anyhow::bail!("Page {page} exceeds document page count ({page_count})");
            }
            admit(indices.len(), 1, max_pages, operation, limit_variable)?;
            indices.push(page - 1);
        }
    }
    if indices.is_empty() {
        anyhow::bail!("No pages specified");
    }
    Ok(indices)
}

fn parse_page(value: &str) -> Result<usize> {
    let value = value.trim();
    let page = value
        .parse::<usize>()
        .with_context(|| format!("Invalid page number: '{value}'"))?;
    if page == 0 {
        anyhow::bail!("Page numbers are 1-based, got 0");
    }
    Ok(page)
}

fn admit(
    retained: usize,
    additional: usize,
    maximum: usize,
    operation: &str,
    limit_variable: &str,
) -> Result<()> {
    let requested = retained.saturating_add(additional);
    if requested > maximum {
        anyhow::bail!(
            "{operation}: requested {requested} pages, exceeds {limit_variable} ({maximum})"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_limit_precedes_collection() {
        let error = parse_pages_spec("all", usize::MAX, 3, "render", "LIMIT")
            .unwrap_err()
            .to_string();
        assert!(error.contains("LIMIT (3)"), "{error}");

        let error = parse_pages_spec("1-3,1", 3, 3, "render", "LIMIT")
            .unwrap_err()
            .to_string();
        assert!(error.contains("requested 4"), "{error}");
        assert_eq!(
            parse_pages_spec("1-3", 3, 3, "render", "LIMIT").unwrap(),
            vec![0, 1, 2]
        );
    }
}
