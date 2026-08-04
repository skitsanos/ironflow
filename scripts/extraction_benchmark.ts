#!/usr/bin/env bun

import { createHash } from "node:crypto";
import { hostname, platform, arch, cpus, totalmem } from "node:os";
import { mkdir, readdir, rm, stat } from "node:fs/promises";
import { basename, dirname, extname, join, relative, resolve } from "node:path";

export interface TimeMetrics {
  wall_seconds: number;
  user_cpu_seconds: number;
  system_cpu_seconds: number;
  peak_rss_bytes: number;
}

export const RESULT_SCHEMA_VERSION = 2;

interface Options {
  samples?: string;
  fixtures: string;
  output: string;
  repetitions: number;
  concurrency: number[];
  nodes: string[];
  cancelAfterMs: number;
  skipBuild: boolean;
}

interface Source {
  label: string;
  node: string;
  input?: string;
  cancelAfterMs?: number;
}

const nodeByExtension: Record<string, string> = {
  ".docx": "extract_word",
  ".pptx": "extract_pptx",
  ".pdf": "extract_pdf",
  ".html": "extract_html",
  ".htm": "extract_html",
  ".srt": "extract_srt",
  ".vtt": "extract_vtt",
  ".xlsx": "extract_xlsx",
};

export function parseTimeOutput(value: string, os: "darwin" | "linux"): TimeMetrics {
  const number = (pattern: RegExp, label: string): number => {
    const match = value.match(pattern);
    if (!match) throw new Error(`time output is missing ${label}`);
    return Number(match[1]);
  };
  if (os === "darwin") {
    return {
      wall_seconds: number(/^\s*real\s+([0-9.]+)$/m, "wall time"),
      user_cpu_seconds: number(/^\s*user\s+([0-9.]+)$/m, "user CPU"),
      system_cpu_seconds: number(/^\s*sys\s+([0-9.]+)$/m, "system CPU"),
      peak_rss_bytes: number(/^\s*(\d+)\s+maximum resident set size$/m, "peak RSS"),
    };
  }
  return {
    wall_seconds: parseElapsed(
      value.match(/^\s*Elapsed \(wall clock\) time.*:\s*([0-9]+(?::[0-9.]+){1,2})$/m)?.[1],
    ),
    user_cpu_seconds: number(/^\s*User time \(seconds\):\s*([0-9.]+)$/m, "user CPU"),
    system_cpu_seconds: number(/^\s*System time \(seconds\):\s*([0-9.]+)$/m, "system CPU"),
    peak_rss_bytes:
      number(/^\s*Maximum resident set size \(kbytes\):\s*(\d+)$/m, "peak RSS") * 1024,
  };
}

function parseElapsed(value?: string): number {
  if (!value) throw new Error("time output is missing wall time");
  const parts = value.split(":").map(Number);
  if (parts.some(Number.isNaN)) throw new Error(`invalid elapsed time '${value}'`);
  if (parts.length === 2) return parts[0] * 60 + parts[1];
  if (parts.length === 3) return parts[0] * 3600 + parts[1] * 60 + parts[2];
  throw new Error(`invalid elapsed time '${value}'`);
}

export function nodeForPath(path: string): string | undefined {
  return nodeByExtension[extname(path).toLowerCase()];
}

export function parseOptions(args: string[], root = process.cwd()): Options {
  const values = new Map<string, string>();
  let skipBuild = false;
  for (let index = 0; index < args.length; index++) {
    const argument = args[index];
    if (argument === "--skip-build") {
      skipBuild = true;
      continue;
    }
    if (!argument.startsWith("--")) throw new Error(`unexpected argument '${argument}'`);
    const value = args[++index];
    if (!value || value.startsWith("--")) throw new Error(`${argument} requires a value`);
    values.set(argument, value);
  }
  const repetitions = positiveInteger(values.get("--repetitions") ?? "1", "repetitions");
  const concurrency = (values.get("--concurrency") ?? "1,2,4")
    .split(",")
    .map((value) => positiveInteger(value, "concurrency"));
  if (concurrency.some((value) => ![1, 2, 4].includes(value))) {
    throw new Error("concurrency values must be selected from 1,2,4");
  }
  const nodes = (values.get("--nodes") ?? "")
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);
  const supportedNodes = new Set(Object.values(nodeByExtension));
  const unsupportedNode = nodes.find((node) => !supportedNodes.has(node));
  if (unsupportedNode) throw new Error(`unsupported extraction node '${unsupportedNode}'`);
  const timestamp = new Date().toISOString().replaceAll(":", "-");
  return {
    samples: values.has("--samples") ? resolve(root, values.get("--samples")!) : undefined,
    fixtures: resolve(root, values.get("--fixtures") ?? "benchmarks/extraction/fixtures"),
    output: resolve(
      root,
      values.get("--output") ?? `benchmarks/results/extraction-${timestamp}.jsonl`,
    ),
    repetitions,
    concurrency: [...new Set(concurrency)],
    nodes: [...new Set(nodes)],
    cancelAfterMs: positiveInteger(values.get("--cancel-after-ms") ?? "2", "cancel-after-ms"),
    skipBuild,
  };
}

