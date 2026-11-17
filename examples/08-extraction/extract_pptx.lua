local flow = Flow.new("extract_pptx_demo")

-- Extract a PowerPoint deck as structured JSON with metadata and comments.
-- Slide text, notes, metadata and comments retain XML references and CDATA.
-- Metadata is read from the core-properties part named in _rels/.rels,
-- falling back to docProps/core.xml.
-- Notes separate paragraphs rather than individual XML text fragments.
-- A line break (a:br) inside a slide or notes paragraph is a newline in the
-- text; in a table cell and in the slide title it is a space.
-- Notes are the body placeholder of the notes slide only: its slide number,
-- date, header and footer placeholders and field values are not included.
-- A paragraph is a list item (list_level, "- " in Markdown) when a bullet is
-- declared for it in its own properties or in the shape's list style; a
-- first-level bullet has list_level 0. A placeholder paragraph that declares
-- nothing inherits the bullet of its slide layout and slide master.
-- Footer, date, slide-number and header placeholders are page furniture and
-- are left out of slide text in every format.
-- Slides follow the presentation list, including listed hidden slides.
-- In JSON a hidden slide carries hidden = true; a shown slide has no such
-- field. Text and Markdown do not mark hidden slides.
-- Notes/comments/images follow relationships, not matching filename numbers.
-- Both comment formats are read: legacy comments (idx) and PowerPoint modern
-- comments (id, optional status such as "resolved", replies nested under
-- the comment). A slide with a modern comments part reports only those; its
-- legacy copy, if any, is not read.
-- Embedded media, when present, is streamed to IRONFLOW_ARTIFACT_DIR and
-- represented by a small artifact descriptor; binary bytes never enter the
-- workflow context. Only internal OOXML image relationships are published;
-- external and non-image targets on image elements are ignored. Published artifacts are
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

    assert(ctx.deck_meta.slide_count == #slides, "Slide count must match membership")
    for index, slide in ipairs(slides) do
        assert(slide.slide_index == index, "Slide indices must follow presentation order")
        assert(slide.hidden == nil or slide.hidden == true, "hidden is set only on hidden slides")
        for _, comment in ipairs(slide.comments or {}) do
            assert(comment.slide_index == index, "Comment must refer to its slide position")
            assert(comment.replies == nil or comment.id ~= nil, "Only modern comments have replies")
            for _, reply in ipairs(comment.replies or {}) do
                assert(reply.slide_index == nil, "Replies are nested, not flat comments")
            end
        end
    end

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
