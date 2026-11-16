//! The bounded library must behave like the reference one (compared against
//! an unmodified Lua state) and must stop where the reference one cannot.

use std::time::{Duration, Instant};

use mlua::prelude::*;

use crate::lua::sandbox::new_sandboxed_lua;
use crate::util::limits::{LuaExecutionLimits, apply_lua_limits, apply_lua_limits_with_control};

/// Renders every result of an expression, or its error, as one string.
const DESCRIBE: &str = r#"
local function render(value)
  if type(value) == "string" then return string.format("%q", value) end
  if type(value) == "table" then
    local parts = {}
    for i = 1, 8 do parts[#parts + 1] = render(rawget(value, i)) end
    return "{" .. table.concat(parts, ",") .. "}"
  end
  if type(value) == "function" then return "function" end
  return tostring(value)
end
function describe(chunk)
  local results = table.pack(pcall(chunk))
  local parts = {}
  for i = 1, results.n do parts[i] = render(results[i]) end
  return table.concat(parts, " | ")
end
"#;

fn limits(max_pattern_steps: Option<u64>) -> LuaExecutionLimits {
    LuaExecutionLimits {
        max_instructions: Some(2_000_000),
        max_seconds: None,
        max_memory_bytes: Some(64 * 1024 * 1024),
        max_pattern_steps,
        hook_interval: 1_000,
        gc_after_execution: false,
    }
}

fn bounded(max_pattern_steps: Option<u64>) -> Lua {
    let lua = new_sandboxed_lua().unwrap();
    apply_lua_limits(&lua, limits(max_pattern_steps)).unwrap();
    lua
}

/// Evaluate `expression` in `lua` and render its results or its error.
fn describe(lua: &Lua, expression: &str) -> String {
    lua.load(DESCRIBE).exec().unwrap();
    let chunk = lua
        .load(format!(
            "local results = table.pack({expression}) return table.unpack(results, 1, results.n)"
        ))
        .set_name("=t")
        .into_function()
        .unwrap();
    lua.globals()
        .get::<LuaFunction>("describe")
        .unwrap()
        .call(chunk)
        .unwrap()
}

fn assert_same(expressions: &[&str]) {
    let (reference, replaced) = (Lua::new(), bounded(None));
    for expression in expressions {
        assert_eq!(
            describe(&replaced, expression),
            describe(&reference, expression),
            "{expression}"
        );
    }
}

#[test]
fn find_and_match_agree_with_the_reference_library() {
    assert_same(&[
        r#"string.find("hello world", "wor")"#,
        r#"string.find("hello world", "o", 6)"#,
        r#"string.find("hello world", "o", -3)"#,
        r#"string.find("hello world", "l+")"#,
        r#"string.find("hello", "")"#,
        r#"string.find("hello", "", 6)"#,
        r#"string.find("hello", "", 7)"#,
        r#"string.find("hello", "xyz")"#,
        r#"string.find("a.b", ".", 1, true)"#,
        r#"string.find("a.b", ".", 2, true)"#,
        r#"string.find("a+b", "+", 1, true)"#,
        r#"string.find("abc", "b", -100)"#,
        r#"string.find("abc", "(b)(c)")"#,
        r#"string.find("abc", "()b()")"#,
        r#"string.find("  x", "^%s*")"#,
        r#"string.find("abc", "^b")"#,
        r#"string.find("abc", "c$")"#,
        r#"string.find("a$c", "a$c")"#,
        r#"string.find(12345, 34)"#,
        r#"("key = value"):match("^(%w+)%s*=%s*(%w+)$")"#,
        r#"string.match("hello", ".-")"#,
        r#"string.match("hello", ".-l")"#,
        r#"string.match("hello", "h?e")"#,
        r#"string.match("ello", "h?e")"#,
        r#"string.match("2026-11-15", "(%d+)-(%d+)-(%d+)")"#,
        r#"string.match("THE (quick) fox", "%((%a+)%)")"#,
        r#"string.match("THE (quick) fox", "%b()")"#,
        r#"string.match("f(a(b)c) d", "%b()")"#,
        r#"string.match("THE quick fox", "%f[%a]%a+", 5)"#,
        r#"string.match("THE (quick) fox", "%f[%l]%l+")"#,
        r#"string.match("hello hello", "(h%w+) %1")"#,
        r#"string.match("x=1, y=2", "(%w)=(%d), (%w)=(%d)")"#,
        r#"string.match("abc", "[a-c]+")"#,
        r#"string.match("abc]", "[]]")"#,
        r#"string.match("a-c", "[a%-]+")"#,
        r#"string.match("abc", "[^a]+")"#,
        r#"string.match("a^b", "[b^]+")"#,
        r#"string.match("caf\xc3\xa9", "[\x80-\xff]+")"#,
        r#"string.match("a\0b", "%z")"#,
        r#"string.match("a\0b", "a\0b")"#,
        r#"string.match("  \t\v\f\r\nx", "^%s+()")"#,
        r#"string.match("aB3_ ~\127", "^%l%u%d%p%s%g%c$")"#,
        r#"string.match("aB3", "^%L")"#,
        r#"string.match("ff0G", "^%x+")"#,
        r#"string.match("abc", "((a)(b)(c))")"#,
        r#"string.match("abc", "()")"#,
        r#"string.match("abc", "a", 10)"#,
        r#"string.match("alo", "(%1)")"#,
        r#"string.match("alo", "(%0)")"#,
        r#"string.match("alo", "%")"#,
        r#"string.match("alo", "[a")"#,
        r#"string.match("alo", "[a%")"#,
        r#"string.match("alo", "[]")"#,
        r#"string.match("alo", "(a")"#,
        r#"string.match("alo", "a)")"#,
        r#"string.match("alo", "%b")"#,
        r#"string.match("alo", "%bx")"#,
        r#"string.match("alo", "%f")"#,
        r#"string.match("alo", "%fa")"#,
        r#"string.match(("a"):rep(40), ("(a)"):rep(33))"#,
        r#"string.match(("a"):rep(300), ("a?"):rep(300))"#,
        r#"string.match("abc", "^(.-)$")"#,
        r#"string.match("a\nb", "a.b")"#,
    ]);
}

#[test]
fn gmatch_agrees_with_the_reference_library() {
    assert_same(&[
        r#"(function() local t = {} for w in string.gmatch("one two  three", "%a+") do t[#t+1] = w end return t end)()"#,
        r#"(function() local t = {} for k, v in string.gmatch("a=1, b=2", "(%w+)=(%w+)") do t[#t+1] = k .. v end return t end)()"#,
        r#"(function() local t = {} for p in string.gmatch("abc", "()") do t[#t+1] = p end return t end)()"#,
        r#"(function() local t = {} for w in string.gmatch("abc", "") do t[#t+1] = w end return t end)()"#,
        r#"(function() local t = {} for w in string.gmatch("abc", "b*") do t[#t+1] = w end return t end)()"#,
        r#"(function() local t = {} for w in string.gmatch("a b c", "%a", 3) do t[#t+1] = w end return t end)()"#,
        r#"(function() local t = {} for w in string.gmatch("a b c", "%a", -1) do t[#t+1] = w end return t end)()"#,
        r#"(function() local t = {} for w in string.gmatch("abc", "", 5) do t[#t+1] = w end return t end)()"#,
        r#"(function() local t = {} for w in string.gmatch("^a^b", "^%a") do t[#t+1] = w end return t end)()"#,
        r#"(function() local f = string.gmatch("ab", "%a") return f(), f(), f(), f() end)()"#,
        r#"string.gmatch("abc", "[a")()"#,
    ]);
}

#[test]
fn gsub_agrees_with_the_reference_library() {
    assert_same(&[
        r#"string.gsub("hello world", "o", "0")"#,
        r#"string.gsub("hello world", "o", "0", 1)"#,
        r#"string.gsub("hello world", "o", "0", 0)"#,
        r#"string.gsub("hello world", "o", "0", -1)"#,
        r#"string.gsub("hello world", "(%w+)", "<%1>")"#,
        r#"string.gsub("hello world", "%w+", "%0 %0")"#,
        r#"string.gsub("hello world", "%w+", "%1")"#,
        r#"string.gsub("hello", "", "-")"#,
        r#"string.gsub("abc", "b*", "-")"#,
        r#"string.gsub("abc", "^a", "x")"#,
        r#"string.gsub("aaa", "^a", "x")"#,
        r#"string.gsub("abc", "x", "y")"#,
        r#"string.gsub("abc", "%w", "%%")"#,
        r#"string.gsub("abc", "()", "%1")"#,
        r#"string.gsub("abc", "b", 7)"#,
        r#"string.gsub("abc", "b", 1.5)"#,
        r#"string.gsub("$name is $age", "%$(%w+)", { name = "Ada", age = 36 })"#,
        r#"string.gsub("$name is $missing", "%$(%w+)", { name = false })"#,
        r#"string.gsub("abc", "%w", { a = 1, b = true })"#,
        r#"string.gsub("abc", "%w", { a = {} })"#,
        r#"string.gsub("abc", "%w", function(c) return c:upper() end)"#,
        r#"string.gsub("abc", "(%w)(%w)", function(a, b) return b .. a end)"#,
        r#"string.gsub("abc", "%w", function(c) if c == "b" then return nil end return 1 end)"#,
        r#"string.gsub("abc", "%w", function() return false end)"#,
        r#"string.gsub("abc", "%w", function() return {} end)"#,
        r#"string.gsub("abc", "%w", function() error("boom", 0) end)"#,
        r#"string.gsub("abc", "()b", function(p) return p end)"#,
        r#"string.gsub("abc", "%w", "%2")"#,
        r#"string.gsub("abc", "%w", "%x")"#,
        r#"string.gsub("abc", "%w", "%")"#,
        r#"string.gsub("abc", "(%w", "x")"#,
        r#"string.gsub("abc", "%w", true)"#,
        r#"string.gsub("abc", "%w", nil)"#,
        r#"string.gsub("a,b,,c", ",", ";")"#,
        r#"string.gsub("  trim  ", "^%s*(.-)%s*$", "%1")"#,
        r#"string.gsub("caf\xc3\xa9", "[\x80-\xff]", "?")"#,
    ]);
}

#[test]
fn rep_and_table_functions_agree_with_the_reference_library() {
    assert_same(&[
        r#"string.rep("ab", 3)"#,
        r#"string.rep("ab", 3, ",")"#,
        r#"string.rep("", 5)"#,
        r#"string.rep("", 5, "")"#,
        r#"string.rep("", 3, ",")"#,
        r#"string.rep("ab", 0)"#,
        r#"string.rep("ab", -1)"#,
        r#"string.rep(7, 2)"#,
        r#"string.rep()"#,
        r#"string.rep("", "x")"#,
        r#"(function() local t = {1, 2, 3} table.insert(t, 4) table.insert(t, 1, 0) return t end)()"#,
        r#"(function() local t = {1, 2, 3} return table.remove(t), table.remove(t, 1), t end)()"#,
        r#"(function() local t = {1, 2, 3} return table.move(t, 1, 3, 2) end)()"#,
        r#"(function() local t = {1, 2, 3} return table.move(t, 2, 3, 1) end)()"#,
        r#"(function() local t = {1, 2, 3} return table.move(t, 1, 3, 1, {}) end)()"#,
        r#"table.insert({}, 5, 1)"#,
        r#"table.insert({}, 1, 2, 3)"#,
        r#"table.insert(nil, 1)"#,
        r#"table.remove({}, 5)"#,
        r#"table.remove({})"#,
        r#"table.move({}, 1, math.maxinteger, 2)"#,
        r#"table.move({}, -1, math.maxinteger, 2)"#,
        r#"table.move({}, 1, 2)"#,
        // Tables with a metatable take the Lua path.
        r#"(function() local t = setmetatable({1, 2, 3}, {}) table.insert(t, 4) table.insert(t, 1, 0) table.insert(t, 3, 9) return t end)()"#,
        r#"(function() local t = setmetatable({1, 2, 3}, {}) return table.remove(t), table.remove(t, 1), table.remove(t, 2), t end)()"#,
        r#"(function() local t = setmetatable({1, 2, 3}, {}) return table.remove(t, 4), t end)()"#,
        r#"(function() local t = setmetatable({}, {}) return table.remove(t), table.remove(t, 0), t end)()"#,
        r#"(function() local t = setmetatable({1, 2}, { __len = function() return 4 end }) table.insert(t, "x") return t end)()"#,
        r#"table.insert(setmetatable({}, {}), 5, 1)"#,
        r#"table.insert(setmetatable({}, {}), 1, 2, 3)"#,
        r#"table.remove(setmetatable({1}, {}), 5)"#,
    ]);
}

/// A small deterministic generator, so a failure names a reproducible case.
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % bound
    }

    fn text(&mut self, alphabet: &[&str], max_items: usize) -> String {
        (0..self.below(max_items + 1))
            .map(|_| alphabet[self.below(alphabet.len())])
            .collect()
    }
}

