# Next IronFlow release — draft

Development candidate: **1.19.1-dev.1**. These changes are after v1.19.0;
this draft does not authorize or announce another stable release.

## Toolchain and dependencies

- Update the exact local, CI, release, and container Rust toolchain to 1.99.0.
  No cargo-chef image for that Rust version was published when this candidate
  was prepared, so the builder uses the digest-pinned official Rust image
  and installs the existing cargo-chef 0.1.78 with `--locked` dependencies.
  Dependency cooking remains separate from application source compilation.
- Group Renovate updates for the Rust Docker base and local/CI toolchain pins.
- Replace the deprecated atomic `fetch_update` call in the S3 artifact retry
  test helper with `try_update`, preserving its orderings and failure budget.
- Update JSON Schema and its companion crates from 0.58.1 to 0.58.4.
- Update AWS S3 to 1.152.0, S3 Vectors to 1.42.0, and their compatible Smithy
  HTTP client/runtime crates together.
- Replace yanked `yoke-derive 0.8.3` with 0.8.4. The dependency audit passes
  with warnings denied and no advisory exceptions.

Existing workflow configuration and defaults are unchanged. Published v1.19.0
notes are retained in [1.19.0.md](./1.19.0.md).
