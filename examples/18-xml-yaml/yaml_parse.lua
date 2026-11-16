local flow = Flow.new("yaml_parse_demo")

flow:step("parse", nodes.yaml_parse({
    -- port is an integer. tenant is outside the signed 64-bit range and
    -- burst is not a finite number, so both stay strings and keep their text.
    input = "server:\n  host: localhost\n  port: 8080\n  tenant: 12345678901234567890\n  burst: .inf\n  features:\n    - auth\n    - logging",
    output_key = "config"
}))

flow:step("log_result", nodes.log({
    message = "Parsed YAML: ${ctx.config}",
    level = "info"
})):depends_on("parse")

return flow
