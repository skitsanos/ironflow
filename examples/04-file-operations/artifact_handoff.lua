-- Stream a binary document into the artifact store, then hand only its small
-- descriptor to an extractor. IRONFLOW_ARTIFACT_DIR defaults to data/artifacts.
-- Effects: publishes one immutable artifact; the local store does not auto-prune.
-- The extractor opens and verifies it inside the blocking worker, then parses
-- that same rewound handle; the store pathname never enters workflow context.
local flow = Flow.new("artifact_handoff")

flow:step("store_document", nodes.read_file({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.docx",
    encoding = "artifact",
    mime_type = "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    output_key = "document"
}))

flow:step("extract_document", nodes.extract_word({
    source_key = "document_artifact",
    format = "text",
    output_key = "document_text"
})):depends_on("store_document")

flow:step("summary", nodes.log({
    message = "Artifact ${ctx.document_artifact.artifact_uri} extracted successfully"
})):depends_on("extract_document")

return flow
