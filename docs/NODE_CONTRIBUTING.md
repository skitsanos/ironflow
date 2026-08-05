# Node Contributor Manual

This manual describes how to add a new built-in node to IronFlow.

If you are adding a user-facing node, also update `docs/NODE_REFERENCE.md` with the full public contract and example usage.

## 1) Understand the node lifecycle

A flow step stores:
- `node_type` (string)
- `config` (JSON object)

At execution:
1. `engine::executor` resolves the `node_type` from `NodeRegistry`.
2. It calls `node.execute(config, &context)`.
3. A successful `NodeOutput` is buffered until the current DAG phase settles,
   then committed to global flow context in flow declaration order.
4. Execution errors are handled with retries and an optional total step
   deadline shared by all attempts and retry backoffs. A `NodeFailure` can
   publish the terminal failed attempt's structured output.

So your node implementation only needs to do one thing:
- Read and validate config.
- Perform the operation.
- Return a `NodeOutput` map, or a `NodeFailure` when failed structured output
  is part of the contract.

## 2) Node contract (mandatory)

All nodes implement:

```rust
use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

#[async_trait::async_trait]
impl Node for YourNode {
    fn node_type(&self) -> &str;
    fn description(&self) -> &str;
    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> anyhow::Result<NodeOutput>;
}
```

Implemented in:
- `src/nodes/mod.rs` (`Node` trait definition)
- `src/engine/types.rs` (`Context`, `NodeOutput` aliases)

### Contract details

- `node_type` must be unique across all nodes.
- `description` should be short and user-facing (single line).
- `execute` receives a shared, read-only phase-start context reference
  (`&Context`). Independent phase members never see peer output. Do not mutate
  input context directly; return a `NodeOutput` map and let the executor publish
  it at the phase barrier.
- `Node` must be `Send + Sync` because `Node` is used across async execution.
- Return errors with actionable messages (`anyhow::anyhow!(...)`).
- A normal `Err` never publishes output. If a completed external operation must
  fail the task while preserving structured diagnostics, return
  `nodes::NodeFailure`; the executor publishes only the terminal failed
  attempt's redacted output.

## 3) Output semantics

`NodeOutput` is `HashMap<String, serde_json::Value>`. Successful output is
buffered and committed at the phase barrier; structured failure output follows
the terminal-only rules below. If same-phase nodes publish the same key, the
later-declared step wins. Prefer configurable, node-specific `output_key`
prefixes and document every fixed companion key so parallel callers can avoid
collisions.

Pattern to build output:

```rust
let mut output = NodeOutput::new();
output.insert("foo".to_string(), serde_json::json!("bar"));
Ok(output)
```

Structured failure pattern:

```rust
use crate::nodes::NodeFailure;

if !result.success {
    return Err(NodeFailure::new("provider rejected the request", output).into());
}
```

Structured output stays private during retries. If retries are exhausted, the
final attempt's output is buffered for the phase commit, persisted in the
failed task, and supplied to its recovery handler as invocation-local
`_error_output`. Do not put secrets in output; execution overlays are redacted
as a persistence fence, not as a general secret-discovery mechanism.

Common conventions used by existing nodes:
- use explicit success flags like `*_success` for status nodes.
- use clear count/result naming (for example `items`, `items_count`).
- avoid mutating input `Context` directly; return a map instead.

## 4) How to parse config safely

Prefer strict required/optional parsing with clear errors:

```rust
use crate::util::node_config::{config_bool_or, config_f64_or, config_u64_strict};

let path = config
    .get("path")
    .and_then(|v| v.as_str())
    .ok_or_else(|| anyhow::anyhow!("node requires 'path'"))?;

let timeout = config_f64_or(config, "timeout", ctx, 1.0)?;
let retries = config_u64_strict(config, "retries", ctx)?.unwrap_or(0);
let pretty = config_bool_or(config, "pretty", ctx, false)?;
```

**Read numeric and boolean parameters with the `node_config` helpers, never
with a bare `as_f64()` / `as_u64()` / `as_bool()`.** Use strict `*_or` forms
when a parameter has a default, so an invalid present value is not mistaken for
an absent value. Flow authors write parameters as `"${ctx.key}"` templates, and
interpolation always produces a *string*. A bare `as_f64()` returns `None` for
that string, so `unwrap_or(default)` silently discards the caller's value — no
error, no warning. The helpers accept a native JSON value, a string form, or a
`${ctx.key}` template resolving to either. `config_bool` accepts
`true`/`yes`/`on`/`1` and `false`/`no`/`off`/`0`, case-insensitive.

