# Tool-authored extraction fixtures

Files written by an authoring tool rather than assembled by hand, used by
`tests/test_tool_authored_documents.rs` (IF-222). Each binary sits next to the
Markdown it was generated from.

| File | Source | Tool |
| --- | --- | --- |
| `pandoc-deck.pptx` | `pandoc-deck.md` | pandoc 3.14 |
| `pandoc-document.docx` | `pandoc-document.md` | pandoc 3.14 |

Regenerate with a fixed timestamp so the packages stay stable:

```bash
SOURCE_DATE_EPOCH=1794700800 pandoc pandoc-deck.md -o pandoc-deck.pptx
SOURCE_DATE_EPOCH=1794700800 pandoc pandoc-document.md -o pandoc-document.docx
```

The deck has speaker notes with a hard line break and a slide without notes.
The document has style-based headings, direct bold, italic and strikethrough,
a hard line break, a comment with an anchored range, a tracked insertion and
deletion, and a table.

The text content is original test content. The styles, themes, slide masters
and layouts inside the packages come from pandoc's default reference
documents; these files are therefore kept apart from the CC0 set in
`examples/fixtures/`. pandoc writes no text boxes, presentation comments,
tracked formatting changes, or header, date and footer notes placeholders.
