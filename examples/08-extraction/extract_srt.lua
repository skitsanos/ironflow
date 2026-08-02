local flow = Flow.new("extract_srt_demo")

flow:step("extract", nodes.extract_srt({
    path = "${ctx._flow_dir}/../fixtures/ironflow-transcript.srt",
    format = "text",
    metadata_key = "subtitles_meta"
}))

flow:step("summarize", function(ctx)
    local transcript = ctx.transcript or ""
    local cues = ctx.cues or {}
    local first_cue_bytes = 0
    if cues[1] and cues[1].text then first_cue_bytes = #cues[1].text end

    return {
        subtitle_summary = "Parsed " .. #cues
            .. " SRT cues; transcript bytes: " .. #transcript
            .. "; first cue bytes: " .. first_cue_bytes
    }
end):depends_on("extract")

flow:step("log", nodes.log({
    message = "${ctx.subtitle_summary}",
    level = "info"
})):depends_on("summarize")

return flow
