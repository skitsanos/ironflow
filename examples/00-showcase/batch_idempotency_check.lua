-- Idempotent batch ingestion: a small SQLite ledger makes a redelivered batch
-- safe to receive. INSERT OR IGNORE registers the batch under its primary key,
-- and rows_affected says whether this delivery was the first one.
-- The batch is registered before it is processed: a redelivery is detected and
-- skipped, but a batch whose processing fails is not retried by this flow.
-- Effects: creates the SQLite file at db_path if it is missing and keeps it;
-- adds at most one row per batch_id.
local flow = Flow.new("batch_idempotency_check")

local db = "sqlite:${ctx.db_path}?mode=rwc"

-- 1. Validate the incoming envelope.
flow:step("validate", nodes.validate_schema({
    source_key = "batch",
    schema = {
        type = "object",
        required = {
            "batch_id",
            "source"
        },
        properties = {
            batch_id = { type = "string", minLength = 1 },
            source = { type = "string", minLength = 1 }
        },
        additionalProperties = false
    }
}))

-- 2. Check the ledger path and stamp the arrival. A missing context key renders
--    as an empty string in the connection URL, so it is rejected here.
flow:step("prepare", nodes.code({
    source = function(ctx)
        if type(ctx.db_path) ~= "string" or ctx.db_path == "" then
            error("db_path is required: pass it through --context")
        end
        return {
            received_at = now_rfc3339()
        }
    end
})):depends_on("validate")

-- 3. The ledger: one row per batch_id.
flow:step("create_ledger", nodes.db_exec({
    connection = db,
    query = [[
        CREATE TABLE IF NOT EXISTS batch_ledger (
            batch_id TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            received_at TEXT NOT NULL
        )
    ]]
})):depends_on("prepare")

-- 4. Register the batch. A redelivery with the same batch_id affects zero rows.
flow:step("register", nodes.db_exec({
    connection = db,
    query = [[
        INSERT OR IGNORE INTO batch_ledger
            (batch_id, source, received_at)
        VALUES (?, ?, ?)
    ]],
    params = {
        "${ctx.batch.batch_id}",
        "${ctx.batch.source}",
        "${ctx.received_at}"
    }
})):depends_on("create_ledger")

-- 5. Keep the result as a flag before another db_exec can overwrite
--    rows_affected.
flow:step("classify", nodes.code({
    source = function(ctx)
        return {
            is_new_batch = ctx.rows_affected == 1
        }
    end
})):depends_on("register")

-- 6. Read back the ledger entry: the first delivery's row, whoever asks.
flow:step("lookup", nodes.db_query({
    connection = db,
    query = [[
        SELECT batch_id, source, received_at
        FROM batch_ledger
        WHERE batch_id = ?
    ]],
    params = {
        "${ctx.batch.batch_id}"
    },
    output_key = "ledger_entry",
    max_rows = 1
})):depends_on("classify")

-- 7. Gate on the flag. Each branch is one step that depends on the gate and
--    names its route.
flow:step("gate", nodes.if_node({
    condition = "ctx.is_new_batch",
    true_route = "new",
    false_route = "duplicate"
})):depends_on("lookup")

-- 8. First delivery: do the work.
flow:step("process", nodes.log({
    message = "Processing batch ${ctx.batch.batch_id} from ${ctx.batch.source}"
})):depends_on("gate"):route("new")

-- 9. Redelivery: report the first arrival and skip.
flow:step("skip", nodes.log({
    message =
        "Duplicate batch ${ctx.batch.batch_id}: " ..
        "first received ${ctx.ledger_entry[0].received_at}; skipped"
})):depends_on("gate"):route("duplicate")

return flow

-- db_path and batch are required. SQLite creates the ledger file but not its
-- folders, so the directory of db_path must exist. Run it twice with the same
-- batch, from the repository root:
--   mkdir -p data/output
--   ironflow run examples/00-showcase/batch_idempotency_check.lua \
--     --context '{"db_path":"data/output/batch-ledger.db","batch":{"batch_id":"ERP-2026-09-23-0042","source":"inventory-export"}}'
-- The first run processes the batch; the second reports a duplicate and skips.
