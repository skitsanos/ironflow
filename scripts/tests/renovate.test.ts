import { expect, test } from "bun:test";
import { join } from "node:path";

type Rule = {
  description?: string;
  matchManagers?: string[];
  matchDatasources?: string[];
  matchPackageNames?: string[];
  groupName?: string;
  allowedVersions?: string;
};
type Dependency = { manager: string; datasource: string; name: string };

const repository = join(import.meta.dir, "../..");
const cargo = (name: string): Dependency => ({ manager: "cargo", datasource: "crate", name });
// crate, last good release, first release broken by smithy-lang/smithy-rs#4853, its next patch
const deferred = [
  ["aws-sdk-s3", "1.146.1", "1.147.0", "1.147.1"],
  ["aws-sdk-s3vectors", "1.36.0", "1.37.0", "1.37.1"],
  ["aws-smithy-types", "1.6.3", "1.7.0", "1.7.1"],
] as const;

async function packageRules(): Promise<Rule[]> {
  const config = await Bun.file(join(repository, "renovate.json")).json();
  return config.packageRules as Rule[];
}

// Renovate string pattern matching: "/re/" or "!/re/" is a regex, anything else a glob.
function matchesName(pattern: string, name: string): boolean {
  const regex = pattern.match(/^(!?)\/(.*)\/(i?)$/);
  if (regex) return new RegExp(regex[2], regex[3]).test(name) !== (regex[1] === "!");
  return new Bun.Glob(pattern).match(name);
}
function applies(rule: Rule, dependency: Dependency): boolean {
  return (rule.matchManagers?.includes(dependency.manager) ?? true)
    && (rule.matchDatasources?.includes(dependency.datasource) ?? true)
    && (rule.matchPackageNames?.some((pattern) => matchesName(pattern, dependency.name)) ?? true);
}
// Renovate merges every matching packageRule in order, later rules winning.
function resolve(rules: Rule[], dependency: Dependency): Rule {
  return Object.assign({}, ...rules.filter((rule) => applies(rule, dependency)));
}
// Only the exact-exclusion form "!/^X\.Y\.Z$/" is modelled; the deferral
// tests require it so that repaired later releases stay eligible.
function admits(exclusion: string, version: string): boolean {
  const pattern = exclusion.match(/^!\/(.*)\/$/);
  if (!pattern) throw new Error(`Unsupported allowedVersions exclusion: ${exclusion}`);
  return !new RegExp(pattern[1]).test(version);
}

test("every AWS crate updates inside one lockstep group", async () => {
  const rules = await packageRules();
  const manifest = Bun.TOML.parse(await Bun.file(join(repository, "Cargo.toml")).text()) as {
    dependencies: Record<string, unknown>;
  };
  const crates = Object.keys(manifest.dependencies).filter((name) => name.startsWith("aws-"));
  expect(crates.length).toBeGreaterThanOrEqual(deferred.length);
  expect(new Set(crates.map((name) => resolve(rules, cargo(name)).groupName))).toEqual(new Set(["AWS SDK"]));
  const grouping = rules.filter((rule) => rule.groupName && crates.some((name) => applies(rule, cargo(name))));
  expect(grouping).toHaveLength(1);
  expect(grouping[0].matchManagers).toEqual(["cargo"]);
  expect(resolve(rules, cargo("tokio")).groupName).toBeUndefined();
  expect(resolve(rules, { manager: "custom.regex", datasource: "rust-version", name: "rust" }).groupName).toBe("Rust toolchain");
});

test("AWS deferrals exclude only the known-broken releases and keep later ones eligible", async () => {
  const rules = await packageRules();
  const lock = Bun.TOML.parse(await Bun.file(join(repository, "Cargo.lock")).text()) as {
    package: { name: string; version: string }[];
  };
  for (const [name, lastGood, broken, nextPatch] of deferred) {
    const { allowedVersions, description } = resolve(rules, cargo(name));
    expect(allowedVersions).toBe(`!/^${broken.replaceAll(".", "\\.")}$/`);
    expect(admits(allowedVersions!, broken)).toBeFalse();
    expect(admits(allowedVersions!, lastGood)).toBeTrue();
    expect(admits(allowedVersions!, nextPatch)).toBeTrue();
    const locked = lock.package.find((entry) => entry.name === name)?.version;
    expect(locked).toBeDefined();
    expect(admits(allowedVersions!, locked!)).toBeTrue();
    expect(description).toContain("smithy-lang/smithy-rs/issues/4853");
    expect(description).toContain("Later releases remain eligible");
  }
  expect(resolve(rules, cargo("aws-config")).allowedVersions).toBeUndefined();
});

test("deferrals are exact exclusions, never version ceilings", async () => {
  const rules = await packageRules();
  const exclusions = rules.filter((rule) => rule.allowedVersions !== undefined);
  expect(exclusions).toHaveLength(deferred.length);
  for (const rule of exclusions) {
    expect(rule.matchManagers).toEqual(["cargo"]);
    expect(rule.matchPackageNames).toHaveLength(1);
    expect(rule.allowedVersions).toMatch(/^!\/\^\d+\\\.\d+\\\.\d+\$\/$/);
  }
});
