use super::*;

#[test]
fn records_only_undefined_reads_inside_registered_handlers_once() {
    let source = r#"
local flow = Flow.new("lint")
local outside = missing_outside
flow:step("render", function(ctx)
    local value = string.format("%s", missing_inside)
    return { value = value, id = uuid4(), context = ctx }
end)
return flow
"#;
    let analysis = HandlerDiagnostics::analyze(source).unwrap();
    analysis.record_handler(4, 7).unwrap();
    analysis.record_handler(4, 7).unwrap();
    analysis.clone().record_handler(4, 7).unwrap();
    let warnings = analysis.warnings().unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].message,
        "`missing_inside` is not defined in the Lua handler environment"
    );
    assert_eq!((warnings[0].line, warnings[0].column), (5, 39));
    assert_eq!(warnings[0].step, None);
}

#[test]
fn handler_runtime_does_not_allow_flow_loader_globals() {
    let source = r#"
local flow = Flow.new("lint")
flow:step("render", function()
    return { leaked = Flow, factory = nodes }
end)
return flow
"#;
    let analysis = HandlerDiagnostics::analyze(source).unwrap();
    analysis.record_handler(3, 5).unwrap();
    let warnings = analysis.warnings().unwrap();
    assert_eq!(warnings.len(), 2);
    assert!(warnings.iter().any(|item| item.message.contains("`Flow`")));
    assert!(warnings.iter().any(|item| item.message.contains("`nodes`")));
}

#[test]
fn understands_handler_lexical_scopes_and_non_variable_identifiers() {
    let source = r#"local flow = Flow.new("scopes")
flow:step("render", function(ctx)
    local left, total = ctx.left, 0
    for index, item in ipairs(ctx.items) do
        local upper = string.upper(item.name)
        total = total + index + #upper
    end
    repeat
        local complete = total > 0
        total = total + 1
    until complete
    local nested = function(value) return value + total end
    return { [ctx.key] = left, total = nested(total) }
end)
return flow
"#;
    let analysis = HandlerDiagnostics::analyze(source).unwrap();
    analysis.record_handler(2, 14).unwrap();
    assert_eq!(analysis.warnings().unwrap(), Vec::new());
}

#[test]
fn local_names_are_not_visible_in_their_own_initializers() {
    let source = r#"local flow = Flow.new("initializer")
flow:step("render", function()
    local missing = missing
    return { missing = missing }
end)
return flow
"#;
    let analysis = HandlerDiagnostics::analyze(source).unwrap();
    analysis.record_handler(2, 5).unwrap();
    let warnings = analysis.warnings().unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 3);
}

#[test]
fn invalid_syntax_is_left_to_the_lua_parser() {
    let analysis = HandlerDiagnostics::analyze("return function( ???").unwrap();
    assert_eq!(analysis.warnings().unwrap(), Vec::new());
}

#[test]
fn unmatched_ranges_still_fail_without_recording_warnings() {
    let analysis = HandlerDiagnostics::analyze("local f = function() return missing end").unwrap();
    let error = analysis.record_handler(2, 3).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Could not map serialized Lua handler at lines 2-3")
    );
    assert!(analysis.warnings().unwrap().is_empty());
}

#[test]
fn ambiguous_nested_ranges_fail_without_recording_warnings() {
    let analysis = HandlerDiagnostics::analyze(
        "local f = function() return function() return missing end end",
    )
    .unwrap();
    for _ in 0..2 {
        let error = analysis.record_handler(1, 1).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Ambiguous serialized Lua handler")
        );
        assert!(analysis.warnings().unwrap().is_empty());
    }
}

#[test]
fn code_source_diagnostics_are_step_relative() {
    let warnings = analyze_code_source(
        "local header = 'hello'\nreturn string.format('%s %s', header, footer)",
        "render",
    )
    .unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].step.as_deref(), Some("render"));
    assert_eq!((warnings[0].line, warnings[0].column), (2, 39));
    assert!(warnings[0].message.contains("code source environment"));
}

#[test]
fn expression_source_diagnostics_map_to_original_columns() {
    let warnings = analyze_code_source("missing_value", "expression").unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!((warnings[0].line, warnings[0].column), (1, 1));
    assert_eq!(warnings[0].step.as_deref(), Some("expression"));
}

#[test]
fn deferred_steps_keep_reads_declarations_and_scopes_in_source_order() {
    // Each line depends on the walk order: an initializer is read before its
    // name exists, a global is known only after its assignment, and a loop or
    // `repeat` scope ends where the source ends it.
    let source = r#"local flow = Flow.new("order")
flow:step("render", function(ctx)
    local function fact(n)
        return n <= 1 and 1 or n * fact(n - 1)
    end
    function helper(x) return x + late end
    late = 2
    total = (total or early) + helper(fact(ctx.n))
    for index = first, #ctx.items do total = total + index end
    repeat local done = total > limit until done
    return { total = total, late = late, done = done }
end)
return flow
"#;
    let analysis = HandlerDiagnostics::analyze(source).unwrap();
    analysis.record_handler(2, 12).unwrap();
    let reported = analysis
        .warnings()
        .unwrap()
        .into_iter()
        .map(|warning| (warning.message, warning.line, warning.column))
        .collect::<Vec<_>>();
    let expected = [
        ("late", 6, 35),
        ("total", 8, 14),
        ("early", 8, 23),
        ("first", 9, 17),
        ("limit", 10, 33),
        ("done", 11, 49),
    ]
    .map(|(name, line, column)| {
        (
            format!("`{name}` is not defined in the Lua handler environment"),
            line,
            column,
        )
    });
    assert_eq!(reported, expected);
}

/// `return 1+1...` nests one level per operator below the chunk, the return
/// statement and its expression list.
fn operator_chain(operators: usize) -> String {
    format!("return 1{}", "+1".repeat(operators))
}

#[test]
fn syntax_nesting_is_accepted_at_the_cap_and_rejected_one_level_above() {
    let at_cap = operator_chain(tree::MAX_SYNTAX_DEPTH - 3);
    assert!(HandlerDiagnostics::analyze(&at_cap).is_ok());
    assert_eq!(analyze_code_source(&at_cap, "calc").unwrap(), Vec::new());

    let above = operator_chain(tree::MAX_SYNTAX_DEPTH - 2);
    let error = HandlerDiagnostics::analyze(&above).err().unwrap();
    assert_eq!(
        error.to_string(),
        "Lua source is nested too deeply for validation: line 1 exceeds 1000 levels of syntax nesting"
    );
    let error = analyze_code_source(&above, "calc").unwrap_err();
    assert_eq!(
        error.to_string(),
        "Step 'calc' code source: Lua source is nested too deeply for validation: line 1 exceeds 1000 levels of syntax nesting"
    );
}

#[test]
fn a_deep_tree_is_walked_without_native_recursion() {
    // The chain stays under the cap, yet is deeper than a 128 KiB stack could
    // recurse through. The undefined read sits at the bottom of the chain.
    let source = format!(
        "local f = function()\n    return bottom{}\nend",
        "+1".repeat(tree::MAX_SYNTAX_DEPTH - 20)
    );
    let warnings = std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(move || {
            let analysis = HandlerDiagnostics::analyze(&source).unwrap();
            analysis.record_handler(1, 3).unwrap();
            analysis.warnings().unwrap()
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("`bottom`"));
    assert_eq!((warnings[0].line, warnings[0].column), (2, 12));
}
