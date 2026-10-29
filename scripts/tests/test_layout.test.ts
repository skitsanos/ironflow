// IF-162: integration tests are declared as a bounded set of area binaries.
// With Cargo test autodiscovery off, a `tests/test_*.rs` file that no root
// includes would never compile or run, so this guard fails on any file that is
// not included by exactly one binary.
import { expect, test } from "bun:test";
import { readdir } from "node:fs/promises";
import { join } from "node:path";
import { SINGLE_THREADED } from "../test_rust_targets";

const repository = join(import.meta.dir, "../..");

type TestTarget = { name: string; path: string; "required-features"?: string[] };

async function layout() {
  const parsed = (await Bun.TOML.parse(await Bun.file(join(repository, "Cargo.toml")).text())) as {
    package: { autotests?: boolean };
    test?: TestTarget[];
  };
  const targets = parsed.test ?? [];
  const files = (await readdir(join(repository, "tests")))
    .filter((name) => /^test_.*\.rs$/.test(name))
    .sort();
  const owners = new Map<string, string[]>();
  const claim = (file: string, owner: string) => owners.set(file, [...(owners.get(file) ?? []), owner]);
  const roots = new Map<string, string>();
  for (const target of targets) {
    const match = target.path.match(/^tests\/([a-z_]+)\/main\.rs$/);
    if (match) {
      const source = await Bun.file(join(repository, target.path)).text();
      roots.set(target.name, source);
      for (const [, file] of source.matchAll(/^#\[path = "\.\.\/(test_[a-z0-9_]+\.rs)"\]$/gm)) {
        claim(file, target.name);
      }
    } else {
      const file = target.path.match(/^tests\/(test_[a-z0-9_]+\.rs)$/)?.[1];
      expect(file, `unexpected test path ${target.path}`).toBeDefined();
      claim(file!, target.name);
    }
  }
  return { parsed, targets, files, owners, roots };
}

test("test autodiscovery is off and every test file belongs to exactly one binary", async () => {
  const { parsed, targets, files, owners } = await layout();
  expect(parsed.package.autotests).toBe(false);
  expect(targets.length).toBeGreaterThan(0);
  // Twelve area binaries plus the documented process-isolated files; growth
  // here means a new file took its own executable instead of joining an area.
  expect(targets.length).toBeLessThanOrEqual(24);
  for (const file of files) {
    expect(owners.get(file) ?? [], `${file} is not included by any test binary`).toHaveLength(1);
  }
  for (const [file, binaries] of owners) {
    expect(files, `${file} is declared but missing`).toContain(file);
    expect(binaries, `${file} is included more than once`).toHaveLength(1);
  }
  for (const target of targets) {
    expect(await Bun.file(join(repository, target.path)).exists(), target.path).toBeTrue();
  }
});

test("feature-gated test files live in binaries that require the feature", async () => {
  const { parsed, targets, files, owners } = await layout();
  const required = new Map(targets.map((t) => [t.name, new Set(t["required-features"] ?? [])]));
  // A file gated on a default feature compiles in every default build and may
  // share an area binary; only non-default features need a dedicated target.
  const defaults = new Set((parsed as { features?: { default?: string[] } }).features?.default ?? []);
  for (const file of files) {
    const source = await Bun.file(join(repository, "tests", file)).text();
    for (const [, feature] of source.matchAll(/^#!\[cfg\(feature = "([a-z-]+)"\)\]$/gm)) {
      if (defaults.has(feature)) continue;
      const [owner] = owners.get(file)!;
      expect(required.get(owner)?.has(feature), `${file} needs feature ${feature} but ${owner} does not require it`).toBeTrue();
    }
  }
});

test("environment-mutating test files run single-threaded, in `serial` or alone", async () => {
  const { targets, files, owners, roots } = await layout();
  expect(SINGLE_THREADED.has("serial")).toBeTrue();
  expect(roots.get("serial")).toContain("fn single_threaded_harness()");
  const singleFile = new Set(targets.filter((t) => /^tests\/test_/.test(t.path)).map((t) => t.name));
  for (const file of files) {
    const source = await Bun.file(join(repository, "tests", file)).text();
    if (!/\b(set_var|remove_var|set_current_dir)\(/.test(source)) continue;
    const [owner] = owners.get(file)!;
    expect(
      owner === "serial" || singleFile.has(owner),
      `${file} mutates process state but is in the parallel binary ${owner}`,
    ).toBeTrue();
  }
});

test("the runner always hands `serial` the single-thread flag", async () => {
  const { invocations } = await import("../test_rust_targets");
  const targets = [["--lib"], ["--test", "api"], ["--test", "serial"], ["--bin", "ironflow"]];
  expect(invocations(targets, [])).toEqual([
    { selection: ["--lib", "--test", "api", "--bin", "ironflow"], harness: [] },
    { selection: ["--test", "serial"], harness: ["--test-threads=1"] },
  ]);
  expect(invocations(targets, ["--test-threads=1"])).toEqual([
    { selection: ["--lib", "--test", "api", "--bin", "ironflow"], harness: ["--test-threads=1"] },
    { selection: ["--test", "serial"], harness: ["--test-threads=1"] },
  ]);
  expect(invocations([["--test", "serial"]], ["--nocapture"])).toEqual([
    { selection: ["--test", "serial"], harness: ["--nocapture", "--test-threads=1"] },
  ]);
});