#[test]
fn random_patterns_agree_with_the_reference_library() {
    const PATTERN: &[&str] = &[
        "a", "b", "c", ".", "%a", "%d", "%s", "%w", "%A", "[ab]", "[^a]", "[a-b]", "*", "+", "-",
        "?", "(", ")", "()", "^", "$", "%1", "%b()", "%f[%a]", "%%", "[", "]", "%",
    ];
    const SUBJECT: &[&str] = &["a", "b", "c", " ", "1", "(", ")", "%", "-"];
    let (reference, replaced) = (Lua::new(), bounded(None));
    let mut random = Lcg(213);
    for _ in 0..4_000 {
        let pattern = random.text(PATTERN, 6);
        let subject = random.text(SUBJECT, 10);
        let init = random.below(14) as i64 - 4;
        let expressions = [
            format!("string.find({subject:?}, {pattern:?}, {init})"),
            format!("string.match({subject:?}, {pattern:?}, {init})"),
            format!("string.gsub({subject:?}, {pattern:?}, '<%0|%1>')"),
            format!("string.gsub({subject:?}, {pattern:?}, string.upper, 2)"),
            format!(
                "(function() local t = {{}} for a, b in string.gmatch({subject:?}, {pattern:?}) \
                 do t[#t+1] = tostring(a) .. ':' .. tostring(b) if #t > 12 then break end end \
                 return table.concat(t, ',') end)()"
            ),
        ];
        for expression in &expressions {
            assert_eq!(
                describe(&replaced, expression),
                describe(&reference, expression),
                "{expression}"
            );
        }
    }
}

