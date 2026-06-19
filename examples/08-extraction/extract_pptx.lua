local flow = Flow.new("extract_pptx_demo")

-- Extract a PowerPoint deck as structured JSON with metadata and comments.
flow:step("extract_deck", nodes.extract_pptx({
    path = "data/samples/sample.pptx",
    format = "json",
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
