local flow = Flow.new("replica_hold")

flow:step("hold", nodes.delay({
    seconds = 300
}))

return flow
