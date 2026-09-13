use std::io::{BufReader, Read};

use quick_xml::Reader;
use quick_xml::events::{BytesCData, BytesRef, BytesStart};

use super::*;

struct Fragmented<'a>(&'a [u8]);
impl Read for Fragmented<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = buffer.len().min(1);
        self.0.read(&mut buffer[..count])
    }
}

#[test]
fn fragmented_utf8_events_decode_once_without_recursive_unescaping() {
    let xml = "<r>A&amp;B &#233; &#x1F680;<![CDATA[ &amp; <x>\r\n]]>&amp;amp;</r>";
    let mut reader = Reader::from_reader(BufReader::new(Fragmented(xml.as_bytes())));
    let mut decoder = Decoder::default();
    let mut buffer = Vec::new();
    let mut output = String::new();
    let mut admitted = 0;
    loop {
        let event = decoder
            .decode(reader.read_event_into(&mut buffer).unwrap(), |bytes| {
                admitted += bytes;
                Ok(())
            })
            .unwrap();
        match event {
            Event::Text(text) => output.push_str(text.as_ref()),
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    assert_eq!(output, "A&B \u{e9} \u{1f680} &amp; <x>\n&amp;");
    assert!(admitted >= output.len() as u64);
    assert!(admitted <= xml.len() as u64);
}

#[test]
fn declaration_controls_text_and_attribute_normalization() {
    for (version, expected) in [("1.0", "a\u{85}b\u{2028}c"), ("1.1", "a\nb\nc")] {
        let xml = format!("<?xml version=\"{version}\"?><r>a\u{85}b\u{2028}c</r>");
        let mut reader = Reader::from_str(&xml);
        let mut decoder = Decoder::default();
        let mut output = String::new();
        loop {
            match decoder
                .decode(reader.read_event().unwrap(), |_| Ok(()))
                .unwrap()
            {
                Event::Text(text) => output.push_str(text.as_ref()),
                Event::Eof => break,
                _ => {}
            }
        }
        assert_eq!(output, expected);
        let start = BytesStart::from_content("r a=\"x\t\r\n&amp;&#xA;&amp;amp;\u{85}\"", 1);
        let attribute = start.attributes().next().unwrap().unwrap();
        let value = attribute_value(&attribute, decoder.version, |_| Ok(())).unwrap();
        assert_eq!(
            value,
            format!(
                "x  &\n&amp;{}",
                if version == "1.0" { "\u{85}" } else { " " }
            )
        );
    }
}

#[test]
fn unknown_and_invalid_references_are_errors_in_text_and_attributes() {
    for reference in [
        "unknown", "nbsp", "#0", "#xD800", "#x110000", "#xZZ", "#-1", "#1", "#xFFFF",
    ] {
        let error = Decoder::default()
            .decode(Event::GeneralRef(BytesRef::new(reference)), |_| Ok(()))
            .unwrap_err();
        assert!(format!("{error:#}").contains("XML"));
        let start = BytesStart::from_content(format!("r a=\"&{reference};\""), 1);
        let attribute = start.attributes().next().unwrap().unwrap();
        assert!(
            attribute_value(&attribute, XmlVersion::Implicit1_0, |_| Ok(())).is_err(),
            "{reference}"
        );
    }
}

#[test]
fn admission_rejects_text_cdata_references_and_attributes_before_expansion() {
    for event in [
        Event::Text(BytesText::new("1234")),
        Event::CData(BytesCData::new("1234")),
        Event::GeneralRef(BytesRef::new("#x1F680")),
    ] {
        let mut charges = Vec::new();
        let error = Decoder::default()
            .decode(event, |bytes| {
                charges.push(bytes);
                anyhow::ensure!(bytes <= 3, "test budget exhausted");
                Ok(())
            })
            .unwrap_err();
        assert_eq!(error.to_string(), "test budget exhausted");
        assert_eq!(charges, [4]);
    }
    let start = BytesStart::from_content("r a=\"&amp;\"", 1);
    let attribute = start.attributes().next().unwrap().unwrap();
    let error = attribute_value(&attribute, XmlVersion::Implicit1_0, |_| {
        anyhow::bail!("no capacity")
    })
    .unwrap_err();
    assert_eq!(error.to_string(), "no capacity");
}

#[test]
fn repeated_references_consume_cumulative_byte_capacity() {
    let mut used = 0;
    let mut decoder = Decoder::default();
    for index in 0..3 {
        let result = decoder.decode(Event::GeneralRef(BytesRef::new("#x1F680")), |bytes| {
            used += bytes;
            anyhow::ensure!(used <= 8, "test cumulative limit");
            Ok(())
        });
        assert_eq!(result.is_ok(), index < 2);
    }
}

#[test]
fn dtd_expansion_is_rejected_without_resolving_entities() {
    let xml = "<!DOCTYPE r [<!ENTITY a 'payload'><!ENTITY b '&a;&a;&a;'>]><r>&b;</r>";
    let mut reader = Reader::from_str(xml);
    let error = Decoder::default()
        .decode(reader.read_event().unwrap(), |_| {
            panic!("DTD must not allocate decoded text")
        })
        .unwrap_err();
    assert_eq!(error.to_string(), "XML DTDs are not supported");
}