This applies only to **node config**. Values read from an API response or another
non-config JSON document should keep using the plain `as_*` accessors.

Guidelines:
- Keep config keys backward compatible when possible.
- For strings and arrays, validate expected type exactly.
- For optional values use `unwrap_or` defaults and document them.
- Opt a config field into `${ctx...}` interpolation where user data should be
  templated, and document that behavior explicitly.
- Use the shared helpers in `crate::lua::interpolate`; do not add a node-local
  path parser or recursive JSON walker.

## 5) Make execution cancellation-safe

An async node should keep owned resources inside its returned future so
dropping that future cancels pending I/O. Do not detach background work unless
the node's documented contract is explicitly fire-and-forget.

CPU-heavy or synchronous work must not run directly on a Tokio runtime worker.
Use `crate::util::execution::run_blocking_step` and call the supplied
`ExecutionControl::checkpoint()` between meaningful units of work. The helper
propagates the enclosing step deadline and signals cancellation when the async
waiter is dropped. A third-party blocking call that never returns cannot be
forcibly unwound, so document that boundary and avoid starting irreversible
side effects before a checkpoint when possible.

Node-owned subprocesses should use the shared command configuration and RAII
guard in `crate::nodes::child_process`; cleanup must happen from `Drop`, not
only from a node-local timeout branch. Never assume timeout can roll back an
external side effect that already completed.

## 6) Context interpolation

Interpolation is opt-in per documented config field. IronFlow does not rewrite
all strings in every node config. A scalar string field that supports
interpolation should call:

```rust
let interpolated = interpolate_ctx(&raw_value, &ctx);
```

For arbitrary JSON payloads, use
`crate::lua::interpolate::interpolate_value` rather than walking objects and
arrays inside the node. Object keys remain unchanged.

The accepted path grammar is `${ctx.user.name}`, `${ctx.items[0].name}`, and
`${ctx["key.with.dots"]}`. Array indexes are zero-based. Interpolation does not
evaluate expressions, calls, or fallback operators; missing and `null` values
render as an empty string. Non-`ctx` forms such as `${HOME}` remain literal.
The runtime escape `\${ctx.foo}` emits `${ctx.foo}`; Lua source must spell it
`"\\${ctx.foo}"`.

Add tests and parameter documentation for every field you opt in. Include
nested object/array coverage when using the recursive helper.

## 7) Place the node in the right folder

Nodes live in category folders under `src/nodes/<category>/`, one file per node (or a small group of closely related nodes), with shared helpers in their own files within the folder. Pick the category that fits — e.g.:
- `src/nodes/http/`, `src/nodes/file/`, `src/nodes/transform/`, `src/nodes/database/`, `src/nodes/extract/`, `src/nodes/image/`, `src/nodes/ai/`, `src/nodes/cloud/`, `src/nodes/s3vector/`, `src/nodes/notify/`, `src/nodes/mcp/`, `src/nodes/composition/`, `src/nodes/utility/`.

Add your node's struct to the appropriate file (e.g. `src/nodes/transform/json.rs`), or create a new file in the folder if it's a distinct responsibility. Keep files focused, target no more than 300 physical lines, and split responsibilities rather than let a file exceed 400 lines. Move large helpers/parsers into their own sibling file. If your node needs a brand-new category, create `src/nodes/<category>/mod.rs` with a `pub fn register_all(registry: &mut NodeRegistry)` and add `pub mod <category>;` to `src/nodes/mod.rs`.

## 8) Register the node

Update:

1) The category's `src/nodes/<category>/mod.rs`
- add the `mod` declaration / `pub use` re-export for your node's file if needed
- add a `registry.register(Arc::new(YourNode))` line inside that folder's `register_all(registry)` function

2) `src/nodes/mod.rs::with_builtins()` calls each category's `register_all(&mut registry)` directly. If you added a new category, add its `register_all` call there.

3) If the node is special for subflow execution (child registry behavior), update `src/nodes/composition/subworkflow.rs` only if needed.

## 9) Lua API exposure

Nodes are exposed to Lua from `LuaRuntime` by iterating over `NodeRegistry::list()` and creating `nodes.<node_type>(...)` factories.

This means:
- Once registered, your node is automatically available in flows.

Only add custom Lua handling when needed:
- if your node accepts a Lua function as input, use bytecode transport patterns used by `code` and `foreach`.
- if your node accepts a custom nested DSL, add conversion logic in `LuaRuntime` and convert to stable JSON before execution.

## 10) Documentation requirements

For each new node add:
1. `docs/nodes/<node_type>.md`
   - Parameters table
   - Output fields
   - Example Lua usage
