local flow = Flow.new("pdf_gemini_rag_schema")

local OUTPUT_DIR = "/tmp/ironflow-pdf-gemini-book"

local function response_format_schema()
    return {
        type = "json_schema",
        json_schema = {
            name = "generic_pdf_rag_document",
            strict = true,
            schema = {
                type = "object",
                additionalProperties = false,
                properties = {
                    document = {
                        type = "object",
                        additionalProperties = false,
                        properties = {
                            title = { type = "string" },
                            document_type = { type = "string" },
                            language = { type = "string" },
                            summary = { type = "string" }
                        },
                        required = { "title", "document_type", "language", "summary" }
                    },
                    pages = {
                        type = "array",
                        items = {
                            type = "object",
                            additionalProperties = false,
                            properties = {
                                page_number = { type = "integer" },
                                blocks = {
                                    type = "array",
                                    items = {
                                        type = "object",
                                        additionalProperties = false,
                                        properties = {
                                            block_id = { type = "string" },
                                            type = { type = "string" },
                                            heading = { type = "string" },
                                            text = { type = "string" },
                                            level = { type = "integer" },
                                            items = {
                                                type = "array",
                                                items = { type = "string" }
                                            },
                                            table = {
                                                type = "object",
                                                additionalProperties = false,
                                                properties = {
                                                    columns = {
                                                        type = "array",
                                                        items = { type = "string" }
                                                    },
                                                    rows = {
                                                        type = "array",
                                                        items = {
                                                            type = "array",
                                                            items = { type = "string" }
                                                        }
                                                    }
                                                },
                                                required = { "columns", "rows" }
                                            },
                                            metadata = {
                                                type = "object",
                                                additionalProperties = false,
                                                properties = {
                                                    role = { type = "string" },
                                                    importance = { type = "string" }
                                                },
                                                required = { "role", "importance" }
                                            }
                                        },
                                        required = { "block_id", "type", "heading", "text", "level", "items", "table", "metadata" }
                                    }
                                }
                            },
                            required = { "page_number", "blocks" }
                        }
                    },
                    rag_chunks = {
                        type = "array",
                        items = {
                            type = "object",
                            additionalProperties = false,
                            properties = {
                                chunk_id = { type = "string" },
                                page_numbers = {
                                    type = "array",
                                    items = { type = "integer" }
                                },
                                heading_path = {
                                    type = "array",
                                    items = { type = "string" }
                                },
                                text = { type = "string" },
                                block_ids = {
                                    type = "array",
                                    items = { type = "string" }
                                },
                                metadata = {
                                    type = "object",
                                    additionalProperties = false,
                                    properties = {
                                        document_type = { type = "string" },
                                        content_types = {
                                            type = "array",
                                            items = { type = "string" }
                                        }
                                    },
                                    required = { "document_type", "content_types" }
                                }
                            },
                            required = { "chunk_id", "page_numbers", "heading_path", "text", "block_ids", "metadata" }
                        }
                    }
                },
                required = { "document", "pages", "rag_chunks" }
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
    cmd = "sh",
    args = { "-c", "rm -rf /tmp/ironflow-pdf-gemini-book && mkdir -p /tmp/ironflow-pdf-gemini-book" },
    timeout = 10,
    output_key = "prepare_output_dir"
})):depends_on("check_key")

flow:step("extract_pdf", nodes.extract_pdf({
    path = "data/samples/generated_book.pdf",
    format = "markdown",
    output_key = "pdf_markdown",
    metadata_key = "pdf_meta"
})):depends_on("prepare_output_dir")

flow:step("render_pages", nodes.shell_command({
    cmd = "pdftoppm",
    args = {
        "-png",
        "-r", "120",
        "-f", "1",
        "-l", "2",
        "data/samples/generated_book.pdf",
        OUTPUT_DIR .. "/page"
    },
    timeout = 30,
    output_key = "render_pages"
})):depends_on("prepare_output_dir")

