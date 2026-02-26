-- Test all Phase 2 nodes
local flow = Flow.new("phase2_test")

-- 1. Test data_filter
flow:step("setup_data", nodes.log({
    message = "Setting up test data"
}))

-- 2. Test hash
flow:step("hash_test", nodes.hash({
    input = "hello world",
    algorithm = "sha256",
    output_key = "sha256_result"
})):depends_on("setup_data")

flow:step("hash_md5", nodes.hash({
    input = "hello world",
    algorithm = "md5",
    output_key = "md5_result"
})):depends_on("setup_data")

-- 3. Test template_render + log to show hash results
flow:step("show_hashes", nodes.log({
    message = "SHA256: ${ctx.sha256_result}, MD5: ${ctx.md5_result}",
    level = "info"
})):depends_on("hash_test", "hash_md5")

return flow
