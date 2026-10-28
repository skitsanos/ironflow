// Runs every Cargo test target of the single-package workspace, in two Cargo
// invocations: all parallel-safe targets together, then each single-threaded
// binary with `--test-threads=1`. Feature and harness flags pass through, and
// the linked test executables are removed once their invocation finishes so a
// repeated gate never accumulates them. Library, CLI and dependency artifacts
// stay in place for the next build.
import { lstat, realpath, unlink } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";

export interface Target {
  name: string;
  kind: string[];
  "required-features"?: string[];
}
interface Package {
  id: string;
  name: string;
  manifest_path: string;
  targets: Target[];
  features: Record<string, string[]>;
}
export interface Metadata {
  workspace_root: string;
  target_directory: string;
  packages: Package[];
}
interface Options { cargo: string[]; harness: string[]; features: Set<string> }
export interface Invocation { selection: string[]; harness: string[] }

// Test binaries that mutate process-global state (environment variables) and
// therefore run one test at a time; `tests/serial/main.rs` explains the rule.
export const SINGLE_THREADED = new Set(["serial"]);
const SINGLE_THREAD_FLAG = "--test-threads=1";

export function options(args: string[], definitions: Record<string, string[]>): Options {
  const separator = args.indexOf("--");
  const cargo = separator < 0 ? args : args.slice(0, separator);
  const harness = separator < 0 ? [] : args.slice(separator + 1);
  const pending = cargo.includes("--no-default-features") ? [] : ["default"];
  for (let i = 0; i < cargo.length; i++) {
    if (cargo[i] === "--features") {
      if (!cargo[i + 1] || cargo[i + 1].startsWith("-")) throw new Error("--features needs a value");
      const requested = cargo[++i].split(/[\s,]+/).filter(Boolean);
      if (requested.some(feature => !Object.hasOwn(definitions, feature))) {
        throw new Error("Only explicitly declared local Cargo features are supported");
      }
      pending.push(...requested);
    } else if (cargo[i] === "--all-features") pending.push(...Object.keys(definitions));
    else if (cargo[i] !== "--no-default-features") throw new Error(`Unsupported Cargo option: ${cargo[i]}`);
  }
  const features = new Set<string>();
  while (pending.length) {
    const feature = pending.pop()!;
    if (features.has(feature)) continue;
    features.add(feature);
    pending.push(...(definitions[feature] ?? []).filter(f => !f.includes(":") && !f.includes("/")));
  }
  return { cargo, harness, features };
}

export function selections(targets: Target[], features: Set<string>): string[][] {
  return targets.flatMap(target => {
    if (target["required-features"]?.some(f => !features.has(f))) {
      console.log(`[rust targets] feature-gated: ${target.name}`);
      return [];
    }
    const kind = target.kind[0];
    if (kind === "custom-build") return [];
    if (["lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"].includes(kind)) return [["--lib"]];
    if (["bin", "test", "example", "bench"].includes(kind)) return [[`--${kind}`, target.name]];
    throw new Error(`Unsupported Cargo target kind: ${kind}`);
  });
}

// One invocation for every parallel-safe target, then one per single-threaded
// test binary so its harness always receives `--test-threads=1`.
export function invocations(targets: string[][], harness: string[]): Invocation[] {
  const serial = targets.filter(([kind, name]) => kind === "--test" && SINGLE_THREADED.has(name));
  const parallel = targets.filter(target => !serial.includes(target));
  const plan: Invocation[] = [];
  if (parallel.length) plan.push({ selection: parallel.flat(), harness });
  const singleThreaded = harness.includes(SINGLE_THREAD_FLAG) ? harness : [...harness, SINGLE_THREAD_FLAG];
  for (const target of serial) plan.push({ selection: target, harness: singleThreaded });
  return plan;
}

function inside(root: string, path: string): boolean {
  const suffix = relative(root, path);
  return suffix !== "" && suffix !== ".." && !suffix.startsWith(`..${process.platform === "win32" ? "\\" : "/"}`) && !isAbsolute(suffix);
}

