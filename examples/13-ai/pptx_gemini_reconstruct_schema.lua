--[[
PPTX + Gemini JSON-schema reconstruction demo.

Flow:
1. Extract the PPTX as structured JSON.
2. Build batches from however many slides the deck actually contains.
3. Fan the batches out through a reusable Gemini child workflow.
4. Combine the ordered schema-constrained results and write JSON + text files.

Environment variables:
- GEMINI_API_KEY

Dependencies:
- POSIX-compatible `mkdir` command.

Run:
  cargo run -- --dotenv .env run examples/13-ai/pptx_gemini_reconstruct_schema.lua

Effects:
- Retains a UUID-scoped output directory containing reconstruction.json and
  reconstruction.txt; remove it when finished.

Notes:
- JSON schema keeps each response parseable, but it does not remove model output limits.
- Runtime batching covers decks of different lengths without hard-coded LLM steps.
- `max_concurrent` bounds provider pressure while preserving result order.
]]

local flow = Flow.new("pptx_gemini_reconstruct_schema")

local TEMP_ROOT = env("TMPDIR")
if TEMP_ROOT == nil or TEMP_ROOT == "" then TEMP_ROOT = env("TMP") end
if TEMP_ROOT == nil or TEMP_ROOT == "" then TEMP_ROOT = env("TEMP") end
if TEMP_ROOT == nil or TEMP_ROOT == "" then TEMP_ROOT = "." end
local OUTPUT_DIR = TEMP_ROOT .. "/ironflow-pptx-gemini-schema-" .. uuid4()

flow:step("check_key", function()
    if not env("GEMINI_API_KEY") or env("GEMINI_API_KEY") == "" then
        error("GEMINI_API_KEY is required")
    end

    return { gemini_key_available = true }
end)

flow:step("prepare_output_dir", nodes.shell_command({
    cmd = "mkdir",
    args = { "-p", OUTPUT_DIR },
    timeout = 10,
    output_key = "prepare_output_dir"
})):depends_on("check_key")

flow:step("extract_deck", nodes.extract_pptx({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pptx",
    format = "json",
    output_key = "deck",
    metadata_key = "deck_meta",
    comments_key = "deck_comments"
})):depends_on("prepare_output_dir")

flow:step("prepare_batches", function(ctx)
    local batch_size = 10
    local slides = ctx.deck and ctx.deck.slides or {}
    local image_count = 0
    local batches = {}

    for _, slide in ipairs(slides) do
        for _, element in ipairs(slide.elements or {}) do
            if element.type == "image" then
                image_count = image_count + 1
            end
        end
    end

    for first = 1, #slides, batch_size do
        local last = math.min(first + batch_size - 1, #slides)
        local selected = {}
        for slide_index = first, last do
            table.insert(selected, slides[slide_index])
        end

        table.insert(batches, {
            slide_start = first,
            slide_end = last,
            payload = json_stringify({
                deck_metadata = ctx.deck_meta,
                slide_start = first,
                slide_end = last,
                slides = selected
            })
        })
    end

    return {
        _pptx_batches = batches,
        selected_slide_count = #slides,
        selected_image_count = image_count
    }
end):depends_on("extract_deck")

flow:step("reconstruct_batches", nodes.parallel_subworkflows({
    flow = "pptx_gemini_reconstruct_batch.lua",
    source_key = "_pptx_batches",
    item_key = "_batch",
    index_key = "_batch_index",
    child_output_key = "batch",
    max_concurrent = 2,
    output_key = "schema_batches"
})):depends_on("prepare_batches")

flow:step("combine", function(ctx)
    local slides = {}
    local seen = {}
    local text_parts = {}

    for _, entry in ipairs(ctx.schema_batches or {}) do
        for _, slide in ipairs((entry.batch and entry.batch.slides) or {}) do
            if not seen[slide.slide_index] then
                seen[slide.slide_index] = true
                table.insert(slides, slide)
            end
        end
    end

    table.sort(slides, function(a, b)
        return a.slide_index < b.slide_index
    end)

    for _, slide in ipairs(slides) do
        table.insert(text_parts, "Slide " .. slide.slide_index)

        local lines = slide.lines or {}
        local title_in_lines = false
        for _, line in ipairs(lines) do
            if line == slide.title then title_in_lines = true end
        end
        if slide.title and slide.title ~= "" and not title_in_lines then
            table.insert(text_parts, slide.title)
        end
        for _, line in ipairs(lines) do
            table.insert(text_parts, line)
        end

        for _, note in ipairs(slide.image_notes or {}) do
            table.insert(text_parts, "[Image] " .. note)
        end

        table.insert(text_parts, "")
    end

    return {
        batch_count = ctx.schema_batches_count,
        parsed_slide_count = #slides,
        full_reconstruction_json = json_stringify({ slides = slides }),
        full_reconstruction_text = table.concat(text_parts, "\n")
    }
end):depends_on("reconstruct_batches")

flow:step("write_json", nodes.write_file({
    path = OUTPUT_DIR .. "/reconstruction.json",
    content = "${ctx.full_reconstruction_json}"
})):depends_on("combine")

flow:step("write_text", nodes.write_file({
    path = OUTPUT_DIR .. "/reconstruction.txt",
    content = "${ctx.full_reconstruction_text}"
})):depends_on("write_json")

flow:step("log_result", nodes.log({
    message = "Parsed ${ctx.parsed_slide_count} slides in ${ctx.batch_count} schema batches. Outputs: " .. OUTPUT_DIR .. "/reconstruction.json and " .. OUTPUT_DIR .. "/reconstruction.txt"
})):depends_on("write_text")

return flow
