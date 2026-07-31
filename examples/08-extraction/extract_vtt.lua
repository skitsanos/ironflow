local flow = Flow.new("extract_vtt_demo")

flow:step("extract", nodes.extract_vtt({
    path = "${ctx._flow_dir}/../fixtures/ironflow-transcript.vtt",
    format = "markdown",
    output_key = "subtitles_markdown",
    metadata_key = "subtitles_meta"
}))

flow:step("summarize", function(ctx)
    local transcript = ctx.transcript or ""
    local markdown = ctx.subtitles_markdown or ""
    local cues = ctx.cues or {}

    return {
        subtitle_summary = "Parsed " .. #cues
            .. " VTT cues; transcript bytes: " .. #transcript
            .. "; Markdown bytes: " .. #markdown
    }
end):depends_on("extract")

flow:step("log", nodes.log({
    message = "${ctx.subtitle_summary}",
    level = "info"
})):depends_on("summarize")

return flow
