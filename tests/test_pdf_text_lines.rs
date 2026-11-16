//! Lines of extracted PDF text follow the position of the text, not the
//! `BT … ET` blocks it is written in (IF-222 f). The Quartz PDF writer ends
//! a block at every font change, so a word with one glyph from a fallback
//! font was returned on three lines.

#[path = "pdf_text_lines/fixture.rs"]
mod fixture;

use fixture::{block, document_with, extract, extract_document, font, operation, show, show_cids};
use lopdf::Object;

/// Text of size 12 in the simple font advances 6 units per glyph.
const ADVANCE: f32 = 6.0;

fn after(x: f32, glyphs: usize) -> f32 {
    x + ADVANCE * glyphs as f32
}

#[tokio::test]
async fn blocks_that_continue_a_line_are_joined() {
    let mut page = block(72.0, 700.0, 12.0, "F1", show("Cas"));
    // One glyph from the composite font, placed where the last block ended.
    page.extend(block(after(72.0, 3), 700.0, 12.0, "F2", show_cids(&[1])));
    page.extend(block(after(72.0, 4), 700.0, 12.0, "F1", show(" mare")));
    // The next line of the page.
    page.extend(block(72.0, 686.0, 12.0, "F1", show("ora")));
    // CID 2 is a quarter of an em wide.
    page.extend(block(after(72.0, 3), 686.0, 12.0, "F2", show_cids(&[2])));
    page.extend(block(after(72.0, 3) + 3.0, 686.0, 12.0, "F1", show("e")));
    assert_eq!(extract(page).await, "Cas\u{103} mare\nora\u{219}e\n");
}

#[tokio::test]
async fn a_gap_on_one_baseline_is_a_space() {
    let mut page = block(72.0, 700.0, 12.0, "F1", show("Total"));
    // A value in the next column of the same row, and a superscript whose
    // baseline is raised by less than half the text size.
    page.extend(block(400.0, 700.0, 12.0, "F1", show("42")));
    page.extend(block(after(400.0, 2), 704.0, 8.0, "F1", show("a")));
    // Text that starts left of where the line ended begins a new line.
    page.extend(block(72.0, 700.0, 12.0, "F1", show("overprint")));
    assert_eq!(extract(page).await, "Total 42a\noverprint\n");
}

#[tokio::test]
async fn moves_inside_one_block_start_new_lines() {
    let page = vec![
        operation("q", vec![]),
        // The page scale applies to positions and sizes alike.
        operation(
            "cm",
            vec![2.into(), 0.into(), 0.into(), 2.into(), 0.into(), 0.into()],
        ),
        operation("BT", vec![]),
        font("F1", 12.0),
        operation("Td", vec![36.into(), 350.into()]),
        show("first line"),
        operation("Td", vec![0.into(), (-14).into()]),
        show("second line"),
        operation("TD", vec![0.into(), (-14).into()]),
        show("third"),
        // Still on the third line: the move only skips a word space.
        operation("Td", vec![Object::Real(6.0 * 6.0), 0.into()]),
        show("line"),
        operation("T*", vec![]),
        show("fourth line"),
        operation("ET", vec![]),
        operation("Q", vec![]),
    ];
    assert_eq!(
        extract(page).await,
        "first line\nsecond line\nthird line\nfourth line\n"
    );
}

/// A font without widths gives no end position: its blocks stay on their
/// own lines, as they always were, unless the baseline moves.
#[tokio::test]
async fn text_of_unknown_width_keeps_one_line_per_block() {
    let mut page = block(72.0, 700.0, 12.0, "F3", show("left"));
    page.extend(block(200.0, 700.0, 12.0, "F3", show("right")));
    page.extend(vec![
        operation("BT", vec![]),
        font("F3", 12.0),
        operation("Td", vec![72.into(), 650.into()]),
        show("glued"),
        show("runs"),
        operation("Td", vec![0.into(), (-14).into()]),
        show("next"),
        operation("ET", vec![]),
    ]);
    assert_eq!(extract(page).await, "left\nright\ngluedruns\nnext\n");
}

/// A width table that cannot be read is not an error: the font's advances
/// are unknown and its blocks keep their own lines.
#[tokio::test]
async fn a_malformed_width_table_falls_back_to_one_line_per_block() {
    for table in [
        vec![1.into()],
        vec![1.into(), 2.into()],
        vec![5.into(), 2.into(), 500.into()],
        vec![1.into(), Object::Array(vec![Object::Null])],
        vec![Object::Null, Object::Array(vec![500.into()])],
        vec![(-1).into(), Object::Array(vec![500.into()])],
        vec![
            1.into(),
            Object::Array(vec![Object::Name(b"wide".to_vec())]),
        ],
    ] {
        let mut page = block(72.0, 700.0, 12.0, "F2", show_cids(&[1]));
        page.extend(block(78.0, 700.0, 12.0, "F2", show_cids(&[2])));
        // An odd number of bytes is not a string of two-byte codes.
        page.extend(block(84.0, 700.0, 12.0, "F2", show("odd")));
        let label = format!("{table:?}");
        let text = extract_document(document_with(table, page)).await;
        assert!(text.starts_with("\u{103}\n\u{219}\n"), "{label}: {text:?}");
    }
}
