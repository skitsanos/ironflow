import { expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DEFAULT_MAX_AGE_SECONDS, checkReceipt, receiptPath, writeReceipt } from "../gate_receipt";

// Git exports repository-local paths when invoking hooks; a fixture must never
// inherit them from the repository that runs this test.
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

async function fixture(): Promise<{ root: string; target: string }> {
  const root = await mkdtemp(join(tmpdir(), "ironflow-gate-receipt-"));
  git(root, "init", "-q");
  await writeFile(join(root, "flow.lua"), "return 1\n");
  git(root, "add", "flow.lua");
  git(root, "commit", "-qm", "first");
  return { root, target: join(root, "target-dir") };
}

test("a receipt records the validated tree and survives a message-only amend, not a content change", async () => {
  const { root, target } = await fixture();
  try {
    const receipt = await writeReceipt(root, target);
    expect(receipt.commit).toBe(git(root, "rev-parse", "HEAD"));
    expect(receipt.tree).toBe(git(root, "rev-parse", "HEAD^{tree}"));
    expect(await Bun.file(receiptPath(root, target)).exists()).toBeTrue();

    const same = await checkReceipt(root, git(root, "rev-parse", "HEAD"), target);
    expect(same.accepted).toBeTrue();

    git(root, "commit", "-q", "--amend", "-m", "reworded");
    const reworded = git(root, "rev-parse", "HEAD");
    expect(reworded).not.toBe(receipt.commit);
    const amended = await checkReceipt(root, reworded, target);
    expect(amended.accepted).toBeTrue();

    await writeFile(join(root, "flow.lua"), "return 2\n");
    git(root, "commit", "-qam", "changed");
    const changed = await checkReceipt(root, git(root, "rev-parse", "HEAD"), target);
    expect(changed.accepted).toBeFalse();
    if (!changed.accepted) expect(changed.reason).toContain("the push carries tree");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("a dirty worktree cannot record a receipt", async () => {
  const { root, target } = await fixture();
  try {
    await writeFile(join(root, "flow.lua"), "return 3\n");
    await expect(writeReceipt(root, target)).rejects.toThrow("uncommitted changes");
    expect(await Bun.file(receiptPath(root, target)).exists()).toBeFalse();
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("missing, stale, future-dated and malformed receipts are rejected", async () => {
  const { root, target } = await fixture();
  try {
    const head = git(root, "rev-parse", "HEAD");
    const missing = await checkReceipt(root, head, target);
    expect(missing.accepted).toBeFalse();
    if (!missing.accepted) expect(missing.reason).toContain("no integration gate receipt");

    const completed = new Date("2026-10-29T10:00:00Z");
    await writeReceipt(root, target, completed);
    const fresh = await checkReceipt(root, head, target, new Date("2026-10-29T18:00:00Z"));
    expect(fresh.accepted).toBeTrue();
    if (fresh.accepted) expect(fresh.ageSeconds).toBe(8 * 3600);
    const stale = await checkReceipt(root, head, target, new Date(completed.getTime() + (DEFAULT_MAX_AGE_SECONDS + 1) * 1000));
    expect(stale.accepted).toBeFalse();
    if (!stale.accepted) expect(stale.reason).toContain("old");
    const future = await checkReceipt(root, head, target, new Date("2026-10-29T09:00:00Z"));
    expect(future.accepted).toBeFalse();

    await writeFile(receiptPath(root, target), "{\"tree\": 1}\n");
    const malformed = await checkReceipt(root, head, target);
    expect(malformed.accepted).toBeFalse();
    if (!malformed.accepted) expect(malformed.reason).toContain("unreadable");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("the CLI accepts a matching receipt and fails closed on a malformed age limit", async () => {
  const { root, target } = await fixture();
  try {
    const script = join(import.meta.dir, "../gate_receipt.ts");
    const head = git(root, "rev-parse", "HEAD");
    const run = (args: string[], extra: Record<string, string> = {}) =>
      Bun.spawnSync([process.execPath, "--no-env-file", script, ...args], {
        cwd: root,
        env: { ...environment, CARGO_TARGET_DIR: target, ...extra },
        stdout: "pipe",
        stderr: "pipe",
      });

    const blocked = run(["check", head]);
    expect(blocked.exitCode).toBe(1);
    expect(new TextDecoder().decode(blocked.stderr)).toContain("Run scripts/integration_gate.sh on this commit first");

    const written = run(["write"]);
    expect(written.exitCode, new TextDecoder().decode(written.stderr)).toBe(0);
    const accepted = run(["check", head]);
    expect(accepted.exitCode, new TextDecoder().decode(accepted.stderr)).toBe(0);
    expect(new TextDecoder().decode(accepted.stdout)).toContain("Integration gate receipt accepted");

    const malformed = run(["check", head], { IRONFLOW_GATE_RECEIPT_MAX_AGE_SECONDS: "1h" });
    expect(malformed.exitCode).toBe(1);
    expect(new TextDecoder().decode(malformed.stderr)).toContain("non-negative integer");

    expect(run(["check"]).exitCode).toBe(2);
    expect(run(["write", "extra"]).exitCode).toBe(2);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
