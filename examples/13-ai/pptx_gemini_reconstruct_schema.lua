--[[
PPTX + Gemini JSON-schema reconstruction demo.

Flow:
1. Extract the PPTX as structured JSON.
2. Split the deck into small slide batches.
3. Ask Gemini to reconstruct each batch with response_format=json_schema.
4. Parse the schema-constrained responses and write combined JSON + text files.

Environment variables:
- GEMINI_API_KEY

Run:
  cargo run -- --dotenv .env run examples/13-ai/pptx_gemini_reconstruct_schema.lua

Outputs:
- /tmp/ironflow-pptx-gemini-schema/reconstruction.json
- /tmp/ironflow-pptx-gemini-schema/reconstruction.txt

Notes:
- JSON schema keeps each response parseable, but it does not remove model output limits.
- Batching keeps each completion small enough to cover the full sample deck.
- Tool calls are exposed by nodes.llm as tool-call metadata; IronFlow does not execute
  model-requested tools inside a single llm node call.
]]

local flow = Flow.new("pptx_gemini_reconstruct_schema")

local OUTPUT_DIR = "/tmp/ironflow-pptx-gemini-schema"

local batches = {
    { name = "01_10", first = 1, last = 10 },
    { name = "11_20", first = 11, last = 20 },
    { name = "21_30", first = 21, last = 30 },
    { name = "31_40", first = 31, last = 40 },
    { name = "41_50", first = 41, last = 50 },
    { name = "51_60", first = 51, last = 60 },
    { name = "61_70", first = 61, last = 70 },
    { name = "71_80", first = 71, last = 80 },
    { name = "81_86", first = 81, last = 86 }
}

local function response_format_schema()
    return {
        type = "json_schema",
        json_schema = {
            name = "pptx_slide_reconstruction_batch",
            strict = true,
            schema = {
                type = "object",
                additionalProperties = false,
                properties = {
                    slides = {
                        type = "array",
                        items = {
                            type = "object",
                            additionalProperties = false,
                            properties = {
                                slide_index = { type = "integer" },
                                title = { type = "string" },
                                lines = {
                                    type = "array",
                                    items = { type = "string" }
                                },
                                image_notes = {
                                    type = "array",
                                    items = { type = "string" }
                                }
                            },
                            required = { "slide_index", "title", "lines", "image_notes" }
                        }
                    }
                },
                required = { "slides" }
            }
        }
    }
end

local function gemini_schema_step(batch, depends_on)
    local step_name = "schema_" .. batch.name
    local payload_key = "payload_" .. batch.name
    local prompt = [[
Reconstruct slides ]] .. batch.first .. [[ through ]] .. batch.last .. [[ from this structured PPTX extraction.

Rules:
- Preserve all meaningful content.
- Do not summarize.
- Do not invent missing content.
- Write each slide as text lines in human reading order.
- Keep the original slide_index value.
- Include image descriptions only when extracted alt text carries meaningful information.
- Return only JSON matching the required schema.

Structured PPTX extraction JSON:
${ctx.]] .. payload_key .. [[}
]]

    return flow:step(step_name, nodes.llm({
        provider = "custom",
        mode = "chat",
        model = "gemini-3.5-flash",
        base_url = "https://generativelanguage.googleapis.com/v1beta/openai",
        auth_type = "bearer",
        api_key = env("GEMINI_API_KEY"),
        max_tokens = 20000,
        temperature = 0.0,
        timeout = 180,
        output_key = step_name,
        messages = {
            {
                role = "user",
                content = prompt
            }
        },
        extra = {
            response_format = response_format_schema()
        }
    })):depends_on(depends_on)
end

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
    path = "data/samples/sample.pptx",
    format = "json",
    output_key = "deck",
    metadata_key = "deck_meta",
    comments_key = "deck_comments"
})):depends_on("prepare_output_dir")

flow:step("prepare_batches", function(ctx)
    local batch_ranges = {
        { name = "01_10", first = 1, last = 10 },
        { name = "11_20", first = 11, last = 20 },
        { name = "21_30", first = 21, last = 30 },
        { name = "31_40", first = 31, last = 40 },
        { name = "41_50", first = 41, last = 50 },
        { name = "51_60", first = 51, last = 60 },
        { name = "61_70", first = 61, last = 70 },
        { name = "71_80", first = 71, last = 80 },
        { name = "81_86", first = 81, last = 86 }
    }
    local slides = ctx.deck and ctx.deck.slides or {}
    local image_count = 0
    local output = {
        selected_slide_count = #slides
    }

    for _, slide in ipairs(slides) do
        for _, element in ipairs(slide.elements or {}) do
            if element.type == "image" then
                image_count = image_count + 1
            end
        end
    end

    output.selected_image_count = image_count

    for _, batch in ipairs(batch_ranges) do
        local selected = {}
        local last = math.min(batch.last, #slides)

        if batch.first <= #slides then
            for slide_index = batch.first, last do
                table.insert(selected, slides[slide_index])
            end
        end

        output["payload_" .. batch.name] = json_stringify({
            deck_metadata = ctx.deck_meta,
            slide_start = batch.first,
            slide_end = last,
            slides = selected
        })
    end

    return output
end):depends_on("extract_deck")

local previous_step = "prepare_batches"
for _, batch in ipairs(batches) do
    gemini_schema_step(batch, previous_step)
    previous_step = "schema_" .. batch.name
end

flow:step("combine", function(ctx)
    local batch_ranges = {
        { name = "01_10" },
        { name = "11_20" },
        { name = "21_30" },
        { name = "31_40" },
        { name = "41_50" },
        { name = "51_60" },
        { name = "61_70" },
        { name = "71_80" },
        { name = "81_86" }
    }
    local slides = {}
    local seen = {}
    local text_parts = {}

    for _, batch in ipairs(batch_ranges) do
        local key = "schema_" .. batch.name .. "_text"
        local parsed = json_parse(ctx[key])

        for _, slide in ipairs(parsed.slides or {}) do
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

        if slide.title and slide.title ~= "" then
            table.insert(text_parts, slide.title)
        end

        for _, line in ipairs(slide.lines or {}) do
            table.insert(text_parts, line)
        end

        for _, note in ipairs(slide.image_notes or {}) do
            table.insert(text_parts, "[Image] " .. note)
        end

        table.insert(text_parts, "")
    end

    return {
        batch_count = #batch_ranges,
        parsed_slide_count = #slides,
        full_reconstruction_json = json_stringify({ slides = slides }),
        full_reconstruction_text = table.concat(text_parts, "\n")
    }
end):depends_on(previous_step)

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
