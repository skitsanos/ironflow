local flow = Flow.new("yaml_stringify_demo")

flow:step("prepare", nodes.code({
    source = function()
        return {
            config = {
                server = {
                    host = "localhost",
                    port = 8080,
                    features = { "auth", "logging" },
                },
            },
        }
    end,
}))

flow:step("to_yaml", nodes.yaml_stringify({
    source_key = "config",
    output_key = "yaml_config",
})):depends_on("prepare")

flow:step("log_result", nodes.log({
    message = "YAML:\n${ctx.yaml_config}",
    level = "info",
})):depends_on("to_yaml")

return flow
