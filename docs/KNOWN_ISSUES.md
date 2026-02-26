# IronFlow — Known Issues

Issues discovered during development and testing. Resolved issues are removed from this list.

---

### 1. Inline Lua via API requires proper JSON escaping

**Severity:** Low
**Component:** REST API (`POST /flows/run`, `POST /flows/validate`)
**Description:** When sending Lua source as inline JSON in the `source` field, newlines and special characters must be JSON-escaped (e.g., `\n` for newlines). Sending raw multi-line Lua without escaping causes a JSON parse error from axum's body parser, not a Lua-specific error message.
**Workaround:** Use the `file` field to reference a `.lua` file on disk, or properly escape the Lua source in JSON.
