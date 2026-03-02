local flow = Flow.new("base64_demo")

flow:step("encode", nodes.base64_encode({
    input = "IronFlow workflow engine — Rust + Lua",
    output_key = "encoded"
}))

flow:step("decode", nodes.base64_decode({
    source_key = "encoded",
    output_key = "decoded"
})):depends_on("encode")

flow:step("log", nodes.log({
    message = "Original → Encoded: ${ctx.encoded} → Decoded: ${ctx.decoded}",
    level = "info"
})):depends_on("decode")

return flow
