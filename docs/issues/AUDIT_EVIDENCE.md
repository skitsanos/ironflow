# IronFlow audit evidence

These cross-issue snapshots and resource baselines were preserved from the original engineering ledger. They are historical evidence, not the current runtime contract.

## Fresh audit 2026-07-24 (IF-035+)

Second deep Rust/Lua/documentation audit on `develop` at v1.12.0, independent of
IF-001..IF-034 (which were re-verified as genuinely resolved). Baseline was
healthy: `cargo fmt --check`, `cargo clippy --all-features --all-targets`, the
default `cargo test`, and `cargo test --features postgres` all passed. The
recurring theme is that existing safety machinery (the `IRONFLOW_MAX_*` limits,
the IF-001 sandbox, the IF-006 blocking-pool offload) is not uniformly applied
to every node and API path.

## Audit evidence snapshot

- Branch: `develop`.
- Baseline before remediation: formatting, all-target check, strict Clippy, and
  497 Rust tests passed under default features.
- `cargo check --all-targets --features postgres,redis` passed.
- Registry: 98 nodes; docs: 98 node pages; names matched exactly.
- Lua catalog: 125/125 passed static validation.
- Representative offline workflows passed actual execution, but static
  validation cannot prove fixtures, external transports, or runtime semantics.
- `tests/test_examples.rs` checks one README entry, not the executable catalog.

