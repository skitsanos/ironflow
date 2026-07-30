local flow = Flow.new("xlsx_workbook_demo")

-- ironflow-sample.xlsx has two sheets: "Parts" (whose real header sits in
-- row 2, with a title row above it) and "Notes". Reading with
-- has_header = false keeps every row as a plain array, so the title row is
-- just data instead of getting flattened into the object keys -- a common
-- real-world workbook layout where the header is not row one.
flow:step("extract", nodes.extract_xlsx({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.xlsx",
    has_header = false,
    output_key = "workbook"
}))

flow:step("log_sheets", nodes.log({
    message = "Sheets in workbook order: ${ctx.workbook_sheet_names}"
})):depends_on("extract")

-- Object key order does not survive the round trip into Lua, so counting
-- rows per sheet has to iterate workbook_sheet_names rather than the
-- workbook object's own keys -- this is why extract_xlsx emits that array.
flow:step("count_rows", nodes.foreach({
    source_key = "workbook_sheet_names",
    output_key = "sheet_row_counts",
    transform = function(name)
        return {
            sheet = name,
            rows = #ctx.workbook[name]
        }
    end
})):depends_on("log_sheets")

flow:step("log_counts", nodes.log({
    message = "Row counts per sheet: ${ctx.sheet_row_counts}"
})):depends_on("count_rows")

return flow
