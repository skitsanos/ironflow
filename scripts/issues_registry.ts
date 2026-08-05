#!/usr/bin/env bun

import { basename, join } from "node:path";

export type IssueStatus = "open" | "in-progress" | "resolved";

export type Issue = {
  id: string;
  title: string;
  priority: string;
  status: IssueStatus;
  area: string;
  resolved?: string;
  path: string;
  body: string;
};

type YamlMap = Record<string, unknown>;

const repository = join(import.meta.dir, "..");
const issuesDirectory = join(repository, "docs/issues");
const registryPath = join(issuesDirectory, "README.md");
const compatibilityPath = join(repository, "ISSUES.md");
const roadmapPath = join(repository, "docs/ROADMAP.md");
const issuePattern = /^IF-\d{3}$/;
const allowedFields = new Set(["id", "title", "priority", "status", "area", "resolved"]);

function asMap(value: unknown): YamlMap | undefined {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return undefined;
  return value as YamlMap;
}

function stringField(map: YamlMap, key: string): string | undefined {
  const value = map[key];
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

export function parseIssuePage(path: string, source: string): { issue?: Issue; errors: string[] } {
  const errors: string[] = [];
  const match = source.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n([\s\S]+)$/);
  if (!match) return { errors: [`${path}: expected YAML frontmatter and a non-empty body`] };

  let metadata: YamlMap | undefined;
  try {
    metadata = asMap(Bun.YAML.parse(match[1]));
  } catch (error) {
    errors.push(`${path}: invalid YAML frontmatter: ${String(error)}`);
  }
  if (!metadata) return { errors: [...errors, `${path}: frontmatter must be a mapping`] };

  const unexpected = Object.keys(metadata).filter((key) => !allowedFields.has(key));
  if (unexpected.length) errors.push(`${path}: unsupported fields: ${unexpected.join(", ")}`);

  const id = stringField(metadata, "id");
  const title = stringField(metadata, "title");
  const priority = stringField(metadata, "priority");
  const status = stringField(metadata, "status") as IssueStatus | undefined;
  const area = stringField(metadata, "area");
  const resolved = stringField(metadata, "resolved");
  const expectedId = basename(path, ".md");

  if (!id || !issuePattern.test(id)) errors.push(`${path}: id must match IF-NNN`);
  if (id && id !== expectedId) errors.push(`${path}: id ${id} must match filename ${expectedId}`);
  if (!title) errors.push(`${path}: title must be a non-empty string`);
  if (!priority || !/^P[0-3]$/.test(priority)) errors.push(`${path}: priority must be P0, P1, P2, or P3`);
  if (!status || !["open", "in-progress", "resolved"].includes(status)) {
    errors.push(`${path}: status must be open, in-progress, or resolved`);
  }
  if (!area) errors.push(`${path}: area must be a non-empty string`);
  if (status === "resolved" && (!resolved || !/^\d{4}-\d{2}-\d{2}$/.test(resolved))) {
    errors.push(`${path}: resolved issues require an ISO resolved date`);
  }
  if (status !== "resolved" && resolved) errors.push(`${path}: unresolved issues cannot set resolved`);

  const body = match[2].trimEnd() + "\n";
  if (id && title && !body.startsWith(`# ${id} — ${title}\n`)) {
    errors.push(`${path}: body heading must match id and title`);
  }
  const requiredHeading = status === "resolved"
    ? "## Outcome and validation"
    : "## Required outcome and acceptance";
  if (!body.includes(`\n${requiredHeading}\n`)) {
    errors.push(`${path}: body must contain '${requiredHeading}'`);
  }
  if (body.length < 160) errors.push(`${path}: issue record is too short to preserve evidence`);

  if (errors.length || !id || !title || !priority || !status || !area) return { errors };
  return { issue: { id, title, priority, status, area, resolved, path, body }, errors };
}

