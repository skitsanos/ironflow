-- foreach with a function handler (instead of a string transform)
local flow = Flow.new("foreach_function_demo")

-- Set up test data
flow:step("setup", nodes.code({
    source = function()
        return {
            products = {
                { name = "Widget", price = 10.50, qty = 3 },
                { name = "Gadget", price = 25.00, qty = 1 },
                { name = "Doohickey", price = 5.75, qty = 10 }
            }
        }
    end
}))

-- Transform each product using a function
flow:step("calc_totals", nodes.foreach({
    source_key = "products",
    output_key = "line_items",
    transform = function(item, index)
        return {
            line = index,
            name = string.upper(item.name),
            total = item.price * item.qty
        }
    end
})):depends_on("setup")

-- Log results
flow:step("done", nodes.log({
    message = "Calculated ${ctx.line_items_count} line items"
})):depends_on("calc_totals")

return flow