/// Backtracks in time cubic in the subject length and never matches.
const PATHOLOGICAL: &str = r#"(string.rep("a", 4000)), "^.-.-.-b""#;

#[test]
fn a_pathological_pattern_stops_at_the_step_budget() {
    for call in [
        format!("local found = string.find({PATHOLOGICAL})"),
        format!("local found = string.match({PATHOLOGICAL})"),
        format!("local found = string.gsub({PATHOLOGICAL}, '')"),
        "local found = string.gmatch(string.rep('a', 4000), '.-.-.-b')()".to_string(),
    ] {
        let lua = bounded(Some(200_000));
        let started = Instant::now();
        let error = lua.load(&call).set_name("=flow").exec().unwrap_err();
        assert!(
            error.to_string().contains(
                "flow:1: Lua pattern matching exceeded step budget of 200000 \
                 (IRONFLOW_LUA_MAX_PATTERN_STEPS)"
            ),
            "{call}: {error}"
        );
        assert!(started.elapsed() < Duration::from_secs(5), "{call}");
    }
}

#[test]
fn the_step_budget_is_shared_by_every_call_of_a_lua_state() {
    let lua = bounded(Some(50_000));
    let error = lua
        .load(
            r#"
            local subject = string.rep("a", 1000)
            for _ = 1, 1000 do string.find(subject, "b+") end
            "#,
        )
        .exec()
        .unwrap_err();
    assert!(
        error.to_string().contains("exceeded step budget of 50000"),
        "{error}"
    );

    // A caught budget error does not buy more steps.
    let lua = bounded(Some(50_000));
    let caught: bool = lua
        .load(format!(
            "pcall(string.find, {PATHOLOGICAL}) return pcall(string.find, 'abc', 'b+')"
        ))
        .eval()
        .unwrap();
    assert!(!caught);
}