function displayStatus(status: IssueStatus): string {
  if (status === "in-progress") return "In progress";
  return status[0].toUpperCase() + status.slice(1);
}

function row(issue: Issue, prefix = "."): string {
  return `| [${issue.id}](${prefix}/${issue.id}.md) | ${issue.priority} | ${displayStatus(issue.status)} | ${issue.area} | ${issue.title} |`;
}

export function renderRegistry(issues: Issue[]): string {
  const active = issues.filter((issue) => issue.status !== "resolved").length;
  return `# IronFlow issue registry

This is the canonical registry for IronFlow engineering findings. Each finding
has a stable page whose frontmatter is the source of truth for status,
priority, area, and title. The registry is generated with
\`bun run scripts/issues_registry.ts generate\` and verified with
\`bun run scripts/issues_registry.ts check\`.

- Total findings: ${issues.length}
- Active findings: ${active}
- Historical audit evidence: [AUDIT_EVIDENCE.md](./AUDIT_EVIDENCE.md)

| ID | Priority | Status | Area | Summary |
|---|---:|---|---|---|
${issues.map((issue) => row(issue)).join("\n")}
`;
}

export function renderCompatibilityIndex(issues: Issue[]): string {
  const active = issues.filter((issue) => issue.status !== "resolved");
  const rows = active.length
    ? active.map((issue) => row(issue, "docs/issues")).join("\n")
    : "| — | — | — | — | No active findings |";
  return `# IronFlow engineering issues

The canonical engineering ledger is maintained in
[\`docs/issues/README.md\`](docs/issues/README.md). Individual findings use
stable paths such as [\`docs/issues/IF-001.md\`](docs/issues/IF-001.md).

## Active findings

| ID | Priority | Status | Area | Summary |
|---|---:|---|---|---|
${rows}

## Working agreement

1. Select one issue, or one tightly coupled pair, from the highest-priority
   active group and set its frontmatter status to \`in-progress\`.
2. Confirm the live code still supports the finding, then add focused
   regression coverage for the original defect or missing contract.
3. Align implementation, current documentation, and Lua examples.
4. During ordinary work, run focused tests and the required surface validators.
   Run the complete integration gate only at branch integration, before a
   \`develop\` push, during release preparation, or when explicitly requested.
5. Set an issue to \`resolved\` only after its acceptance criteria pass. Record
   the outcome, contract boundary, exact validation evidence, ISO completion
   date, and commit or PR when applicable.
6. Regenerate the indexes and run \`bun run scripts/issues_registry.ts check\`.

Historical audit baselines and cross-issue evidence are retained in
[\`docs/issues/AUDIT_EVIDENCE.md\`](docs/issues/AUDIT_EVIDENCE.md).
`;
}

function roadmapSection(source: string, heading: string): string | undefined {
  const marker = `## ${heading}\n`;
  const start = source.indexOf(marker);
  if (start === -1) return undefined;
  const bodyStart = start + marker.length;
  const nextHeading = source.indexOf("\n## ", bodyStart);
  return source.slice(bodyStart, nextHeading === -1 ? source.length : nextHeading);
}

