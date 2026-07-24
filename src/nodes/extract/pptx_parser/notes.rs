pub(in crate::nodes::extract) fn parse_pptx_notes(xml: &str) -> String {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut text = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref event)) => {
                let raw = String::from_utf8_lossy(event.name().as_ref()).to_string();
                in_text = raw.rsplit(':').next().unwrap_or(&raw) == "t";
            }
            Ok(Event::Text(ref event)) if in_text => {
                text.push_str(&String::from_utf8_lossy(event.as_ref()));
                text.push('\n');
            }
            Ok(Event::End(_)) => in_text = false,
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    text.trim().to_string()
}
