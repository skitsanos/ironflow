local flow = Flow.new("extract_vtt_demo")

flow:step("extract", nodes.extract_vtt({
    path = "data/samples/sample_subtitles.vtt",
    format = "markdown",
    output_key = "subtitles",
    metadata_key = "subtitles_meta"
}))

flow:step("log", nodes.log({
    message = "Parsed ${ctx.subtitles_meta.cue_count} VTT cues",
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
