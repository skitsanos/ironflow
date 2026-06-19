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
3. The returned `NodeOutput` is merged into the global flow context.
4. Execution errors are handled with retries and optionally timeouts.

So your node implementation only needs to do one thing:
- Read and validate config.
- Perform the operation.
- Return a `NodeOutput` map to merge into context.

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
- `execute` receives a shared, read-only context reference (`&Context`). Do not mutate input context directly; return a `NodeOutput` map and let the executor merge it back into workflow context.
- `Node` must be `Send + Sync` because `Node` is used across async execution.
- Return errors with actionable messages (`anyhow::anyhow!(...)`).

## 3) Output semantics

`NodeOutput` is `HashMap<String, serde_json::Value>` and is merged into context.

Pattern to build output:

```rust
let mut output = NodeOutput::new();
output.insert("foo".to_string(), serde_json::json!("bar"));
Ok(output)
```

Common conventions used by existing nodes:
- use explicit success flags like `*_success` for status nodes.
- use clear count/result naming (for example `items`, `items_count`).
- avoid mutating input `Context` directly; return a map instead.

## 4) How to parse config safely

Prefer strict required/optional parsing with clear errors:

```rust
let path = config
    .get("path")
    .and_then(|v| v.as_str())
    .ok_or_else(|| anyhow::anyhow!("node requires 'path'"))?;

let timeout = config.get("timeout").and_then(|v| v.as_f64()).unwrap_or(1.0);
```

Guidelines:
- Keep config keys backward compatible when possible.
- For booleans, numbers, strings, and arrays, validate expected type exactly.
- For optional values use `unwrap_or` defaults and document them.
- Include `{ctx}` interpolation where user data should be templated.
  - Helpers are available in `crate::lua::interpolate::interpolate_ctx`.

## 5) Context interpolation

Most nodes that send external payloads or filenames should call:

```rust
let interpolated = interpolate_ctx(&raw_value, &ctx);
```

Use this for user-provided strings (URLs, paths, templates, SQL statements, etc.).

## 6) Place the node in the right folder

Nodes live in category folders under `src/nodes/<category>/`, one file per node (or a small group of closely related nodes), with shared helpers in their own files within the folder. Pick the category that fits — e.g.:
- `src/nodes/http/`, `src/nodes/file/`, `src/nodes/transform/`, `src/nodes/database/`, `src/nodes/extract/`, `src/nodes/image/`, `src/nodes/ai/`, `src/nodes/cloud/`, `src/nodes/s3vector/`, `src/nodes/notify/`, `src/nodes/mcp/`, `src/nodes/composition/`, `src/nodes/utility/`.

Add your node's struct to the appropriate file (e.g. `src/nodes/transform/json.rs`), or create a new file in the folder if it's a distinct responsibility. Keep files focused and under ~400 LOC; move large helpers/parsers into their own sibling file. If your node needs a brand-new category, create `src/nodes/<category>/mod.rs` with a `pub fn register_all(registry: &mut NodeRegistry)` and add `pub mod <category>;` to `src/nodes/mod.rs`.

## 7) Register the node

Update:

1) The category's `src/nodes/<category>/mod.rs`
- add the `mod` declaration / `pub use` re-export for your node's file if needed
- add a `registry.register(Arc::new(YourNode))` line inside that folder's `register_all(registry)` function

2) `src/nodes/mod.rs::with_builtins()` calls each category's `register_all(&mut registry)` directly. If you added a new category, add its `register_all` call there.

3) If the node is special for subflow execution (child registry behavior), update `src/nodes/composition/subworkflow.rs` only if needed.

## 8) Lua API exposure

Nodes are exposed to Lua from `LuaRuntime` by iterating over `NodeRegistry::list()` and creating `nodes.<node_type>(...)` factories.

This means:
- Once registered, your node is automatically available in flows.

Only add custom Lua handling when needed:
- if your node accepts a Lua function as input, use bytecode transport patterns used by `code` and `foreach`.
- if your node accepts a custom nested DSL, add conversion logic in `LuaRuntime` and convert to stable JSON before execution.

## 9) Documentation requirements

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

## 10) Tests

Minimum required coverage:
- add/extend integration tests in `tests/`:
  - validate required config error
  - happy path with output keys/counts
  - null/edge behavior

If runtime parsing is affected, add flow-level tests using:
- `LuaRuntime::load_flow_from_string`
- `NodeRegistry::with_builtins`
- direct `node.execute(...)` calls where appropriate

Recommended to add a simple fixture in `tests/`.

## 11) Example implementation template

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

## 12) Linting and quality

Before opening PR:
- run targeted tests for the new behavior
- run `cargo clippy --tests -- -D warnings`
- ensure no broad warnings.

## 13) Common mistakes to avoid

- returning `NodeOutput` with missing defaults when config is invalid
- swallowing errors and returning partial output
- mutating global/shared state directly
- storing non-serializable or huge payloads in output
- forgetting to update docs/tests when adding or renaming config fields

## 14) Optional: add an example flow

Add an example in `examples/` when the node is user-facing.

Recommended structure:
- demonstrate successful use case
- show failure/error handling where appropriate
- document prerequisites (e.g., external binary/service)
