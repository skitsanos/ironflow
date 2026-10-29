#!/usr/bin/env bun
// Validate the canonical repository skills under `.agents/skills` and the
// Claude entries under `.claude/skills`. A Claude entry that names a canonical
// skill is a shim: it must point at an existing skill and stay thin (IF-169).

import { basename, dirname, join } from "node:path";

type YamlMap = Record<string, unknown>;

const repository = join(import.meta.dir, "..");
const canonicalGlob = new Bun.Glob(".agents/skills/*/SKILL.md");
const claudeGlob = new Bun.Glob(".claude/skills/*/SKILL.md");
const canonicalFields = new Set(["name", "description"]);
const claudeFields = new Set([
  "name",
  "description",
  "argument-hint",
  "disable-model-invocation",
  "user-invocable",
  "allowed-tools",
]);
const canonicalReference = /\.agents\/skills\/[a-z0-9-]+\/SKILL\.md/g;
const maxShimBodyLines = 15;
const errors: string[] = [];

function asMap(value: unknown, label: string): YamlMap | undefined {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    errors.push(`${label} must be a YAML mapping`);
    return undefined;
  }
  return value as YamlMap;
}

function nonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function parseYaml(source: string, label: string): YamlMap | undefined {
  try {
    return asMap(Bun.YAML.parse(source), label);
  } catch (error) {
    errors.push(`${label} is invalid YAML: ${String(error)}`);
    return undefined;
  }
}

function validateSkill(
  path: string,
  source: string,
  allowed: Set<string>,
): { name: string; body: string } | undefined {
  const match = source.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n([\s\S]+)$/);
  if (!match) {
    errors.push(`${path}: expected YAML frontmatter and a non-empty body`);
    return undefined;
  }

  const frontmatter = parseYaml(match[1], `${path} frontmatter`);
  if (!frontmatter) return undefined;

  const unexpected = Object.keys(frontmatter).filter((key) => !allowed.has(key));
  if (unexpected.length > 0) {
    errors.push(`${path}: unsupported frontmatter fields: ${unexpected.join(", ")}`);
  }

  const expectedName = basename(dirname(path));
  const name = frontmatter.name;
  if (!nonEmptyString(name) || !/^[a-z0-9-]{1,64}$/.test(name)) {
    errors.push(`${path}: name must use 1-64 lowercase letters, digits, or hyphens`);
    return undefined;
  }
  if (name !== expectedName) {
    errors.push(`${path}: name '${name}' must match directory '${expectedName}'`);
  }
  if (!nonEmptyString(frontmatter.description)) {
    errors.push(`${path}: description must be a non-empty string`);
  }
  if (source.includes("[TODO") || source.includes("TODO:")) {
    errors.push(`${path}: unresolved scaffold TODO`);
  }
  return { name, body: match[2] };
}

async function validateInterface(skillPath: string, name: string): Promise<void> {
  const metadataPath = join(dirname(skillPath), "agents", "openai.yaml");
  const file = Bun.file(join(repository, metadataPath));
  if (!(await file.exists())) return;

  const metadata = parseYaml(await file.text(), metadataPath);
  if (!metadata) return;
  const interfaceMap = asMap(metadata.interface, `${metadataPath} interface`);
  if (!interfaceMap) return;

  if (!nonEmptyString(interfaceMap.display_name)) {
    errors.push(`${metadataPath}: interface.display_name must be a non-empty string`);
  }
  const summary = interfaceMap.short_description;
  if (!nonEmptyString(summary) || summary.length < 25 || summary.length > 64) {
    errors.push(`${metadataPath}: short_description must contain 25-64 characters`);
  }
  const prompt = interfaceMap.default_prompt;
  if (!nonEmptyString(prompt) || !prompt.includes(`$${name}`)) {
    errors.push(`${metadataPath}: default_prompt must mention $${name}`);
  }
}

/// A Claude entry that names a canonical skill defers to it: the reference
/// must resolve, and the body must not grow back into a second copy.
async function validateShim(path: string, body: string): Promise<boolean> {
  const references = new Set(body.match(canonicalReference) ?? []);
  if (references.size === 0) return false;
  for (const reference of references) {
    if (!(await Bun.file(join(repository, reference)).exists())) {
      errors.push(`${path}: references missing canonical skill ${reference}`);
    }
  }
  const lines = body.split(/\r?\n/).filter((line) => line.trim().length > 0);
  if (lines.length > maxShimBodyLines) {
    errors.push(
      `${path}: a shim must stay within ${maxShimBodyLines} non-empty lines (found ${lines.length})`,
    );
  }
  if (/^#{1,6}\s/m.test(body)) {
    errors.push(`${path}: a shim must not restate canonical sections under headings`);
  }
  return true;
}

async function collect(glob: Bun.Glob): Promise<string[]> {
  const paths: string[] = [];
  for await (const path of glob.scan({ cwd: repository, dot: true, onlyFiles: true })) {
    paths.push(path);
  }
  return paths.sort();
}

const canonical = await collect(canonicalGlob);
for (const path of canonical) {
  const skill = validateSkill(path, await Bun.file(join(repository, path)).text(), canonicalFields);
  if (skill) await validateInterface(path, skill.name);
}
if (canonical.length === 0) errors.push("no repository skills found under .agents/skills");

let shims = 0;
let standalone = 0;
for (const path of await collect(claudeGlob)) {
  const skill = validateSkill(path, await Bun.file(join(repository, path)).text(), claudeFields);
  if (!skill) continue;
  if (await validateShim(path, skill.body)) shims += 1;
  else standalone += 1;
}

if (errors.length > 0) {
  for (const error of errors) console.error(`skill validation: ${error}`);
  process.exit(1);
}

console.log(
  `Skill validation passed: ${canonical.length} repository skills, ${shims} Claude shims, `
    + `${standalone} Claude-only skills.`,
);
