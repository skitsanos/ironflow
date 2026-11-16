#!/usr/bin/env bun
// Render THIRD_PARTY_LICENSES.md, the attribution file that ships in every
// release archive and in the container image (IF-204). It is generated from
// the same shipped graph that scripts/check_licenses.ts gates, so the two can
// never disagree about which crates are linked into IronFlow. `write` renders
// the file; `check` fails closed when the committed file no longer matches the
// shipped graph. Rows carry no version (IF-208): the file changes when a crate
// is added or removed or a license or source changes, not on a version bump,
// so a lockfile-only dependency update leaves it current. The canonical SPDX
// texts live in scripts/licenses/.

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import {
  ALLOWED_LICENSES,
  LICENSE_EXCEPTIONS,
  loadMetadata,
  parseLicense,
  shippedPackages,
  type LicenseExpr,
  type Metadata,
  type Package,
} from "./check_licenses";

export const NOTICES_FILE = "THIRD_PARTY_LICENSES.md";
export const LICENSE_TEXT_DIR = join(import.meta.dir, "licenses");

/// The allowlist in the order IronFlow picks one option when a crate offers a
/// choice. Every entry must be in ALLOWED_LICENSES and vice versa.
export const LICENSE_PREFERENCE: readonly string[] = [
  "MIT",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Zlib",
  "0BSD",
  "MIT-0",
  "Unlicense",
  "CC0-1.0",
  "BSL-1.0",
  "Unicode-3.0",
  "Unicode-DFS-2016",
  "bzip2-1.0.6",
  "CDLA-Permissive-2.0",
  "MPL-2.0",
  "Apache-2.0 WITH LLVM-exception",
];

export type NoticesOptions = {
  preference?: readonly string[];
  exceptions?: Readonly<Record<string, string>>;
  textDir?: string;
};

type Choice = { ids: string[]; ranks: number[] };

const repository = join(import.meta.dir, "..");

function compareRanks(left: number[], right: number[]): number {
  for (let index = 0; index < Math.min(left.length, right.length); index++) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return left.length - right.length;
}

/// The licenses that apply when IronFlow redistributes a crate: every AND
/// operand, and for each OR the allowed alternative that ranks first. Null
/// when no allowed choice exists.
function choose(expr: LicenseExpr, rank: ReadonlyMap<string, number>): Choice | null {
  if (expr.kind === "id") {
    const position = rank.get(expr.id);
    return position === undefined ? null : { ids: [expr.id], ranks: [position] };
  }
  if (expr.kind === "and") {
    const ids = new Set<string>();
    for (const item of expr.items) {
      const choice = choose(item, rank);
      if (!choice) return null;
      for (const id of choice.ids) ids.add(id);
    }
    const sorted = [...ids].sort((left, right) => rank.get(left)! - rank.get(right)!);
    return { ids: sorted, ranks: sorted.map((id) => rank.get(id)!) };
  }
  let best: Choice | null = null;
  for (const item of expr.items) {
    const choice = choose(item, rank);
    if (choice && (!best || compareRanks(choice.ranks, best.ranks) < 0)) best = choice;
  }
  return best;
}

/// SPDX identifiers IronFlow uses a crate under, in preference order. Throws
/// when the expression cannot be parsed or no allowed option exists.
export function appliedLicenses(expression: string, preference: readonly string[] = LICENSE_PREFERENCE): string[] {
  const rank = new Map(preference.map((id, index) => [id, index]));
  const choice = choose(parseLicense(expression), rank);
  if (!choice) throw new Error(`no allowed license in '${expression}'`);
  return choice.ids;
}

/// The canonical text for one identifier; `A WITH B` is the license text
/// followed by the exception text. Throws when a text is not stored.
export function licenseText(id: string, textDir: string = LICENSE_TEXT_DIR): string {
  return id
    .split(" WITH ")
    .map((part) => {
      const path = join(textDir, `${part}.txt`);
      if (!existsSync(path)) throw new Error(`no canonical license text for ${part}; expected scripts/licenses/${part}.txt`);
      return readFileSync(path, "utf8").replace(/\r\n?/g, "\n").replace(/[ \t]+$/gm, "").trimEnd() + "\n";
    })
    .join("\n");
}

