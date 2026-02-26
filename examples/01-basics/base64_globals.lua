--
-- Base64 encode/decode globals
--
-- Demonstrates the `base64_encode()` and `base64_decode()` Lua globals
-- available in all code and foreach nodes.
--

local flow = Flow.new("base64_globals")

flow:step("encode_decode", function(ctx)
    local original = "Hello, IronFlow!"

    local encoded = base64_encode(original)
    local decoded = base64_decode(encoded)

    return {
        original = original,
        encoded = encoded,
        decoded = decoded,
        roundtrip_ok = (original == decoded)
    }
end)

flow:step("show", nodes.log({
    message = "Encoded: ${ctx.encoded} | Decoded: ${ctx.decoded} | Match: ${ctx.roundtrip_ok}"
})):depends_on("encode_decode")

return flow