#[test]
fn linear_matching_fits_the_default_budget() {
    let lua = new_sandboxed_lua().unwrap();
    apply_lua_limits(&lua, LuaExecutionLimits::from_env()).unwrap();
    let count: i64 = lua
        .load(
            r#"
            local text = string.rep("lorem ipsum dolor sit amet, ", 40000)
            local _, words = string.gsub(text, "%a+", string.upper)
            local trimmed = string.gsub(text, "%s+", " ")
            assert(#trimmed == #text)
            return words
            "#,
        )
        .eval()
        .unwrap();
    assert_eq!(count, 200_000);
}

#[test]
fn a_pathological_pattern_stops_at_the_lua_time_budget() {
    let lua = new_sandboxed_lua().unwrap();
    let mut limits = limits(None);
    limits.max_seconds = Some(1);
    apply_lua_limits(&lua, limits).unwrap();
    let started = Instant::now();
    let error = lua
        .load(r#"return string.find(string.rep("a", 100000), "^.-.-.-b")"#)
        .exec()
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Lua execution exceeded time budget of 1s"),
        "{error}"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn a_pathological_pattern_stops_at_the_step_deadline() {
    let started = Instant::now();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(200);
    let error = crate::util::execution::with_execution_deadline(
        Some(deadline),
        crate::util::execution::run_tracked_blocking_step(move |execution| {
            let lua = new_sandboxed_lua()?;
            apply_lua_limits_with_control(&lua, limits(None), Some(execution))?;
            let error = lua
                .load(r#"return string.gsub(string.rep("a", 100000), "^.-.-.-b", "")"#)
                .exec()
                .unwrap_err();
            Ok(error.to_string())
        }),
    )
    .await
    .unwrap();
    assert!(error.contains("step deadline exceeded"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_gsub_result_cannot_outgrow_the_lua_memory_cap() {
    let lua = new_sandboxed_lua().unwrap();
    let mut limits = limits(None);
    limits.max_memory_bytes = Some(8 * 1024 * 1024);
    apply_lua_limits(&lua, limits).unwrap();
    let error = lua
        .load(r#"return string.gsub(string.rep("a", 100000), "", string.rep("x", 100000))"#)
        .exec()
        .unwrap_err();
    assert!(
        error.to_string().contains(
            "Lua string result exceeded memory limit of 8388608 bytes \
             (IRONFLOW_LUA_MAX_MEMORY_BYTES)"
        ),
        "{error}"
    );
}

#[test]
fn table_calls_that_looped_in_c_now_stop_at_the_instruction_budget() {
    for call in [
        "table.move({}, 1, math.maxinteger - 1, 2)",
        "table.move({}, 1, 1 << 40, 1, {})",
        "table.insert(setmetatable({}, { __len = function() return 1 << 60 end }), 1, 0)",
        "table.remove(setmetatable({}, { __len = function() return 1 << 60 end }), 1)",
    ] {
        let lua = bounded(None);
        let started = Instant::now();
        let error = lua.load(call).exec().unwrap_err().to_string();
        assert!(
            error.contains("Lua execution exceeded instruction budget"),
            "{call}: {error}"
        );
        assert!(started.elapsed() < Duration::from_secs(10), "{call}");
    }
}

#[test]
fn repeating_an_empty_string_does_not_loop() {
    let lua = bounded(None);
    let started = Instant::now();
    let length: i64 = lua
        .load(r#"return #string.rep("", math.maxinteger) + #string.rep("", 1 << 60, "")"#)
        .eval()
        .unwrap();
    assert_eq!(length, 0);
    assert!(started.elapsed() < Duration::from_secs(2));
}
