local flow = Flow.new("extract_srt_demo")

flow:step("extract", nodes.extract_srt({
    path = "${ctx._flow_dir}/../fixtures/ironflow-transcript.srt",
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
})):depends_on("log")

flow:step("show_cues", nodes.log({
    message = "Cue list keys: ${ctx.cues}",
    level = "info"
})):depends_on("show")

return flow
