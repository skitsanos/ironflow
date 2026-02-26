# IronFlow — Known Issues

Issues discovered during development and testing. Resolved issues are removed from this list.

---

### 1. `validate` command does not check for DAG cycles

**Severity:** Low
**Component:** CLI (`cmd_validate`)
**Description:** The `ironflow validate` command checks that node types exist and dependencies reference valid step names, but does not run a topological sort to detect cycles. Cycles are only detected at execution time by the engine.
**Workaround:** Cycles will be caught when running the flow via `ironflow run` or `POST /flows/run`.

### 2. Inline Lua via API requires proper JSON escaping

**Severity:** Low
**Component:** REST API (`POST /flows/run`, `POST /flows/validate`)
**Description:** When sending Lua source as inline JSON in the `source` field, newlines and special characters must be JSON-escaped (e.g., `\n` for newlines). Sending raw multi-line Lua without escaping causes a JSON parse error from axum's body parser, not a Lua-specific error message.
**Workaround:** Use the `file` field to reference a `.lua` file on disk, or properly escape the Lua source in JSON.

### 3. `list_directory` recursive mode is incomplete

**Severity:** Low
**Component:** Nodes (`list_directory`)
**Description:** The `recursive` parameter is accepted but the node only lists immediate entries with a `type: "directory"` marker. It does not descend into subdirectories.
**Workaround:** None currently. Full recursive listing can be added in a future iteration.
