// Behavioral test of `.githooks/pre-push`: the real hook runs against a
// disposable repository with faked `bun` and `cargo`, so each branch of the
// develop gate is exercised without network, Docker or a Rust build.
import { expect, test } from "bun:test";
import { chmod, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const hook = join(import.meta.dir, "../../.githooks/pre-push");
const environment = Object.fromEntries(
  Object.entries(Bun.env).filter(([key]) => !key.startsWith("GIT_")),
) as Record<string, string>;

function git(cwd: string, ...args: string[]): string {
  const result = Bun.spawnSync(
    ["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false", ...args],
    { cwd, env: environment, stdout: "pipe", stderr: "pipe" },
  );
  if (result.exitCode !== 0) throw new Error(new TextDecoder().decode(result.stderr));
  return new TextDecoder().decode(result.stdout).trim();
}

type Outcome = { code: number; stderr: string; trace: string[] };

async function pushDevelop(options: { receipt: number; mode?: string; dirty?: boolean; remoteRef?: string }): Promise<Outcome> {
  // The fake tools and their trace live beside the repository, never inside
  // it: the hook requires a clean worktree.
  const base = await mkdtemp(join(tmpdir(), "ironflow-pre-push-"));
  try {
    const root = join(base, "repo");
    const bin = join(base, "bin");
    const trace = join(base, "trace.log");
    await mkdir(root);
    await mkdir(bin);
    git(root, "init", "-q");
    await writeFile(join(root, "flow.lua"), "return 1\n");
    git(root, "add", "flow.lua");
    git(root, "commit", "-qm", "base");
    const baseCommit = git(root, "rev-parse", "HEAD");
    await writeFile(join(root, "flow.lua"), "return 2\n");
    git(root, "commit", "-qam", "head");
    const head = git(root, "rev-parse", "HEAD");
    if (options.dirty) await writeFile(join(root, "scratch.txt"), "uncommitted\n");

    // The fake tools record every invocation; the receipt check returns the requested status.
    await writeFile(join(bin, "bun"), `#!/bin/sh
printf 'bun %s\\n' "$*" >> "$HOOK_TRACE"
case "$*" in
  *gate_receipt.ts\\ check*) exit "$RECEIPT_STATUS" ;;
esac
exit 0
`);
    await writeFile(join(bin, "cargo"), `#!/bin/sh\nprintf 'cargo %s\\n' "$*" >> "$HOOK_TRACE"\nexit 0\n`);
    for (const file of [join(bin, "bun"), join(bin, "cargo")]) await chmod(file, 0o755);

    const remoteRef = options.remoteRef ?? "refs/heads/develop";
    const child = Bun.spawn(["/bin/bash", hook, "origin", "ssh://example.invalid/ironflow.git"], {
      cwd: root,
      env: {
        ...environment,
        PATH: `${bin}:${environment.PATH}`,
        HOOK_TRACE: trace,
        RECEIPT_STATUS: String(options.receipt),
        ...(options.mode === undefined ? {} : { IRONFLOW_PREPUSH_GATE: options.mode }),
      },
      stdin: new TextEncoder().encode(`refs/heads/develop ${head} ${remoteRef} ${baseCommit}\n`),
      stdout: "pipe",
      stderr: "pipe",
    });
    const [code, stderr] = await Promise.all([child.exited, new Response(child.stderr).text()]);
    const recorded = (await Bun.file(trace).exists())
      ? (await Bun.file(trace).text()).trim().split("\n").filter(Boolean)
      : [];
    return {
      code,
      stderr,
      trace: recorded.map((line) => line.replace(head, "HEAD").replace(baseCommit, "BASE")),
    };
  } finally {
    await rm(base, { recursive: true, force: true });
  }
}

test("a develop push with an accepted gate receipt runs no gate inside the push", async () => {
  const outcome = await pushDevelop({ receipt: 0 });
  expect(outcome.code, outcome.stderr).toBe(0);
  expect(outcome.trace).toEqual([
    "bun run scripts/check_incoming_prs.ts",
    "bun run scripts/development_version.ts check HEAD BASE",
    "bun --no-env-file scripts/gate_receipt.ts check HEAD",
  ]);
});

test("a develop push without a receipt is blocked before any build", async () => {
  const outcome = await pushDevelop({ receipt: 1 });
  expect(outcome.code).not.toBe(0);
  expect(outcome.trace.at(-1)).toBe("bun --no-env-file scripts/gate_receipt.ts check HEAD");
  expect(outcome.trace.some((line) => line.startsWith("cargo") || line.includes("integration_gate"))).toBeFalse();
});

test("the fast mode runs the bounded checks instead of the receipt", async () => {
  const outcome = await pushDevelop({ receipt: 1, mode: "fast" });
  expect(outcome.code, outcome.stderr).toBe(0);
  expect(outcome.trace).toContain("cargo fmt --all -- --check");
  expect(outcome.trace).toContain("cargo clippy --all-targets -- -D warnings");
  expect(outcome.trace).toContain("bun --no-env-file scripts/test_rust_targets.ts --test examples --test cli");
  expect(outcome.trace.some((line) => line.includes("gate_receipt.ts") || line.includes("integration_gate"))).toBeFalse();
});

test("a dirty worktree, an unknown mode and a non-develop ref behave as documented", async () => {
  const dirty = await pushDevelop({ receipt: 0, dirty: true });
  expect(dirty.code).not.toBe(0);
  expect(dirty.stderr).toContain("worktree must be clean");
  expect(dirty.trace).toEqual(["bun run scripts/check_incoming_prs.ts"]);

  const unknown = await pushDevelop({ receipt: 0, mode: "thorough" });
  expect(unknown.code).not.toBe(0);
  expect(unknown.stderr).toContain("must be 'full' or 'fast'");

  const feature = await pushDevelop({ receipt: 1, remoteRef: "refs/heads/feature/anything" });
  expect(feature.code).toBe(0);
  expect(feature.trace).toEqual([]);
});