function codepoint(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function cell(value: string): string {
  return value.replace(/\|/g, "\\|");
}

function cratesUrl(pkg: Package): string {
  return `https://crates.io/crates/${pkg.name}`;
}

type Entry = { pkg: Package; applied: string[]; exception?: string; source: string };

/// One row per crate and license terms. Two locked versions of a crate share
/// a row unless their license, exception or source differs.
function distinctRows(list: Entry[]): Entry[] {
  const seen = new Set<string>();
  return list.filter(({ pkg, applied, exception, source }) => {
    const key = JSON.stringify([pkg.name, exception ? `${pkg.version}:${exception}` : pkg.license, applied, source]);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function entries(metadata: Metadata, options: NoticesOptions): Entry[] {
  const preference = options.preference ?? LICENSE_PREFERENCE;
  const exceptions = options.exceptions ?? LICENSE_EXCEPTIONS;
  return shippedPackages(metadata)
    .sort((left, right) => codepoint(left.name, right.name) || codepoint(left.version, right.version))
    .map((pkg) => {
      const source = pkg.repository?.trim() || cratesUrl(pkg);
      const key = `${pkg.name}@${pkg.version}`;
      if (Object.hasOwn(exceptions, key)) return { pkg, applied: [], exception: exceptions[key], source };
      if (!pkg.license) throw new Error(`${pkg.name} ${pkg.version}: no license expression and no reviewed exception`);
      try {
        return { pkg, applied: appliedLicenses(pkg.license, preference), source };
      } catch (error) {
        throw new Error(`${pkg.name} ${pkg.version}: ${error instanceof Error ? error.message : String(error)}`);
      }
    });
}

function header(count: number, preference: readonly string[]): string {
  return `# Third-party licenses

IronFlow is distributed under the MIT License (\`LICENSE\`). Its binaries and
container image statically link the Rust crates listed below, which are
redistributed under their own licenses. The bundled document fonts are covered
separately by the SIL Open Font License (\`assets/fonts/OFL.txt\` in the source
tree, \`OFL.txt\` in the release archives, \`fonts-OFL.txt\` in the image).

This file is generated from \`Cargo.lock\` by
\`bun --no-env-file scripts/third_party_notices.ts write\`; do not edit it by
hand. It covers every crate reachable from IronFlow through normal or build
dependencies with all Cargo features enabled (${count} crates), so it also lists
crates that only the \`-full\` archives and the container image include. Rows
carry no version: the exact versions linked into a binary are those in the
\`Cargo.lock\` of the source revision it was built from. Where a crate offers
a choice of licenses, IronFlow uses the first option in this order:

${wrap(preference.join(", "))}

Each crate's own license file, including its copyright notices, is available
at the listed source.
`;
}

function wrap(text: string, width = 78): string {
  const lines: string[] = [];
  let line = "";
  for (const word of text.split(" ")) {
    if (line && line.length + 1 + word.length > width) {
      lines.push(line);
      line = word;
    } else {
      line = line ? `${line} ${word}` : word;
    }
  }
  if (line) lines.push(line);
  return lines.join("\n");
}

function crateTable(list: Entry[]): string {
  const lines = ["## Crates", "", "| Crate | License | Used under | Source |", "| --- | --- | --- | --- |"];
  for (const { pkg, applied, exception, source } of list) {
    const license = exception ? "—" : cell(pkg.license!);
    const used = exception ? "reviewed exception" : applied.join(", ");
    lines.push(`| ${cell(pkg.name)} | ${license} | ${used} | ${cell(source)} |`);
  }
  return lines.join("\n") + "\n";
}

function mplSection(list: Entry[]): string {
  const covered = list.filter((entry) => entry.applied.includes("MPL-2.0"));
  if (!covered.length) return "";
  const lines = [
    "## MPL-2.0 source availability",
    "",
    "IronFlow links the following crates unmodified. Their complete source code,",
    "for the version recorded in `Cargo.lock`, is available at the listed",
    "repository and from crates.io, as section 3.2 of the Mozilla Public License",
    "2.0 requires for executable distributions.",
    "",
    "| Crate | Source | crates.io |",
    "| --- | --- | --- |",
  ];
  for (const { pkg, source } of covered) {
    lines.push(`| ${cell(pkg.name)} | ${cell(source)} | ${cratesUrl(pkg)} |`);
  }
  return lines.join("\n") + "\n";
}

function exceptionSection(list: Entry[]): string {
  const reviewed = list.filter((entry) => entry.exception);
  if (!reviewed.length) return "";
  const lines = [
    "## Reviewed exceptions",
    "",
    "These crates declare no SPDX expression; the license was reviewed from the",
    "crate's own license file as recorded in `scripts/check_licenses.ts`.",
    "",
  ];
  for (const { pkg, exception } of reviewed) lines.push(`- \`${pkg.name}@${pkg.version}\`: ${exception}`);
  return lines.join("\n") + "\n";
}

function textSection(list: Entry[], preference: readonly string[], textDir: string): string {
  const usage = new Map<string, number>();
  for (const { applied } of list) {
    for (const id of applied) usage.set(id, (usage.get(id) ?? 0) + 1);
  }
  const rank = new Map(preference.map((id, index) => [id, index]));
  const ids = [...usage.keys()].sort((left, right) => rank.get(left)! - rank.get(right)!);
  const lines = ["## License texts", "", 'Each license below applies to the crates whose "Used under" column names it.'];
  for (const id of ids) {
    const count = usage.get(id)!;
    lines.push("", `### ${id}`, "", `Used by ${count} ${count === 1 ? "crate" : "crates"}.`, "", "```text", licenseText(id, textDir).trimEnd(), "```");
  }
  return lines.join("\n") + "\n";
}

/// The complete notices document for the shipped graph. Output depends only on
/// the graph, the preference order and the stored texts, never on input order.
export function renderNotices(metadata: Metadata, options: NoticesOptions = {}): string {
  const preference = options.preference ?? LICENSE_PREFERENCE;
  const list = distinctRows(entries(metadata, options));
  return [
    header(list.length, preference),
    crateTable(list),
    mplSection(list),
    exceptionSection(list),
    textSection(list, preference, options.textDir ?? LICENSE_TEXT_DIR),
  ]
    .filter(Boolean)
    .join("\n");
}

/// Null when the committed file equals the rendered notices, otherwise the
/// actionable message for the gate.
export function compareNotices(rendered: string, committed: string | null): string | null {
  const command = "bun --no-env-file scripts/third_party_notices.ts write";
  if (committed === null) return `${NOTICES_FILE} is missing; run: ${command}`;
  if (committed !== rendered) return `${NOTICES_FILE} does not match the shipped dependency graph; run: ${command}`;
  return null;
}

async function main(): Promise<void> {
  const [command, target] = process.argv.slice(2);
  if (command !== "write" && command !== "check") {
    throw new Error("usage: bun --no-env-file scripts/third_party_notices.ts <write|check> [path]");
  }
  const path = target ?? join(repository, NOTICES_FILE);
  const rendered = renderNotices(loadMetadata(repository));
  const crates = rendered.match(/\((\d+) crates\)/)?.[1] ?? "?";
  const texts = (rendered.match(/^### /gm) ?? []).length;
  if (command === "write") {
    await Bun.write(path, rendered);
    console.log(`Wrote ${NOTICES_FILE}: ${crates} crates, ${texts} license texts, ${Buffer.byteLength(rendered)} bytes.`);
    return;
  }
  const file = Bun.file(path);
  const problem = compareNotices(rendered, (await file.exists()) ? await file.text() : null);
  if (problem) {
    console.error(problem);
    process.exit(1);
  }
  console.log(`Third-party notices are current: ${crates} crates, ${texts} license texts.`);
}

if (import.meta.main) {
  main().catch((error) => {
    console.error(`third-party notices failed closed: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  });
}

// Keep the allowlist and the preference order from drifting apart at import time.
for (const id of ALLOWED_LICENSES) {
  if (!LICENSE_PREFERENCE.includes(id)) throw new Error(`LICENSE_PREFERENCE is missing allowed license ${id}`);
}
