-- Two distinct topics for inspecting semantic boundaries.
-- Set OPENAI_API_KEY; OPENAI_BASE_URL can select a compatible embedding endpoint.
-- Real-model results can vary. Tests use orthogonal vectors to isolate the algorithm.
local flow = Flow.new("semantic_topic_boundary")

flow:step("prepare", function(ctx)
    if ctx.document ~= nil then
        return { document = ctx.document }
    end
    return { document = table.concat({
        "The database stores customer records.",
        "Tables organize records into rows.",
        "Indexes make database lookups faster.",
        "Transactions keep related updates consistent.",
        "Backups preserve copies of stored data.",
        "Queries retrieve rows from tables.",
        "Replication copies data between database servers.",
        "Database administrators monitor query performance.",
        "The orchestra rehearses a new symphony.",
        "Violins carry the main melody.",
        "The conductor sets the musical tempo.",
        "Cellos add a lower harmony.",
        "Brass instruments fill the concert hall.",
        "Percussion marks the rhythm.",
        "Musicians follow the notes on their scores.",
        "The concert ends with the final chord."
    }, " ") }
end)

flow:step("chunk", nodes.ai_chunk_semantic({
    source_key = "document",
    output_key = "topics",
    provider = "openai",
    model = "text-embedding-3-small",
    threshold = 0.5,
    min_distance = 2,
    sim_window = 3,
    sg_window = 11
})):depends_on("prepare")

flow:step("log", nodes.log({
    message = "Semantic chunks (${ctx.topics_count}): ${ctx.topics}"
})):depends_on("chunk")

return flow