export function validateRoadmap(source: string, issues: Issue[]): string[] {
  const errors: string[] = [];
  if (!source.startsWith("# IronFlow product roadmap\n")) {
    errors.push("docs/ROADMAP.md: expected canonical title");
  }

  const sections = new Map<string, string>();
  for (const heading of ["Product posture", "Capability maturity", "Now", "Next", "Later"]) {
    const body = roadmapSection(source, heading);
    if (body === undefined) errors.push(`docs/ROADMAP.md: missing '## ${heading}' section`);
    else sections.set(heading, body);
  }

  const known = new Map(issues.map((issue) => [issue.id, issue]));
  const seen = new Set<string>();
  const entryPattern = /^### \[(IF-\d{3})\]\(issues\/(IF-\d{3})\.md\) — .+$/gm;

  for (const horizon of ["Now", "Next"]) {
    const body = sections.get(horizon);
    if (body === undefined) continue;
    const entries = [...body.matchAll(entryPattern)];
    if (!entries.length && !body.includes("_No committed initiative._")) {
      errors.push(
        `docs/ROADMAP.md: '${horizon}' requires an active IF-NNN entry or explicit empty marker`,
      );
    }
    for (const entry of entries) {
      const [, label, target] = entry;
      if (label !== target) {
        errors.push(`docs/ROADMAP.md: roadmap label ${label} targets ${target}`);
        continue;
      }
      if (seen.has(label)) {
        errors.push(`docs/ROADMAP.md: duplicate committed roadmap entry ${label}`);
        continue;
      }
      seen.add(label);
      const issue = known.get(label);
      if (!issue) errors.push(`docs/ROADMAP.md: roadmap entry ${label} has no issue page`);
      else if (issue.status === "resolved") {
        errors.push(`docs/ROADMAP.md: resolved issue ${label} cannot remain in ${horizon}`);
      }
    }
  }

  return errors;
}

export async function loadIssues(): Promise<{ issues: Issue[]; errors: string[] }> {
  const glob = new Bun.Glob("IF-*.md");
  const paths: string[] = [];
  for await (const path of glob.scan({ cwd: issuesDirectory, onlyFiles: true })) paths.push(path);
  paths.sort();

  const issues: Issue[] = [];
  const errors: string[] = [];
  const ids = new Set<string>();
  for (const relativePath of paths) {
    const path = `docs/issues/${relativePath}`;
    const parsed = parseIssuePage(path, await Bun.file(join(issuesDirectory, relativePath)).text());
    errors.push(...parsed.errors);
    if (!parsed.issue) continue;
    if (ids.has(parsed.issue.id)) errors.push(`${path}: duplicate id ${parsed.issue.id}`);
    ids.add(parsed.issue.id);
    issues.push(parsed.issue);
  }

  issues.sort((left, right) => left.id.localeCompare(right.id));
  if (!issues.length) errors.push("docs/issues: no IF-NNN pages found");
  if (issues.length) {
    const maximum = Number(issues.at(-1)?.id.slice(3));
    for (let index = 1; index <= maximum; index += 1) {
      const id = `IF-${String(index).padStart(3, "0")}`;
      if (!ids.has(id)) errors.push(`docs/issues: missing ${id}`);
    }
  }
  return { issues, errors };
}

async function checkGenerated(path: string, expected: string, errors: string[]): Promise<void> {
  const file = Bun.file(path);
  if (!(await file.exists()) || await file.text() !== expected) {
    errors.push(`${path.replace(repository + "/", "")}: generated content is stale; run the generate command`);
  }
}

async function run(command: string): Promise<void> {
  const { issues, errors } = await loadIssues();
  const registry = renderRegistry(issues);
  const compatibility = renderCompatibilityIndex(issues);

  if (command === "generate") {
    if (errors.length) throw new Error(errors.join("\n"));
    await Bun.write(registryPath, registry);
    await Bun.write(compatibilityPath, compatibility);
    console.log(`Generated issue indexes for ${issues.length} findings.`);
    return;
  }
  if (command !== "check") throw new Error("usage: bun scripts/issues_registry.ts <check|generate>");

  const roadmap = Bun.file(roadmapPath);
  if (!(await roadmap.exists())) errors.push("docs/ROADMAP.md: file is missing");
  else errors.push(...validateRoadmap(await roadmap.text(), issues));
  await checkGenerated(registryPath, registry, errors);
  await checkGenerated(compatibilityPath, compatibility, errors);
  if (errors.length) throw new Error(errors.join("\n"));
  console.log(`Issue registry validation passed: ${issues.length} findings.`);
}

if (import.meta.main) {
  run(Bun.argv[2] ?? "").catch((error) => {
    console.error(`issue registry: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  });
}
