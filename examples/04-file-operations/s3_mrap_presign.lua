--[[
Generate GET/PUT/HEAD/DELETE presigned URLs through an S3 Multi-Region Access
Point (MRAP). This flow sends no S3 requests and creates no remote objects.

Requirements:
- S3_BUCKET containing an MRAP ARN, for example
  arn:aws:s3::123456789012:accesspoint/fixture.mrap
- AWS credentials from the environment or the SDK credential chain.
- With static credentials, signing runs without network access. Other
  credential providers may contact their identity service.

The returned URLs expire after five minutes. Treat them as bearer credentials;
the flow does not log them, but the CLI returns them in its context output.
]]

local flow = Flow.new("s3_mrap_presign")

for _, method in ipairs({ "GET", "PUT", "HEAD", "DELETE" }) do
    local name = "mrap_" .. string.lower(method)
    flow:step(name, nodes.s3_presign_url({
        bucket = env("S3_BUCKET"),
        key = "example.txt",
        method = method,
        region = "us-east-1",
        expires_in = 300,
        content_type = "text/plain",
        output_key = name
    }))
end

return flow