2. entry in `docs/NODE_REFERENCE.md` table under the correct section
3. update any top-level node-count / feature notes if needed:
   - `docs/NODE_REFERENCE.md`
   - `README.md` (if listing total built-in count)
4. add to any docs/changelog if this changes behavior

## 11) Tests

Minimum required coverage:
- add/extend integration tests in `tests/`:
  - validate required config error
  - happy path with output keys/counts
  - null/edge behavior
  - enclosing step timeout or dropped-future cleanup for owned resources
  - cooperative deadline behavior for CPU/synchronous work

If runtime parsing is affected, add flow-level tests using:
- `LuaRuntime::load_flow_from_string`
- `NodeRegistry::with_builtins`
- direct `node.execute(...)` calls where appropriate

Prefer generating narrow corner-case fixtures inside the test itself. If a Lua
example also needs the asset, add one compact synthetic file under
`examples/fixtures/`, record it in `SHA256SUMS`, and avoid third-party or
production data.

## 12) Example implementation template

```rust
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

pub struct MyNode;

#[async_trait::async_trait]
impl Node for MyNode {
    fn node_type(&self) -> &str {
        "my_node"
    }

    fn description(&self) -> &str {
        "Short one-line description"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let input = config
            .get("input")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("my_node requires 'input'"))?;

        let interpolated = crate::lua::interpolate::interpolate_ctx(input, &ctx);

        // implement logic
        let result = format!("processed:{}", interpolated);

        let mut output = NodeOutput::new();
        output.insert("my_node_result".to_string(), json!(result));
        output.insert("my_node_success".to_string(), json!(true));
        Ok(output)
    }
}
```

## 13) Linting and quality

Before opening PR:
- run targeted tests for the new behavior
- run `python3 -B scripts/check_module_size.py`
- run `cargo clippy --all-targets -- -D warnings`
- ensure no broad warnings.

The module-size check scans production `src/**/*.rs` files, including inline
tests. Files through 300 lines need no exception; a cohesive 301–400-line file
must have its exact physical-line ceiling and an architectural rationale in
`scripts/module_size_policy.json`. If it shrinks, lower the ceiling immediately;
remove the entry at 300 lines or fewer. New exceptions and exception-budget
changes are architecture-policy decisions that require maintainer review.
IF-034 established 13 reviewed exceptions; IF-052 later approved a temporary
ceiling of 17 for security-fix growth, and subsequent extraction ratcheted the
current checked-in ceiling to 11. A new exception must therefore replace an
existing one; files above 400 lines are never exempt. Passing this check does not
establish good modularity: reviewers must still examine responsibility
boundaries, cognitive complexity, and whether moving tests or helpers improves
cohesion.

## 14) Common mistakes to avoid

- returning `NodeOutput` with missing defaults when config is invalid
- building diagnostics and then returning an ordinary error that discards
  them; use `NodeFailure` when terminal structured output is part of the node's
  contract
- mutating global/shared state directly
- storing non-serializable or huge payloads in output
- forgetting to update docs/tests when adding or renaming config fields
- running synchronous CPU/I/O work directly on a Tokio worker
- cleaning up a subprocess only in the success or node-local timeout path

## 15) Cover every built-in node with an example flow

Every node registered by `NodeRegistry::with_builtins()` must appear in at least
one evaluated Lua flow under `examples/`. Add or extend an example whenever a
built-in node is introduced. `tests/test_example_catalog.rs` evaluates every
flow and rejects registered nodes without coverage.

If a built-in genuinely cannot have a runnable example, add its node type and a
specific, non-empty reason to `node_coverage.exemptions` in
`examples/catalog.json`:

```json
"node_coverage": {
  "exemptions": {
    "node_type": "Explain why no runnable example can cover this node."
  }
}
```

The catalog test rejects blank reasons, unknown node types, and stale
exemptions once an example covers the node.

Recommended structure:
- demonstrate successful use case
- show failure/error handling where appropriate
- document required context, credentials, external services, native libraries,
  executables, and operating-system constraints
- document local writes and remote mutations, including what remains after a
  successful run and what can remain when success-dependent cleanup is skipped
- record the flow exactly once in `examples/catalog.json` and assign every
  applicable `external_service`, `credentialed`, `local_state`, and
  `platform_specific` label; requirement labels are composable
- prefer an existing workflow when it can cover the node coherently, and link
  that runnable workflow from the node reference
- use run-owned, collision-resistant output paths instead of fixed machine-wide
  paths; `--store-dir` isolates engine state, not files written by the flow
- resolve versioned inputs from `_flow_dir`; never write generated output back
  into `examples/fixtures/`
