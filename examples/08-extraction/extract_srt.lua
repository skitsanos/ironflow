local flow = Flow.new("extract_srt_demo")

flow:step("extract", nodes.extract_srt({
    path = "data/samples/sample_subtitles.srt",
    format = "text",
    output_key = "subtitles",
    metadata_key = "subtitles_meta"
}))

flow:step("log", nodes.log({
    message = "Parsed ${ctx.subtitles_meta.cue_count} SRT cues",
    level = "info"
})):depends_on("extract")

flow:step("show", nodes.log({
    message = "Transcript: ${ctx.transcript}",
    level = "info"
})):depends_on("extract")

flow:step("show_cues", nodes.log({
    message = "Cue list keys: ${ctx.cues}",
    level = "info"
})):depends_on("extract")

return flow