flow:step("read_page_1", nodes.read_file({
    path = OUTPUT_DIR .. "/page-1.png",
    encoding = "base64",
    output_key = "page_1"
})):depends_on("render_pages")

flow:step("read_page_2", nodes.read_file({
    path = OUTPUT_DIR .. "/page-2.png",
    encoding = "base64",
    output_key = "page_2"
})):depends_on("render_pages")

flow:step("prepare_payload", function(ctx)
    local payload = {
        metadata = ctx.pdf_meta,
        extracted_markdown = ctx.pdf_markdown,
        rendered_pages = {
            { page = 1, path = ctx.page_1_path, format = "png" },
            { page = 2, path = ctx.page_2_path, format = "png" }
        }
    }

    return {
        reconstruction_payload = json_stringify(payload)
    }
end):depends_on("extract_pdf", "read_page_1", "read_page_2")

flow:step("analyze", nodes.llm({
    provider = "custom",
    mode = "chat",
    model = "gemini-3.5-flash",
    base_url = "https://generativelanguage.googleapis.com/v1beta/openai",
    auth_type = "bearer",
    api_key = env("GEMINI_API_KEY"),
    max_tokens = 16000,
    temperature = 0.0,
    timeout = 180,
    output_key = "pdf_rag",
    messages = {
        {
            role = "user",
            content = {
                {
                    type = "text",
                    text = [[
Analyze this unknown PDF for LLM/RAG ingestion.

Use both inputs:
1. The PDF extraction payload. It may be empty or unreliable.
2. Rendered images for pages 1 and 2.

Task:
- Identify the document type and language.
- Segment each page into generic content blocks.
- Preserve meaningful visible text.
- Ignore decorative-only visuals unless they carry content.
- Create retrieval-ready chunks that are semantically coherent.
- Chunks should be useful as standalone RAG context.
- Prefer the rendered images when extraction text is missing or conflicts.
- Return only JSON matching the required schema.

Rules for generic blocks:
- Use type values such as cover, heading, paragraph, list, table, figure, header, footer, caption, key_value, unknown.
- Use metadata.role values such as main_content, title, navigation, caption, footer, noise.
- Use metadata.importance values high, medium, or low.
- For non-table blocks, set table.columns and table.rows to empty arrays.
- For non-list blocks, set items to an empty array.
- Keep block IDs stable and page-scoped, e.g. p1_b1.

PDF extraction payload:
${ctx.reconstruction_payload}
]]
                },
                {
                    type = "image_url",
                    image_url = {
                        url = "data:image/png;base64,${ctx.page_1_content}"
                    }
                },
                {
                    type = "image_url",
                    image_url = {
                        url = "data:image/png;base64,${ctx.page_2_content}"
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
    local parsed = json_parse(ctx.pdf_rag_text)
    local parts = {
        "Title: " .. parsed.document.title,
        "Type: " .. parsed.document.document_type,
        "Language: " .. parsed.document.language,
        "",
        parsed.document.summary,
        ""
    }

    for _, chunk in ipairs(parsed.rag_chunks or {}) do
        table.insert(parts, "Chunk " .. chunk.chunk_id)
        table.insert(parts, table.concat(chunk.heading_path or {}, " > "))
        table.insert(parts, chunk.text)
        table.insert(parts, "")
    end

    return {
        pdf_rag_json = ctx.pdf_rag_text,
        pdf_rag_text = table.concat(parts, "\n")
    }
end):depends_on("analyze")

flow:step("write_json", nodes.write_file({
    path = OUTPUT_DIR .. "/rag.json",
    content = "${ctx.pdf_rag_json}"
})):depends_on("format_text")

flow:step("write_text", nodes.write_file({
    path = OUTPUT_DIR .. "/rag.txt",
    content = "${ctx.pdf_rag_text}"
})):depends_on("write_json")

flow:step("log_result", nodes.log({
    message = "PDF RAG output: " .. OUTPUT_DIR .. "/rag.json and " .. OUTPUT_DIR .. "/rag.txt"
})):depends_on("write_text")

return flow
