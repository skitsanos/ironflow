--[[
PDF + Gemini JSON-schema reconstruction demo.

Flow:
1. Extract text and metadata from a one-page PDF.
2. Render the PDF page to PNG with Poppler (`pdftoppm`) through `shell_command`.
3. Send both the extracted text and rendered page image to Gemini.
4. Ask Gemini to return structured JSON matching a schema.
5. Write JSON, text, and the rendered page PNG to /tmp.

Environment variables:
- GEMINI_API_KEY

Dependencies:
- `pdftoppm` from Poppler must be available on PATH.

Run:
  cargo run -- --dotenv .env run examples/13-ai/pdf_gemini_reconstruct_schema.lua

Outputs:
- /tmp/ironflow-pdf-gemini/reconstruction.json
- /tmp/ironflow-pdf-gemini/reconstruction.txt
- /tmp/ironflow-pdf-gemini/page-1.png
]]

local flow = Flow.new("pdf_gemini_reconstruct_schema")

local OUTPUT_DIR = "/tmp/ironflow-pdf-gemini"

local function response_format_schema()
    return {
        type = "json_schema",
        json_schema = {
            name = "pdf_page_reconstruction",
            strict = true,
            schema = {
                type = "object",
                additionalProperties = false,
                properties = {
                    document_title = { type = "string" },
                    pages = {
                        type = "array",
                        items = {
                            type = "object",
                            additionalProperties = false,
                            properties = {
                                page_number = { type = "integer" },
                                lines = {
                                    type = "array",
                                    items = { type = "string" }
                                },
                                tables = {
                                    type = "array",
                                    items = {
                                        type = "object",
                                        additionalProperties = false,
                                        properties = {
                                            title = { type = "string" },
                                            rows = {
                                                type = "array",
                                                items = { type = "string" }
                                            }
                                        },
                                        required = { "title", "rows" }
                                    }
                                },
                                visual_notes = {
                                    type = "array",
                                    items = { type = "string" }
                                }
                            },
                            required = { "page_number", "lines", "tables", "visual_notes" }
                        }
                    }
                },
                required = { "document_title", "pages" }
            }
        }
    }
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

flow:step("extract_pdf", nodes.extract_pdf({
    path = "data/samples/sample.pdf",
    format = "markdown",
    output_key = "pdf_markdown",
    metadata_key = "pdf_meta"
})):depends_on("prepare_output_dir")

flow:step("render_page", nodes.shell_command({
    cmd = "pdftoppm",
    args = {
        "-png",
        "-r", "200",
        "-f", "1",
        "-l", "1",
        "-singlefile",
        "data/samples/sample.pdf",
        OUTPUT_DIR .. "/page-1"
    },
    timeout = 30,
    output_key = "render_page"
})):depends_on("prepare_output_dir")

flow:step("read_rendered_page", nodes.read_file({
    path = OUTPUT_DIR .. "/page-1.png",
    encoding = "base64",
    output_key = "rendered_page_file"
})):depends_on("render_page")

flow:step("prepare_payload", function(ctx)
    local payload = {
        metadata = ctx.pdf_meta,
        extracted_markdown = ctx.pdf_markdown,
        rendered_page = {
            page = 1,
            format = "png",
            path = ctx.rendered_page_file_path
        }
    }

    return {
        rendered_page_image_base64 = ctx.rendered_page_file_content or "",
        reconstruction_payload = json_stringify(payload)
    }
end):depends_on("extract_pdf", "read_rendered_page")

flow:step("reconstruct", nodes.llm({
    provider = "custom",
    mode = "chat",
    model = "gemini-3.5-flash",
    base_url = "https://generativelanguage.googleapis.com/v1beta/openai",
    auth_type = "bearer",
    api_key = env("GEMINI_API_KEY"),
    max_tokens = 12000,
    temperature = 0.0,
    timeout = 180,
    output_key = "gemini_reconstruction",
    messages = {
        {
            role = "user",
            content = {
                {
                    type = "text",
                    text = [[
Reconstruct this one-page PDF into coherent text-only content.

Use both inputs:
1. The extracted PDF Markdown and metadata.
2. The rendered page image.

Rules:
- Preserve all meaningful content.
- Do not summarize.
- Do not invent missing content.
- Use the rendered image to recover layout, labels, table structure, and reading order.
- Use the extracted text as supporting evidence, but prefer the image when the extraction order is confusing.
- Write page content as if a human read the page top-to-bottom and left-to-right.
- Represent table-like areas as row strings in reading order.
- Put non-text layout/image observations in visual_notes only when they help explain the content.
- Return only JSON matching the required schema.

PDF extraction payload:
${ctx.reconstruction_payload}
]]
                },
                {
                    type = "image_url",
                    image_url = {
                        url = "data:image/png;base64,${ctx.rendered_page_image_base64}"
                    }
                }
            }
        }
    },
    extra = {
        response_format = response_format_schema()
    }
})):depends_on("prepare_payload")

flow:step("format_text", function(ctx)
    local parsed = json_parse(ctx.gemini_reconstruction_text)
    local parts = {}

    if parsed.document_title and parsed.document_title ~= "" then
        table.insert(parts, parsed.document_title)
        table.insert(parts, "")
    end

    for _, page in ipairs(parsed.pages or {}) do
        table.insert(parts, "Page " .. page.page_number)

        for _, line in ipairs(page.lines or {}) do
            table.insert(parts, line)
        end

        for _, table_data in ipairs(page.tables or {}) do
            table.insert(parts, "")
            if table_data.title and table_data.title ~= "" then
                table.insert(parts, table_data.title)
            end
            for _, row in ipairs(table_data.rows or {}) do
                table.insert(parts, row)
            end
        end

        if type(page.visual_notes) == "table" and #page.visual_notes > 0 then
            table.insert(parts, "")
            table.insert(parts, "Visual notes")
            for _, note in ipairs(page.visual_notes or {}) do
                table.insert(parts, "- " .. note)
            end
        end

        table.insert(parts, "")
    end

    return {
        pdf_reconstruction_json = ctx.gemini_reconstruction_text,
        pdf_reconstruction_text = table.concat(parts, "\n")
    }
end):depends_on("reconstruct")

flow:step("write_json", nodes.write_file({
    path = OUTPUT_DIR .. "/reconstruction.json",
    content = "${ctx.pdf_reconstruction_json}"
})):depends_on("format_text")

flow:step("write_text", nodes.write_file({
    path = OUTPUT_DIR .. "/reconstruction.txt",
    content = "${ctx.pdf_reconstruction_text}"
})):depends_on("write_json")

flow:step("log_result", nodes.log({
    message = "PDF reconstruction written to " .. OUTPUT_DIR .. "/reconstruction.json and " .. OUTPUT_DIR .. "/reconstruction.txt"
})):depends_on("write_text")

return flow
