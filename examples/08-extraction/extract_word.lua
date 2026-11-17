local flow = Flow.new("extract_word_demo")

-- Extract once as structured JSON with optional metadata and comments.
-- XML references and CDATA are retained in body, metadata and comment text;
-- splitting a word across XML events or runs does not insert spaces.
-- A paragraph that anchors a text box keeps its text; the box paragraphs
-- follow it as separate blocks. Run flags honour w:val, so a run with
-- <w:b w:val="0"/> is not bold.
-- Bold, italic and strikethrough are resolved through word/styles.xml
-- (paragraph and character styles, w:basedOn chains, document defaults), so
-- a run that is bold only through its style reports bold = true.
-- In Markdown, adjacent runs with the same emphasis share one marker pair
-- and spaces stay outside the markers, however Word split the runs.
-- Metadata comes from the core-properties part named in _rels/.rels
-- (Office for the web writes a .psmdcp part), else docProps/core.xml.
flow:step("extract", nodes.extract_word({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.docx",
    format = "json",
    output_key = "document",
    metadata_key = "metadata",
    comments_key = "comments",
    -- Footnotes and endnotes are on by default (notes = true): a reference
    -- is the marker [^1] (endnotes [^e1]) and JSON lists the notes under
    -- footnotes / endnotes. Headers and footers are page furniture and are
    -- read only on request; JSON then adds headers / footers arrays.
    headers_footers = true
}))

-- Avoid logging the complete structured document.
flow:step("summarize", function(ctx)
    local blocks = ctx.document and ctx.document.blocks or {}
    local comments = ctx.comments or {}
    local footnotes = ctx.document and ctx.document.footnotes or {}
    local headers = ctx.document and ctx.document.headers or {}

    return {
        word_summary = "Blocks: " .. #blocks .. ", comments: " .. #comments
            .. ", footnotes: " .. #footnotes .. ", headers: " .. #headers
    }
end):depends_on("extract")

flow:step("show_summary", nodes.log({
    message = "Word summary: ${ctx.word_summary}"
})):depends_on("summarize")

return flow
