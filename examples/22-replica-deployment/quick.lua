local flow = Flow.new("replica_quick")

flow:step("complete", nodes.log({
    message = "replica acceptance run completed"
}))

return flow
