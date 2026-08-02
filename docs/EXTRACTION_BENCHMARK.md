# Extraction resource benchmark

IronFlow includes an opt-in, release-mode subprocess benchmark for comparing
the CPU and memory shape of document extraction across machines and commits.
It is a trend tool, not a wall-time correctness gate.

## Run it

The benchmark requires Bun, Rust, and `/usr/bin/time` on macOS or Linux. Pass
local calibration documents explicitly; `data/samples/` is ignored by Git and
must never become an example dependency.

```bash
bun run scripts/extraction_benchmark.ts \
  --samples data/samples \
  --repetitions 3 \
  --concurrency 1,2,4 \
  --output benchmarks/results/local.jsonl
```

The script builds `extraction_benchmark_worker` in release mode once, then runs
each input in a separate process. Supported extensions are `.docx`, `.pptx`,
`.pdf`, `.html`, `.htm`, `.srt`, `.vtt`, and `.xlsx`. Use `--skip-build` only
when the release worker is already current. `--cancel-after-ms` controls the
pathological-HTML cancellation probe and defaults to 2 ms.

Every concurrency value runs that many copies of the same input together. The
committed deterministic corpus covers long ZIP filenames, repeated PPTX media,
compressed PDF text, and pathological HTML. It is intentionally compact; local
samples provide calibration at realistic sizes. The `baseline/empty` case
measures process/runtime startup.

## JSONL schema

Each line is one subprocess observation. It contains no extracted document
content and no raw error message.

| Field | Meaning |
|-------|---------|
| `schema_version` | Result schema, currently `1` |
| `measured_at` | UTC observation timestamp |
| `machine` | OS, architecture, CPU model/count, total memory, Bun/Rust versions, commit/dirty state, worker-binary checksum, and SHA-256 of the hostname |
| `label` / `node` | Fixture or sample-relative label and selected extractor |
| `input_sha256` | SHA-256 of input bytes; `null` for the baseline |
| `raw_bytes` | On-disk input bytes |
| `declared_bytes` | Sum of declared uncompressed ZIP entry bytes for OOXML; otherwise raw bytes |
| `status` | `success`, `limit`, `error`, or `cancelled` |
| `limit` | Named `IRONFLOW_MAX_*` limit when the failure identified one |
| `wall_seconds` | Subprocess wall time from `/usr/bin/time` |
| `user_cpu_seconds` / `system_cpu_seconds` | Subprocess CPU time |
| `peak_rss_bytes` | Maximum resident set size, normalized to bytes |
| `serialized_output_bytes` | JSON bytes counted with a sink writer, without retaining another serialized copy |
| `persisted_bytes` | Regular artifact bytes created under the worker's isolated artifact root |
| `cancellation_requested_ms` | Worker-relative cancellation request time, when cancellation won |
| `post_cancellation_drain_ms` | Time from dropped async waiter through blocking-runtime drain |
| `concurrency` / `repetition` / `slot` | Matrix coordinates |
| `batch_id` | Identifier shared by concurrently started copies |
| `batch_peak_rss_sum_bytes` | Conservative sum of per-process peak RSS values in the batch; not a synchronized system-RSS sample |

The benchmark stores only relative labels. Absolute source paths, extracted
text, comments, cells, media content, and provider data are excluded.

## Interpretation

- Compare the same input checksum, release profile, machine metadata, limits,
  and concurrency. A changed checksum is a different workload.
- Use medians from at least three repetitions. Treat small differences as
  noise; allocator and filesystem caches make peak RSS and wall time variable.
- Inspect absolute RSS as well as baseline-adjusted RSS. Do not subtract the
  baseline when it would hide a higher absolute peak.
- `batch_peak_rss_sum_bytes` is deliberately conservative because individual
  process peaks may occur at different instants. Use OS/container telemetry for
  exact simultaneous host pressure.
- A `limit` result is a valid bounded rejection, not a successful extraction.
  Compare the named limit and environment before comparing resources.
- A missing cancellation drain value means the extraction completed before the
  timeout. Increase the sample size or reduce `--cancel-after-ms` rather than
  treating it as zero drain time.

CI may validate the worker, parser, fixture checksums, and broad safety
ceilings. It must not fail on narrow wall-time, CPU, or RSS changes. Keep full
trend runs opt-in and retain their JSONL outside the repository unless a result
is deliberately selected as review evidence.
