-- Runnable scheduler flow for examples/21-schedules/ironflow.yaml.
-- Start it from the repository root with:
--   cargo run -- -C examples/21-schedules/ironflow.yaml serve
-- The JSON store keeps claim ownership atomic while retention runs in bounded,
-- schedule-specific batches; no cleanup setting is required in this flow.

local flow = Flow.new("scheduled_hello")

flow:step("announce", nodes.log({
    message = "Scheduled hello for ${ctx.audience}"
}))

return flow
