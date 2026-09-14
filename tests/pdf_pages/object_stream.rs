use lopdf::{Document, Object, Stream, dictionary};

pub fn document(padding: usize) -> Document {
    let mut doc = Document::with_version("1.5");
    let pages = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica"
    });
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 10 50 Td (STREAM_PAGE_TEXT) Tj ET".to_vec(),
    ));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages, "Contents" => content,
        "MediaBox" => vec![0.into(), 0.into(), 200.into(), 100.into()],
        "Resources" => dictionary! {"Font" => dictionary! {"F1" => font}}
    });
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1
        }),
    );
    let catalog = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages});
    let info = doc.add_object(dictionary! {
        "Title" => Object::string_literal("STREAM_TITLE"),
        "Padding" => Object::string_literal(vec![b'x'; padding])
    });
    doc.trailer.set("Root", catalog);
    doc.trailer.set("Info", info);
    doc
}

pub fn encoded(padding: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    document(padding).save_modern(&mut bytes).unwrap();
    bytes
}

pub fn encrypted(padding: usize, user_password: &str) -> Vec<u8> {
    let mut doc = Document::load_mem(&encoded(padding)).unwrap();
    // The writer drops existing ObjStm containers. Preserve one as a regular
    // stream, then restore its equal-length type name in this controlled fixture.
    for object in doc.objects.values_mut() {
        if let Ok(stream) = object.as_stream_mut()
            && stream.dict.has_type(b"ObjStm")
        {
            stream.dict.set("Type", "ObjTmp");
        }
    }
    let id = Object::string_literal(vec![0x42; 16]);
    doc.trailer.set("ID", vec![id.clone(), id]);
    let state = lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V2 {
        document: &doc,
        owner_password: "fixture-owner",
        user_password,
        key_length: 128,
        permissions: lopdf::Permissions::PRINTABLE,
    })
    .unwrap();
    doc.encrypt(&state).unwrap();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    restore_type(&mut bytes, b"/ObjTmp", b"/ObjStm");
    bytes
}

pub fn xref_expansion(retained_only: bool) -> Vec<u8> {
    use lopdf::xref::{XrefEntry, XrefType};
    use std::io::Write;

    let mut doc = document(0);
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let xref_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let mut first = Vec::new();
    doc.save_to(&mut first).unwrap();
    let loaded = Document::load_mem(&first).unwrap();
    let mut rows = vec![0; 7];
    for id in 1..=xref_id.0 {
        let XrefEntry::Normal { offset, generation } = loaded.reference_table.entries[&id] else {
            panic!("fixture needs normal objects");
        };
        rows.push(1);
        rows.extend_from_slice(&offset.to_be_bytes());
        rows.extend_from_slice(&generation.to_be_bytes());
    }
    // Additional free xref entries compress well without adding document objects.
    let size = xref_id.0 + 1 + 256;
    rows.resize(size as usize * 7, 0);
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "XReZ", "Size" => size,
            "W" => vec![1.into(), 4.into(), 2.into()],
            "Root" => doc.trailer.get(b"Root").unwrap().clone(),
            "Info" => doc.trailer.get(b"Info").unwrap().clone()
        },
        rows,
    );
    stream.compress().unwrap();
    doc.set_object(xref_id, stream);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    restore_type(&mut bytes, b"/XReZ", b"/XRef");
    let XrefEntry::Normal { offset, .. } = loaded.reference_table.entries[&xref_id.0] else {
        unreachable!();
    };
    // All earlier objects retain their byte positions. Either select this xref
    // or keep it as an unused retained stream behind the original valid table.
    if !retained_only {
        write!(bytes, "\nstartxref\n{offset}\n%%EOF\n").unwrap();
    }
    bytes
}

fn restore_type(bytes: &mut [u8], from: &[u8], to: &[u8]) {
    assert_eq!(from.len(), to.len());
    let positions: Vec<_> = bytes
        .windows(from.len())
        .enumerate()
        .filter_map(|(position, value)| (value == from).then_some(position))
        .collect();
    assert_eq!(positions.len(), 1);
    bytes[positions[0]..positions[0] + to.len()].copy_from_slice(to);
}
