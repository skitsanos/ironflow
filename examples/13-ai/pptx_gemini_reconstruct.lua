--[[
PPTX + Gemini multimodal reconstruction demo.

Flow:
1. Render a Quick Look preview image for the deck (macOS qlmanage).
2. Extract the PPTX as structured JSON.
3. Keep the first 3 slides for a compact demo payload.
4. Send the structured slide data plus preview image to Gemini.
5. Write Gemini's text-only reconstruction to /tmp.

Environment variables:
- GEMINI_API_KEY

Notes:
- qlmanage produces a deck preview thumbnail, not individual slide images.
- extract_pptx exposes per-slide image elements as metadata/path references.
]]

local flow = Flow.new("pptx_gemini_reconstruct")

flow:step("check_key", function()
    if not env("GEMINI_API_KEY") or env("GEMINI_API_KEY") == "" then
        error("GEMINI_API_KEY is required")
    end
    return { gemini_key_available = true }
end)

flow:step("render_preview", nodes.shell_command({
    cmd = "sh",
    args = {
        "-c",
        "rm -rf /tmp/ironflow-pptx-gemini && mkdir -p /tmp/ironflow-pptx-gemini && qlmanage -t -s 1400 -o /tmp/ironflow-pptx-gemini data/samples/sample.pptx >/tmp/ironflow-pptx-gemini/qlmanage.log 2>&1"
    },
    timeout = 30,
    output_key = "preview_render"
})):depends_on("check_key")

flow:step("read_preview", nodes.read_file({
    path = "/tmp/ironflow-pptx-gemini/sample.pptx.png",
    encoding = "base64",
    output_key = "slide_preview"
})):depends_on("render_preview")

flow:step("extract_deck", nodes.extract_pptx({
    path = "data/samples/sample.pptx",
    format = "json",
    output_key = "deck",
    metadata_key = "deck_meta",
    comments_key = "deck_comments"
})):depends_on("check_key")

flow:step("prepare_first_three", function(ctx)
    local selected = {}
    local image_count = 0
    local slides = ctx.deck and ctx.deck.slides or {}

    for i = 1, math.min(3, #slides) do
        local slide = slides[i]
        table.insert(selected, slide)

        for _, element in ipairs(slide.elements or {}) do
            if element.type == "image" then
                image_count = image_count + 1
            end
        end
    end

    local payload = {
        deck_metadata = ctx.deck_meta,
        selected_slide_count = #selected,
        image_element_count = image_count,
        slides = selected
    }

    return {
        selected_slide_count = #selected,
        selected_image_count = image_count,
        reconstruction_payload = json_stringify(payload)
    }
end):depends_on("extract_deck")

flow:step("reconstruct", nodes.llm({
    provider = "custom",
    mode = "chat",
    model = "gemini-3.5-flash",
    base_url = "https://generativelanguage.googleapis.com/v1beta/openai",
    auth_type = "bearer",
    api_key = env("GEMINI_API_KEY"),
    max_tokens = 16000,
    temperature = 0.0,
    output_key = "gemini_reconstruction",
    messages = {
        {
            role = "user",
            content = {
                {
                    type = "text",
                    text = [[
You are reconstructing a PowerPoint deck into plain text.

Use both inputs:
1. The structured PPTX extraction JSON.
2. The rendered deck preview image.

Task:
- Reconstruct the first 3 slides as text-only content.
- Preserve all meaningful content from the extraction.
- Do not summarize.
- Do not invent missing content.
- Write as if a human read each slide left-to-right, top-to-bottom, line by line on paper.
- Keep slide boundaries.
- Include tables as readable plain-text rows.
- Include image descriptions only when the image or extracted alt text carries meaningful information.
- If the preview image conflicts with the extracted JSON, prefer the extracted JSON for text and use the image for layout/read-order hints.

Return exactly this structure:

Slide 1
<lines>

Slide 2
<lines>

Slide 3
<lines>

Structured PPTX extraction JSON:
${ctx.reconstruction_payload}
]]
                },
                {
                    type = "image_url",
                    image_url = {
                        url = "data:image/png;base64,${ctx.slide_preview_content}"
                    }
                }
            }
        }
    }
})):depends_on("prepare_first_three", "read_preview")

flow:step("write_reconstruction", nodes.write_file({
    path = "/tmp/ironflow-pptx-gemini/reconstruction.txt",
    content = "${ctx.gemini_reconstruction_text}"
})):depends_on("reconstruct")

flow:step("log_result", nodes.log({
    message = "Reconstructed ${ctx.selected_slide_count} slides with ${ctx.selected_image_count} image elements. Output: /tmp/ironflow-pptx-gemini/reconstruction.txt"
})):depends_on("write_reconstruction")

return flow
