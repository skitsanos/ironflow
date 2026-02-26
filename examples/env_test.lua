-- Test environment variable access from Lua
local flow = Flow.new("env_test")

flow:step("read_env", nodes.log({
    message = "APP_NAME=" .. (env("APP_NAME") or "NOT SET")
        .. ", API_KEY=" .. (env("API_KEY") or "NOT SET")
        .. ", MISSING=" .. (env("NONEXISTENT_VAR") or "NOT SET"),
    level = "info"
}))

flow:step("use_in_template", nodes.template_render({
    template = "Connecting to ${ctx.db_url}",
    output_key = "connection_info"
})):depends_on("read_env")

return flow
