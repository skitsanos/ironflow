-- Read the final merged process/dotenv environment from Lua.
-- From the repository root, APP_NAME below overrides .env.example:
-- APP_NAME=from-shell ironflow --dotenv .env.example run \
--   examples/01-basics/environment_variables.lua \
--   --context '{"db_url":"sqlite://example.db"}'
local flow = Flow.new("env_test")

flow:step("read_env", nodes.log({
    message = "APP_NAME=" .. (env("APP_NAME") or "NOT SET")
        .. ", CUSTOM_VAR=" .. (env("CUSTOM_VAR") or "NOT SET")
        .. ", API_KEY=" .. (env("API_KEY") and "SET" or "NOT SET")
        .. ", MISSING=" .. (env("NONEXISTENT_VAR") or "NOT SET"),
    level = "info"
}))

flow:step("use_in_template", nodes.template_render({
    template = "Connecting to ${ctx.db_url}",
    output_key = "connection_info"
})):depends_on("read_env")

return flow
