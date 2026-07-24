use std::collections::HashMap;

/// Parse `word/theme/theme1.xml` into OOXML theme-name to hex-color mappings.
pub(in crate::nodes::extract) fn parse_theme_colors(
    archive: &mut zip::ZipArchive<std::fs::File>,
) -> HashMap<String, String> {
    let mut colors = HashMap::new();
    let xml = match archive.by_name("word/theme/theme1.xml") {
        Ok(entry) => match crate::util::bounded_read::read_to_string_capped(
            entry,
            crate::util::limits::max_zip_uncompressed_bytes(),
            "extract_word",
        ) {
            Ok(xml) => xml,
            Err(_) => return colors,
        },
        Err(_) => return colors,
    };

    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut buf = Vec::new();
    let mut in_scheme = false;
    let mut current_role = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref event) | Event::Empty(ref event)) => {
                let raw = String::from_utf8_lossy(event.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                if local == "clrScheme" {
                    in_scheme = true;
                } else if in_scheme {
                    if let Some(role) = canonical_role(local) {
                        current_role = Some(role);
                    } else if (local == "srgbClr" || local == "sysClr")
                        && let Some(role) = current_role
                    {
                        let attr_name: &[u8] = if local == "srgbClr" {
                            b"val"
                        } else {
                            b"lastClr"
                        };
                        for attr in event.attributes().flatten() {
                            if attr.key.as_ref() == attr_name {
                                colors.insert(
                                    role.to_string(),
                                    String::from_utf8_lossy(&attr.value).to_uppercase(),
                                );
                            }
                        }
                    }
                }
            }
            Ok(Event::End(ref event)) => {
                let raw = String::from_utf8_lossy(event.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                if local == "clrScheme" {
                    in_scheme = false;
                    current_role = None;
                } else if canonical_role(local).is_some() {
                    current_role = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    colors
}

fn canonical_role(local: &str) -> Option<&'static str> {
    match local {
        "dk1" => Some("dark1"),
        "lt1" => Some("light1"),
        "dk2" => Some("dark2"),
        "lt2" => Some("light2"),
        "accent1" => Some("accent1"),
        "accent2" => Some("accent2"),
        "accent3" => Some("accent3"),
        "accent4" => Some("accent4"),
        "accent5" => Some("accent5"),
        "accent6" => Some("accent6"),
        "hlink" => Some("hyperlink"),
        "folHlink" => Some("followedHyperlink"),
        _ => None,
    }
}
