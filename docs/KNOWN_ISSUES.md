# IronFlow — Known Issues

Issues discovered during development and testing. Resolved issues are removed from this list.

---

1. Function handlers cannot capture local variables from their enclosing scope
- Severity: Low (by design)
- Component: Lua runtime / code node
- Current behavior: Function handlers are serialized to bytecode at parse time and executed in a fresh Lua VM at runtime. Local variables captured as upvalues (closures) will be `nil` in the execution VM. Globals like `env()` and `ctx` are explicitly provided and work correctly.
- Workaround: Use `env()` and `ctx` directly inside the function body instead of capturing locals:
  ```lua
  -- BAD: captured local will be nil at runtime
  local key = env("API_KEY")
  flow:step("x", function(ctx) return { key = key } end)

  -- GOOD: call env() inside the handler
  flow:step("x", function(ctx) return { key = env("API_KEY") } end)
  ```
- Note: Upvalue validation via `debug.getupvalue` is not possible because mlua's `send` feature sandboxes the debug library. If a user captures a local, they will get a `nil` value at runtime — a clear Lua error that points to the issue.