export function executable(event: any, packageId: string, targetDirectory: string): string | null {
  if (event?.reason !== "compiler-artifact" || event.package_id !== packageId ||
      event.profile?.test !== true || typeof event.executable !== "string") return null;
  const path = resolve(event.executable);
  if (!isAbsolute(event.executable) || !inside(resolve(targetDirectory), path)) {
    throw new Error("Cargo reported a test executable outside its target directory");
  }
  return path;
}

export async function reclaim(paths: Set<string>, targetDirectory: string): Promise<void> {
  if (!paths.size) return;
  const root = await realpath(targetDirectory);
  for (const path of paths) {
    if (!inside(root, await realpath(dirname(path)))) throw new Error("Unsafe Cargo artifact parent");
    const file = await lstat(path).catch(error => {
      if (error.code === "ENOENT") return null;
      throw error;
    });
    if (!file) continue;
    if (!file.isFile()) throw new Error("Cargo test executable is not a regular file");
    await unlink(path);
  }
}

export async function runBatch(
  metadata: Metadata,
  packageId: string,
  command: string[],
): Promise<void> {
  const paths = new Set<string>();
  const child = Bun.spawn(command, { cwd: metadata.workspace_root, stdout: "pipe", stderr: "inherit" });
  const interrupt = () => child.kill("SIGINT");
  const terminate = () => child.kill("SIGTERM");
  process.on("SIGINT", interrupt);
  process.on("SIGTERM", terminate);
  let buildFinished = false;
  try {
    // Stream Cargo's structured build events; preserve the test harness output.
    let pending = "";
    const decoder = new TextDecoder();
    const line = (text: string) => {
      let event: any;
      try { event = JSON.parse(text); } catch { /* libtest output is plain text. */ }
      if (!buildFinished && event?.reason) {
        const path = executable(event, packageId, metadata.target_directory);
        if (path) paths.add(path);
        if (event.reason === "compiler-message" && event.message?.rendered) process.stdout.write(event.message.rendered);
        if (event.reason === "build-finished") buildFinished = true;
        if (["compiler-artifact", "compiler-message", "build-finished", "build-script-executed"].includes(event.reason)) return;
      }
      process.stdout.write(`${text}\n`);
    };
    for await (const chunk of child.stdout) {
      pending += decoder.decode(chunk, { stream: true });
      let newline: number;
      while ((newline = pending.indexOf("\n")) >= 0) {
        line(pending.slice(0, newline));
        pending = pending.slice(newline + 1);
      }
    }
    pending += decoder.decode();
    if (pending) line(pending);
    const code = await child.exited;
    if (code !== 0) throw new Error(`Cargo test batch failed (exit ${code})`);
  } finally {
    if (child.exitCode === null) child.kill();
    await child.exited;
    process.off("SIGINT", interrupt);
    process.off("SIGTERM", terminate);
    await reclaim(paths, metadata.target_directory);
  }
}

async function main(args: string[]): Promise<void> {
  const process = Bun.spawn(["cargo", "metadata", "--no-deps", "--format-version", "1"], {
    stdout: "pipe", stderr: "inherit",
  });
  const metadata = await new Response(process.stdout).json() as Metadata;
  if (await process.exited !== 0) throw new Error("Cargo metadata failed");
  const pkg = metadata.packages.find(p => resolve(p.manifest_path) === join(metadata.workspace_root, "Cargo.toml"));
  if (!pkg || metadata.packages.length !== 1) throw new Error("This runner requires a single-package workspace");
  const config = options(args, pkg.features);
  const targets = selections(pkg.targets, config.features);
  if (!targets.length) throw new Error("Cargo metadata selected no test targets");
  for (const invocation of invocations(targets, config.harness)) {
    const count = invocation.selection.filter(arg => arg.startsWith("--")).length;
    console.log(`[rust targets] ${count} target(s): ${invocation.selection.join(" ")}`
      + (invocation.harness.length ? ` -- ${invocation.harness.join(" ")}` : ""));
    await runBatch(metadata, pkg.id, ["cargo", "test", "--no-fail-fast", "--message-format=json", ...config.cargo,
      ...invocation.selection, ...(invocation.harness.length ? ["--", ...invocation.harness] : [])]);
  }
  console.log(`[rust targets] all ${targets.length} targets passed; test executables reclaimed`);
}

if (import.meta.main) {
  main(Bun.argv.slice(2)).catch(error => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