Remediation gate completed on 2026-07-22:

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo check --all-targets --features postgres,redis`
- `cargo test --all-targets`
- `cargo test --doc`
- `cargo clippy --all-targets -- -D warnings`
- `cargo clippy --all-targets --features postgres,redis -- -D warnings`
- IF-004 terminalization regressions: 6/6 passed (state/context/status write
  failures, panic, cancellation, and detached waiter).
- IF-005 disposable-Redis gate passed against Redis 8.8.0 from `redis:latest`
  (`redis@sha256:234c902a2db49461a129e2d4aeff85b28cf20187ed274a67f6e50995fa713c7b`):
  the complete serial Redis-feature suite passed, including 22/22 atomicity,
  contention, fault-injection, expiry, legacy-migration, and alias regressions.
- IF-006 deadline/cancellation regressions passed 14/14: total retry budgets,
  Lua loop and child-loader preemption, async drop, retry-wait cancellation,
  Unix shell/MCP process trees, and all structured child-workflow paths. The
  complete `postgres,redis` all-target test suite also passed, and no disposable
  cancellation-test subprocess remained afterward.
- 125/125 Lua examples passed static validation.
- README hello-world and data-pipeline workflows completed with `Status:
  success`; control-flow validation and both documented live API submission
  forms succeeded.

IF-028/IF-029 closure gate completed on 2026-07-24:

- `cargo fmt --all -- --check` and `git diff --check`
- `cargo check --all-targets` under default and `postgres,redis` features
- `cargo clippy --all-targets -- -D warnings` under default and
  `postgres,redis` features
- `cargo test --all-targets` under default and `postgres,redis` features; the
  feature-enabled inventory contains 894 tests
- `cargo test --doc`
- JSON ordered-catalog regressions: 14/14 passed, including clean/deep pages
  without directory enumeration, concurrent writer/read races, corruption
  recovery, ordering, and symlink defenses
- 125/125 Lua examples passed static validation
- Rust source inventory: zero files above 400 lines; 13 cohesive or test-heavy
  files remain between 301 and 400 lines

IF-034 closure gate completed on 2026-07-24:

- Module-size checker regressions: 16/16 passed
- Live source inventory: 312 production Rust modules, 13/13 exact reviewed
  exceptions, and zero files above 400 lines
- `actionlint .github/workflows/ci.yml`
- `cargo fmt --all -- --check` and `git diff --check`
- `cargo clippy --all-targets -- -D warnings` under default and
  `postgres,redis` features
- `cargo test --all-targets` and `cargo test --doc`

## Extraction resource baseline before IF-065 (2026-07-31)

The release binary at `fac37b6` (`1.16.0-dev.2`) was profiled on macOS 26.5.2,
an Apple M4 Pro with 12 logical CPUs and 24 GiB RAM. Each case ran in a fresh
JSON state directory under `/usr/bin/time -lp`; the table reports the median of
three runs. CPU is user plus system time. These short wall times are coarse and
include a roughly 0.3-second CLI/startup/storage floor, so the useful signals
are peak RSS, output amplification, limit behavior, and concurrency rather than
small timing differences.

`Output` is the compact serialized task output before the executor's 2 MiB
history truncation. `data/samples/` is gitignored and machine-local, so these
figures are calibration evidence, not a reproducible CI gate.

| Case | Input / shape | Wall | CPU | Peak RSS | Output |
|---|---:|---:|---:|---:|---:|
| Empty workflow baseline | — | 0.32 s | 0.03 s | 17.1 MiB | 11 B |
| XLSX small | 177,369 B / 432 cells | 0.26 s | 0.04 s | 18.5 MiB | 3,311 B |
| XLSX near default ceiling | 354,086 B / 32,014 cells | 0.33 s | 0.10 s | 59.7 MiB | 694,273 B |
| XLSX raised ceiling | 720,564 B / 85,908 cells | 0.41 s | 0.16 s | 98.3 MiB | 1,375,676 B |
| Four concurrent XLSX steps | 4 x 32,014 cells | 0.72 s | 0.38 s | 181.1 MiB | about 2.65 MiB |
| PDF text + metadata | 108,505 B / one page | 0.26 s | 0.03 s | 21.6 MiB | 3,277 B |
| Word JSON + metadata/comments | 19,842 B | 0.26 s | 0.04 s | 23.9 MiB | 63,241 B |
| VTT text + cues/metadata | 11,097 B / 80 cues | 0.25 s | 0.03 s | 18.6 MiB | 32,425 B |
| PPTX text | 38,741,985 B / 86 slides | 0.29 s | 0.06 s | 21.0 MiB | 109,757 B |
| PPTX JSON, no image bytes | same deck | 0.28 s | 0.05 s | 40.4 MiB | 193,002 B |
| PPTX JSON with image bytes | same deck | 0.34 s | 0.10 s | 231.3 MiB | 36,404,071 B |
| PPTX text with image bytes requested (pre-IF-065) | same deck | 0.32 s | 0.09 s | 98.2 MiB | 109,757 B |

The 85,908-cell workbook is correctly rejected by the default 33,000-cell
budget; its successful row above used `IRONFLOW_MAX_XLSX_CELLS=100000`. A Lua
consumer would also need a coordinated conversion-node budget. The four-way
case used distinct output keys and `IRONFLOW_MAX_CONCURRENT_TASKS=4`. The
1.84 MiB `generated_book.pdf` sample is image-only for this extractor and
returned no text, so it is not a useful PDF stress case.

The PPTX contains 536 ZIP entries and 119 media files (about 26 MiB
uncompressed). Its 102 resolved image occurrences refer to 85 unique paths; one
image is read and encoded eleven times. When image bytes are enabled, the full
36.4 MiB result is materialized and copied before durable output is replaced by
the 2 MiB truncation marker. At the profiled pre-remediation commit, requesting
image bytes with text output produced the same 109,757-byte result as ordinary
text while raising median RSS from 21.0 to 98.2 MiB. The current contract
rejects `include_image_bytes = true` for every format and directs JSON callers
to `media_mode = "artifact"`.

A post-remediation release build from the final IF-065 working tree was checked
against the same 37 MiB deck with fresh state directories (median of three
runs). This is a compatibility/resource spot check, not a benchmark gate:

| Current IF-065 case | Wall | CPU | Peak RSS |
|---|---:|---:|---:|
| PPTX text | 0.27 s | 0.04 s | 20.4 MiB |
| PPTX JSON, no image bytes | 0.29 s | 0.05 s | 38.7 MiB |
| PPTX JSON with image bytes | 0.33 s | 0.10 s | 247.0 MiB |

Text and metadata-only JSON remained near or below the baseline footprint.
Embedded-image JSON remains the dominant allocation path and measured higher
in this spot check despite similar CPU and wall time. IF-065 provides bounded
work, explicit failure, and cancellation lifecycle guarantees; it does not
claim an RSS reduction. Repeated-media and output-copy amplification therefore
remain P1 work under IF-066, while IF-067 owns reproducible trend measurement.
