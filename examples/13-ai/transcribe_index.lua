-- Audio to searchable vectors: transcribe, parse the cues, chunk them while
-- preserving timecodes, and embed the chunk text ready for indexing.
-- Transcription and embedding both default to the "openai" provider, so only
-- OPENAI_API_KEY is required. Validates statically but is not part of the
-- offline matrix since it needs network access and a real credential.
local flow = Flow.new("transcribe_index")

flow:step("transcribe", nodes.transcribe({
    path = "${ctx.audio_path}",
    format = "vtt",
    output_key = "transcript",
    output_file = "${ctx.audio_path}.vtt"
})):retries(2, 2):timeout(300)

-- extract_vtt reads a file, which is why transcribe wrote one.
flow:step("cues", nodes.extract_vtt({
    path = "${ctx.transcript_path}",
    cues_key = "cues"
})):depends_on("transcribe")

flow:step("chunks", nodes.ai_chunk({
    mode = "cues",
    source_key = "cues",
    size = 1200,
    output_key = "chunks"
})):depends_on("cues")

flow:step("vectors", nodes.ai_embed({
    input_key = "chunks_texts",
    output_key = "embedding"
})):depends_on("chunks")

return flow
