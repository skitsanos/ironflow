import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { resolve } from "node:path";

import {
  RESULT_SCHEMA_VERSION,
  nodeForPath,
  parseOptions,
  parseTimeOutput,
} from "../extraction_benchmark";

describe("extraction benchmark", () => {
  test("parses macOS time output into bytes and CPU seconds", () => {
    const metrics = parseTimeOutput(
      "real 0.25\nuser 0.20\nsys 0.03\n  123456 maximum resident set size\n",
      "darwin",
    );
    expect(metrics).toEqual({
      wall_seconds: 0.25,
      user_cpu_seconds: 0.2,
      system_cpu_seconds: 0.03,
      peak_rss_bytes: 123456,
    });
  });

  test("normalizes Linux peak RSS from KiB", () => {
    const metrics = parseTimeOutput(
      "User time (seconds): 1.20\nSystem time (seconds): 0.10\nElapsed (wall clock) time (h:mm:ss or m:ss): 0:01.50\nMaximum resident set size (kbytes): 2048\n",
      "linux",
    );
    expect(metrics.peak_rss_bytes).toBe(2 * 1024 * 1024);
    expect(metrics.wall_seconds).toBe(1.5);
  });

  test("maps only supported extraction inputs", () => {
    expect(nodeForPath("sample.PPTX")).toBe("extract_pptx");
    expect(nodeForPath("sample.txt")).toBeUndefined();
  });

  test("rejects unsupported concurrency", () => {
    expect(() => parseOptions(["--concurrency", "3"], "/repo")).toThrow(
      "selected from 1,2,4",
    );
  });

  test("selects one or more supported extraction nodes", () => {
    expect(parseOptions(["--nodes", "extract_pdf,extract_xlsx"], "/repo").nodes).toEqual([
      "extract_pdf",
      "extract_xlsx",
    ]);
    expect(() => parseOptions(["--nodes", "extract_unknown"], "/repo")).toThrow(
      "unsupported extraction node 'extract_unknown'",
    );
  });

  test("keeps the orchestrator and worker result schema aligned", async () => {
    const worker = await Bun.file(
      resolve(import.meta.dir, "../../tools/extraction_benchmark_worker.rs"),
    ).text();
    expect(RESULT_SCHEMA_VERSION).toBe(2);
    expect(worker).toContain(`const RESULT_SCHEMA_VERSION: u8 = ${RESULT_SCHEMA_VERSION};`);
  });

  test("committed deterministic fixtures match their manifest", async () => {
    const fixtureRoot = resolve(import.meta.dir, "../../benchmarks/extraction/fixtures");
    const manifest = await Bun.file(resolve(fixtureRoot, "manifest.json")).json();
    for (const fixture of manifest.fixtures) {
      const bytes = await Bun.file(resolve(fixtureRoot, fixture.file)).arrayBuffer();
      const checksum = createHash("sha256").update(new Uint8Array(bytes)).digest("hex");
      expect(checksum).toBe(fixture.sha256);
      expect(bytes.byteLength).toBe(fixture.raw_bytes);
    }
  });
});
