-- Function Handlers: Pass Lua functions directly as step handlers
-- No need for nodes.code() — just pass a function(ctx)
-- Handlers are serialized, so keep constants/local state inside the handler
-- or pass values through ctx. `ironflow validate --strict` also catches reads
-- of undefined handler globals before execution.
local flow = Flow.new("function_handler_demo")

-- Step 1: Set up some data using a function handler
flow:step("setup", function(ctx)
    local users = ctx.users
    if users == nil then
        users = {
            { name = "Alice", age = 30, role = "admin" },
            { name = "Bob", age = 25, role = "user" },
            { name = "Charlie", age = 35, role = "admin" }
        }
    end
    return { users = users }
end)

-- Step 2: Filter and transform using another function handler
flow:step("process", function(ctx)
    local admins = {}
    for _, user in ipairs(ctx.users) do
        if user.role == "admin" then
            table.insert(admins, {
                name = string.upper(user.name),
                age = user.age
            })
        end
    end
    return { admins = admins, admin_count = #admins }
end):depends_on("setup")

-- Step 3: Log the result
flow:step("show", nodes.log({
    message = "Found ${ctx.admin_count} admins",
    level = "info"
})):depends_on("process")

return flow

-- Run with:
--   ironflow run examples/07-advanced/function_handler.lua
-- Validate handler globals strictly with:
--   ironflow validate examples/07-advanced/function_handler.lua --strict
