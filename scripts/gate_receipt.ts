#!/usr/bin/env bun
// IF-172: the full integration gate cannot run inside `git push`. GitHub closes
// the push's idle SSH session long before the gate finishes, so a green gate
// ended with a dead connection and no push. The gate therefore records a
// receipt for the exact tree it validated, and the develop pre-push hook
// accepts a fresh receipt whose tree matches the commit being pushed.

import { isAbsolute, join, resolve } from "node:path";

export const RECEIPT_NAME = "ironflow-gate-receipt.json";
export const DEFAULT_MAX_AGE_SECONDS = 24 * 60 * 60;

export type Receipt = { tree: string; commit: string; completed: string };

export type Verdict =
  | { accepted: true; receipt: Receipt; ageSeconds: number }
  | { accepted: false; reason: string };

function git(repository: string, args: string[]): string {
  const result = Bun.spawnSync(["git", ...args], { cwd: repository, stdout: "pipe", stderr: "pipe" });
  if (result.exitCode !== 0) {
    const detail = new TextDecoder().decode(result.stderr).trim();
    throw new Error(detail || `git ${args[0]} failed`);
  }
  return new TextDecoder().decode(result.stdout).trim();
}

/** The receipt lives in the Cargo target directory: ignored by git, kept by
 *  `cargo clean --package`, gone with a full `cargo clean`. */
export function receiptPath(repository: string, targetDirectory?: string): string {
  const target = targetDirectory
    ? isAbsolute(targetDirectory) ? targetDirectory : resolve(repository, targetDirectory)
    : join(repository, "target");
  return join(target, RECEIPT_NAME);
}

export function treeOf(repository: string, commit: string): string {
  if (!/^[0-9a-f]{7,40}$/i.test(commit)) throw new Error("commit must be a hexadecimal object id");
  return git(repository, ["rev-parse", "--verify", `${commit}^{tree}`]);
}

/** Record the tree the gate just validated. A dirty worktree is refused: its
 *  tree hash would describe content the gate did not test. */
export async function writeReceipt(
  repository: string,
  targetDirectory?: string,
  now: Date = new Date(),
): Promise<Receipt> {
  if (git(repository, ["status", "--porcelain"]) !== "") {
    throw new Error("the worktree has uncommitted changes, so no commit describes what the gate validated");
  }
  const commit = git(repository, ["rev-parse", "--verify", "HEAD"]);
  const receipt: Receipt = { tree: treeOf(repository, commit), commit, completed: now.toISOString() };
  await Bun.write(receiptPath(repository, targetDirectory), `${JSON.stringify(receipt, null, 2)}\n`);
  return receipt;
}

/** Accept a receipt only when it is readable, fresh, and validated the same
 *  tree that `commit` carries. Amending a message keeps the tree; any content
 *  change does not. */
export async function checkReceipt(
  repository: string,
  commit: string,
  targetDirectory?: string,
  now: Date = new Date(),
  maxAgeSeconds: number = DEFAULT_MAX_AGE_SECONDS,
): Promise<Verdict> {
  const path = receiptPath(repository, targetDirectory);
  const file = Bun.file(path);
  if (!(await file.exists())) return { accepted: false, reason: `no integration gate receipt at ${path}` };

  let receipt: Receipt;
  try {
    const parsed = JSON.parse(await file.text()) as Partial<Receipt>;
    if (typeof parsed.tree !== "string" || typeof parsed.commit !== "string" || typeof parsed.completed !== "string") {
      throw new Error("missing tree, commit or completed field");
    }
    receipt = { tree: parsed.tree, commit: parsed.commit, completed: parsed.completed };
  } catch (error) {
    return { accepted: false, reason: `unreadable integration gate receipt at ${path}: ${describe(error)}` };
  }

  const completed = Date.parse(receipt.completed);
  if (Number.isNaN(completed)) return { accepted: false, reason: "the integration gate receipt has no valid completion time" };
  const ageSeconds = Math.round((now.getTime() - completed) / 1000);
  if (ageSeconds < 0) return { accepted: false, reason: "the integration gate receipt is dated in the future" };
  if (ageSeconds > maxAgeSeconds) {
    return { accepted: false, reason: `the integration gate receipt is ${ageSeconds}s old; the limit is ${maxAgeSeconds}s` };
  }

  const tree = treeOf(repository, commit);
  if (tree !== receipt.tree) {
    return {
      accepted: false,
      reason: `the integration gate validated tree ${short(receipt.tree)} (commit ${short(receipt.commit)}), but the push carries tree ${short(tree)}`,
    };
  }
  return { accepted: true, receipt, ageSeconds };
}

function short(id: string): string {
  return id.slice(0, 12);
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function maxAgeFromEnvironment(): number {
  const raw = process.env.IRONFLOW_GATE_RECEIPT_MAX_AGE_SECONDS;
  if (raw === undefined) return DEFAULT_MAX_AGE_SECONDS;
  if (!/^\d+$/.test(raw.trim())) {
    throw new Error("IRONFLOW_GATE_RECEIPT_MAX_AGE_SECONDS must be a non-negative integer");
  }
  return Number(raw.trim());
}

async function main(argv: string[]): Promise<number> {
  const repository = git(process.cwd(), ["rev-parse", "--show-toplevel"]);
  const targetDirectory = process.env.CARGO_TARGET_DIR || undefined;
  const [command, commit] = argv;

  if (command === "write" && argv.length === 1) {
    const receipt = await writeReceipt(repository, targetDirectory);
    console.log(`Integration gate receipt recorded for tree ${short(receipt.tree)} (commit ${short(receipt.commit)}).`);
    return 0;
  }
  if (command === "check" && commit && argv.length === 2) {
    const verdict = await checkReceipt(repository, commit, targetDirectory, new Date(), maxAgeFromEnvironment());
    if (verdict.accepted) {
      console.log(
        `Integration gate receipt accepted: tree ${short(verdict.receipt.tree)} passed the full gate `
          + `${verdict.ageSeconds}s ago (commit ${short(verdict.receipt.commit)}).`,
      );
      return 0;
    }
    console.error(`Push blocked: ${verdict.reason}.`);
    console.error(
      "Run scripts/integration_gate.sh on this commit first; a passing run records the receipt. "
        + "For an intermediate push, set IRONFLOW_PREPUSH_GATE=fast.",
    );
    return 1;
  }
  console.error("Usage: bun --no-env-file scripts/gate_receipt.ts write | check <commit>");
  return 2;
}

if (import.meta.main) {
  main(Bun.argv.slice(2)).then(
    (code) => process.exit(code),
    (error) => {
      console.error(`integration gate receipt failed closed: ${describe(error)}`);
      process.exit(1);
    },
  );
}
