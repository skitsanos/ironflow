local flow = Flow.new("extract_pptx_demo")

-- Extract a PowerPoint deck as structured JSON with metadata and comments.
-- Embedded media, when present, is streamed to IRONFLOW_ARTIFACT_DIR and
-- represented by a small artifact descriptor; binary bytes never enter the
-- workflow context. Only internal OOXML image relationships are published;
-- external and non-image relationships are ignored. Published artifacts are
-- not automatically pruned.
flow:step("extract_deck", nodes.extract_pptx({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pptx",
    format = "json",
    media_mode = "artifact",
    output_key = "deck",
    metadata_key = "deck_meta",
    comments_key = "deck_comments"
}))

flow:step("summarize", function(ctx)
    local slides = ctx.deck and ctx.deck.slides or {}
    local comments = ctx.deck_comments or {}
    local first_title = "untitled"

    if #slides > 0 and slides[1].title then
        first_title = slides[1].title
    end

    return {
        deck_summary = "Slides: " .. #slides
            .. ", comments: " .. #comments
            .. ", first slide: " .. first_title
    }
end):depends_on("extract_deck")

flow:step("log_summary", nodes.log({
    message = "PPTX metadata: ${ctx.deck_meta}; ${ctx.deck_summary}"
})):depends_on("summarize")

return flow
