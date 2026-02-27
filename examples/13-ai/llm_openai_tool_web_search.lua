--[[
OpenAI Responses API with internal web search tool.

Flow:
1) Call `nodes.llm` in `responses` mode with `tools = { { type = "web_search_preview" } }`.
2) Parse response output text from the raw response.
3) Log the concise extracted result.

Environment variables:
- OPENAI_API_KEY
- OPENAI_BASE_URL (optional, defaults to https://api.openai.com/v1)
]]

local flow = Flow.new("llm_openai_tool_web_search")

flow:step("search", nodes.llm({
    provider = "openai",
    mode = "responses",
    model = "gpt-4o-mini",
    prompt = "Use web search to find the official Rust language website and reply with one concise sentence including the site URL.",
    output_key = "openai_tool_search",
    extra = {
        tools = {
            { type = "web_search_preview" }
        },
        tool_choice = "auto"
    }
}))

flow:step("extract", nodes.code({
    source = function()
        local raw = ctx.openai_tool_search_raw
        local text_parts = {}

        local function add_if_text(value)
            if type(value) == "string" and value ~= "" then
                table.insert(text_parts, value)
            end
        end

        if raw ~= nil then
            if type(raw.output_text) == "string" and raw.output_text ~= "" then
                add_if_text(raw.output_text)
            end

            if type(raw.output) == "table" then
                for _, item in ipairs(raw.output) do
                    if type(item.content) == "table" then
                        for _, part in ipairs(item.content) do
                            if type(part) == "table" then
                                add_if_text(part.text)
                                add_if_text(part.summary)
                            end
                        end
                    end

                    if type(item.text) == "string" then
                        add_if_text(item.text)
                    end
                end
            end
        end

        local search_summary = table.concat(text_parts, "\n")
        if search_summary == "" then
            search_summary = "No summarized output returned in this response."
        end

        return {
            openai_tool_search_summary = search_summary,
        }
    end
})):depends_on("search")

flow:step("print", nodes.log({
    message = "Search summary: ${ctx.openai_tool_search_summary}",
})):depends_on("extract")

return flow
