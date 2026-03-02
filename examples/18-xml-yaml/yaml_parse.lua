local flow = Flow.new("yaml_parse_demo")

flow:step("parse", nodes.yaml_parse({
    input = "server:\n  host: localhost\n  port: 8080\n  features:\n    - auth\n    - logging",
    output_key = "config"
}))

flow:step("log_result", nodes.log({
    message = "Parsed YAML: ${ctx.config}",
    level = "info"
})):depends_on("parse")

return flow
