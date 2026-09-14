use anyhow::Result;

pub(super) fn relationships_part(source: &str) -> String {
    let (directory, name) = source.rsplit_once('/').unwrap_or(("", source));
    if directory.is_empty() {
        format!("_rels/{name}.rels")
    } else {
        format!("{directory}/_rels/{name}.rels")
    }
}

pub(super) fn resolve_part(source: &str, target: &str) -> Result<String> {
    anyhow::ensure!(
        !target.is_empty(),
        "extract_pptx: empty relationship target"
    );
    anyhow::ensure!(
        !target.contains('\\'),
        "extract_pptx: relationship target contains backslashes"
    );
    anyhow::ensure!(
        !target.contains('?') && !target.contains('#'),
        "extract_pptx: relationship target has a query or fragment"
    );
    anyhow::ensure!(
        !target.bytes().any(|byte| byte.is_ascii_control()),
        "extract_pptx: relationship target contains control characters"
    );
    let target_path = target.strip_prefix('/').unwrap_or(target);
    anyhow::ensure!(
        !target_path
            .split('/')
            .next()
            .unwrap_or_default()
            .contains(':'),
        "extract_pptx: relationship target has a URI scheme"
    );
    anyhow::ensure!(
        !target_path.split('/').any(str::is_empty),
        "extract_pptx: relationship target has an empty segment"
    );
    anyhow::ensure!(
        !matches!(target_path.rsplit('/').next(), Some("." | "..")),
        "extract_pptx: relationship target names a directory"
    );
    let mut parts = if target.starts_with('/') {
        Vec::new()
    } else {
        source
            .rsplit_once('/')
            .map(|(directory, _)| directory.split('/').collect())
            .unwrap_or_default()
    };
    for segment in target_path.split('/') {
        match segment {
            "." => {}
            ".." => {
                anyhow::ensure!(
                    parts.pop().is_some(),
                    "extract_pptx: relationship escapes the package root"
                );
            }
            segment => parts.push(segment),
        }
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::{relationships_part, resolve_part};

    #[test]
    fn relationship_names_follow_the_source_part() {
        assert_eq!(
            relationships_part("custom/pages/page.xml"),
            "custom/pages/_rels/page.xml.rels"
        );
        assert_eq!(relationships_part("page.xml"), "_rels/page.xml.rels");
    }

    #[test]
    fn resolves_package_relative_paths_without_uri_decoding() {
        for (source, target, expected) in [
            (
                "ppt/presentation.xml",
                "slides/slide99.xml",
                "ppt/slides/slide99.xml",
            ),
            (
                "custom/pages/page.xml",
                "../notes/review.xml",
                "custom/notes/review.xml",
            ),
            (
                "custom/pages/page.xml",
                "/ppt/media/image.png",
                "ppt/media/image.png",
            ),
            ("page.xml", "image.png", "image.png"),
            (
                "ppt/presentation.xml",
                "./slides/../slide.xml",
                "ppt/slide.xml",
            ),
            (
                "ppt/presentation.xml",
                "slides/%2e%2e.xml",
                "ppt/slides/%2e%2e.xml",
            ),
            (
                "ppt/presentation.xml",
                "slides/caf\u{e9}.xml",
                "ppt/slides/caf\u{e9}.xml",
            ),
        ] {
            assert_eq!(resolve_part(source, target).unwrap(), expected);
        }
    }
}
