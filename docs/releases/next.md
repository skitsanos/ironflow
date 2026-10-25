# Next IronFlow release - draft

Development candidate: **1.19.2-dev.2**, after the published v1.19.1 security
release. This draft does not announce another stable release.

## Dependencies

- Integrate dependency PRs #218, #219, #221, #222, #223 and #224: serde_json
  1.0.154, SHA-2 0.11.1, tree-sitter 0.27.1 (tree-sitter-language 0.1.9),
  Redis 1.8.0, JSON Schema 0.60.1 and lopdf 0.46.0.
- Update ZIP to 9.1.0, with its LZMA 0.23.0 and typed-path 0.13.0 dependencies
  (PR #226). Existing archive admission, path and cancellation limits remain
  unchanged.
- Keep PDF default features disabled and retain only the selected chrono
  feature. Preserve the existing tempfile/getrandom resolution rather than
  importing unrelated resolver churn.
- Retain Rust 1.99.1 and the v1.19.1 TLS distrust updates, container CA floors,
  and disabled SigV4a signing. No advisory ignores are added.

## Regression coverage

- Verify object member order does not change const/enum or uniqueItems
  validation in either schema node, including inline and context schemas.
- Replace a 50 ms XLSX timing assertion with a corrupt-payload regression that
  proves declared limits are checked before decompression (IF-154).
- Check PDF dependency feature policy structurally rather than freezing an
  unrelated version string.

No new workflow options or configuration defaults are introduced. Existing
v1.19.1 upgrade limitations remain documented in [the security guide](../SECURITY.md).
