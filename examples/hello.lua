-- A simple hello world flow
local flow = Flow.new("hello_world")

flow:step("greet", nodes.log({
    message = "Hello from IronFlow! User: ${ctx.user_name}",
    level = "info"
}))

flow:step("render", nodes.template_render({
    template = "Welcome, ${ctx.user_name}! Today is a great day.",
    output_key = "greeting"
})):depends_on("greet")

flow:step("done", nodes.log({
    message = "Rendered greeting: ${ctx.greeting}",
    level = "info"
})):depends_on("render")

return flow