function positiveInteger(value: string, label: string): number {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${label} must be a positive integer`);
  }
  return parsed;
}

async function command(command: string[], cwd: string): Promise<string> {
  const child = Bun.spawn(command, { cwd, stdout: "pipe", stderr: "pipe" });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (exitCode !== 0) {
    throw new Error(`${command.join(" ")} failed (${exitCode}): ${stderr.trim()}`);
  }
  return stdout.trim();
}

async function collectFiles(directory: string): Promise<string[]> {
  const files: string[] = [];
  const pending = [directory];
  while (pending.length > 0) {
    const current = pending.pop()!;
    for (const entry of await readdir(current, { withFileTypes: true })) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) pending.push(path);
      else if (entry.isFile() && nodeForPath(path)) files.push(path);
    }
  }
  return files.sort();
}

async function hashFile(path: string): Promise<string> {
  const hash = createHash("sha256");
  for await (const chunk of Bun.file(path).stream()) hash.update(chunk);
  return hash.digest("hex");
}

async function machineMetadata(root: string, worker: string) {
  const rustc = await command(["rustc", "-Vv"], root);
  const commit = await command(["git", "rev-parse", "HEAD"], root);
  const status = await command(["git", "status", "--porcelain"], root);
  return {
    os: platform(),
    arch: arch(),
    cpu_model: cpus()[0]?.model ?? "unknown",
    logical_cpu_count: cpus().length,
    total_memory_bytes: totalmem(),
    hostname_sha256: createHash("sha256").update(hostname()).digest("hex"),
    bun_version: Bun.version,
    rustc: rustc.split("\n")[0],
    git_commit: commit,
    git_dirty: status.length > 0,
    benchmark_worker_sha256: await hashFile(worker),
  };
}

async function runMeasured(
  worker: string,
  source: Source,
  artifactDir: string,
): Promise<Record<string, unknown>> {
  const os = platform();
  if (os !== "darwin" && os !== "linux") {
    throw new Error("resource measurement currently supports macOS and Linux");
  }
  const timeArgs = os === "darwin" ? ["-lp"] : ["-v"];
  const workerArgs = [worker, "run", "--node", source.node, "--label", source.label];
  if (source.input) workerArgs.push("--input", source.input);
  if (source.cancelAfterMs) {
    workerArgs.push("--cancel-after-ms", String(source.cancelAfterMs));
  }
  const child = Bun.spawn(["/usr/bin/time", ...timeArgs, ...workerArgs], {
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, IRONFLOW_ARTIFACT_DIR: artifactDir, RUST_LOG: "off" },
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (exitCode !== 0) {
    throw new Error(`benchmark worker failed (${exitCode}): ${stderr.trim()}`);
  }
  return { ...JSON.parse(stdout.trim()), ...parseTimeOutput(stderr, os) };
}

async function main() {
  const root = resolve(import.meta.dir, "..");
  const options = parseOptions(Bun.argv.slice(2), root);
  const worker = join(root, "target/release/examples/extraction_benchmark_worker");
  if (!options.skipBuild) {
    await command(["cargo", "build", "--release", "--example", "extraction_benchmark_worker"], root);
  } else if (!(await stat(worker).catch(() => undefined))) {
    throw new Error(`release worker does not exist: ${worker}`);
  }
  if (!(await stat(options.fixtures).catch(() => undefined))) {
    throw new Error(`fixture directory does not exist: ${options.fixtures}`);
  }
  const selected = (input: string): boolean => {
    const node = nodeForPath(input)!;
    return options.nodes.length === 0 || options.nodes.includes(node);
  };
  const sources: Source[] = (await collectFiles(options.fixtures))
    .filter(selected)
    .map((input) => ({
      input,
      node: nodeForPath(input)!,
      label: `fixture/${basename(input)}`,
    }));
  if (options.samples) {
    for (const input of (await collectFiles(options.samples)).filter(selected)) {
      sources.push({
        input,
        node: nodeForPath(input)!,
        label: `sample/${relative(options.samples, input)}`,
      });
    }
  }
  const cancellation = sources.find((source) => source.label.endsWith("pathological.html"));
  if (cancellation) {
    sources.push({
      ...cancellation,
      label: "cancellation/pathological.html",
      cancelAfterMs: options.cancelAfterMs,
    });
  }
  sources.unshift({ label: "baseline/empty", node: "baseline" });

  await mkdir(dirname(options.output), { recursive: true });
  const runRoot = `${options.output}.work`;
  await mkdir(runRoot, { recursive: true });
  const output = Bun.file(options.output).writer();
  const machine = await machineMetadata(root, worker);
  let batchNumber = 0;
  try {
    for (const concurrency of options.concurrency) {
      for (let repetition = 1; repetition <= options.repetitions; repetition++) {
        for (const source of sources) {
          const batchId = `${String(batchNumber++).padStart(5, "0")}-${concurrency}-${repetition}`;
          const records = await Promise.all(
            Array.from({ length: concurrency }, (_, slot) =>
              runMeasured(worker, source, join(runRoot, batchId, String(slot))),
            ),
          );
          const batchPeak = records.reduce(
            (sum, record) => sum + Number(record.peak_rss_bytes),
            0,
          );
          for (let slot = 0; slot < records.length; slot++) {
            output.write(
              `${JSON.stringify({
                schema_version: RESULT_SCHEMA_VERSION,
                measured_at: new Date().toISOString(),
                machine,
                repetition,
                concurrency,
                batch_id: batchId,
                slot,
                batch_peak_rss_sum_bytes: batchPeak,
                ...records[slot],
              })}\n`,
            );
          }
        }
      }
    }
  } finally {
    await output.end();
    await rm(runRoot, { recursive: true, force: true });
  }
  console.log(`wrote ${options.output}`);
}

if (import.meta.main) {
  await main();
}
