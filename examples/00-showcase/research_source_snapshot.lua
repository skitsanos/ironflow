-- Source snapshot for a citation: fetch a page, keep a Markdown copy, and
-- record a SHA-256 fingerprint of that copy in a manifest beside it. Anyone
-- can later re-hash the saved file and compare it with the manifest to show
-- what the page said on the day it was cited.
-- Effects: one GET to source_url; creates <output_dir>/<document_id>.md and
-- <output_dir>/<document_id>.json and keeps them. A second run with the same
-- document_id replaces both files.
local flow = Flow.new("research_source_snapshot")

-- 1. Fetch the source.
flow:step("fetch", nodes.http_get({
    url = "${ctx.source_url}",
    headers = {
        ["Accept"] = "text/html"
    },
    output_key = "page",
    timeout = 20,
    retry_statuses = { 429, 500, 502, 503 },
    status_retries = 2,
    status_retry_backoff = 1
}))

-- 2. Convert the HTML into a portable text representation.
flow:step("normalize", nodes.html_to_markdown({
    source_key = "page_data",
    output_key = "snapshot_markdown"
})):depends_on("fetch")

-- 3. Fingerprint the normalized snapshot.
flow:step("fingerprint", nodes.hash({
    source_key = "snapshot_markdown",
    algorithm = "sha256",
    output_key = "snapshot_sha256"
})):depends_on("normalize")

-- 4. Build provenance metadata. A missing context key renders as an empty
--    string in a path template, so the two keys that name the files are
--    checked here, before anything is written.
flow:step("manifest", nodes.code({
    source = function(ctx)
        for _, key in ipairs({ "document_id", "output_dir" }) do
            if type(ctx[key]) ~= "string" or ctx[key] == "" then
                error(key .. " is required: pass it through --context")
            end
        end
        return {
            manifest_json = json_stringify({
                document_id = ctx.document_id,
                source_url = ctx.source_url,
                captured_at = now_rfc3339(),
                sha256 = ctx.snapshot_sha256,
                http_status = ctx.page_status
            })
        }
    end
})):depends_on("fingerprint")

-- 5. Save the normalized source, then its manifest. One after the other: two
--    write_file steps in the same phase would both write write_file_path and
--    collide.
flow:step("save_snapshot", nodes.write_file({
    path = "${ctx.output_dir}/${ctx.document_id}.md",
    source_key = "snapshot_markdown"
})):depends_on("manifest")

flow:step("save_manifest", nodes.write_file({
    path = "${ctx.output_dir}/${ctx.document_id}.json",
    source_key = "manifest_json"
})):depends_on("save_snapshot")

flow:step("done", nodes.log({
    message =
        "Captured ${ctx.source_url}; SHA-256=${ctx.snapshot_sha256}"
})):depends_on("save_manifest")

return flow

-- source_url, document_id and output_dir are required; without them the run
-- fails before it writes a file. Run from the repository root:
--   ironflow run examples/00-showcase/research_source_snapshot.lua \
--     --context '{"source_url":"https://example.com/","document_id":"source-017","output_dir":"data/output/research-snapshots"}'
-- Then check the copy against its manifest:
--   shasum -a 256 data/output/research-snapshots/source-017.md
