// IF-169: every test here protects a behavior or a cross-file property: the
// paths that skip CI, the shipped dependency surface, feature wiring, action
// pins, permission scopes, publication ordering, generated credentials, the
// toolchain pin and the gate's cleanup trap. Tests that only restated workflow
// or shell text were removed; the inventory is in docs/issues/IF-169.md.
import { describe, expect, test } from "bun:test";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const repository = join(import.meta.dir, "../..");

async function read(path: string): Promise<string> {
  return Bun.file(join(repository, path)).text();
}

type Dependency = { version?: string; optional?: boolean; "default-features"?: boolean; features?: string[] };
type Manifest = { dependencies: Record<string, Dependency>; features: Record<string, string[]> };

describe("repository integration policy", () => {
  test("documentation-only paths skip CI and every build or policy input runs it", async () => {
    const workflow = Bun.YAML.parse(await read(".github/workflows/ci.yml")) as {
      on: Record<string, { "paths-ignore": string[] }>;
    };
    for (const event of ["push", "pull_request"]) {
      const ignored = (path: string) =>
        workflow.on[event]["paths-ignore"].some((pattern) => new Bun.Glob(pattern).match(path));
      expect(ignored("README.md"), event).toBeTrue();
      expect(ignored(".claude/settings.json"), event).toBeTrue();
      for (const path of [
        "src/main.rs",
        "Cargo.lock",
        "rust-toolchain.toml",
        "AGENTS.md",
        "ISSUES.md",
        "docs/issues/IF-133.md",
        ".agents/skills/check-ironflow/SKILL.md",
        ".claude/skills/check/SKILL.md",
        ".codex/hooks.json",
        ".githooks/pre-commit",
        "scripts/tests/test_codex_hooks.py",
        ".github/workflows/release.yml",
      ]) {
        expect(ignored(path), `${event}: ${path}`).toBeFalse();
      }
    }
  });

  test("the shipped dependency surface stays bounded, copyleft-free and TLS-verified", async () => {
    const manifest = await read("Cargo.toml");
    const lockfile = await read("Cargo.lock");
    expect(await read(".cargo/audit.toml")).not.toContain('"RUSTSEC-2026-0192"');
    // IF-161: shipped binaries are MIT; the license gate runs in CI, the local
    // gate and the pre-commit hook for manifest changes.
    for (const surface of [".github/workflows/ci.yml", "scripts/integration_gate.sh", ".githooks/pre-commit"]) {
      expect(await read(surface), surface).toContain("bun --no-env-file scripts/check_licenses.ts");
    }
    expect(manifest).not.toMatch(/^html2md\s*=/m);
    for (const removedPackage of ["html2md", "textris-pdf", "bincode", "paste", "pdf-extract", "syntect", "ttf-parser", "yaml-rust"]) {
      expect(lockfile).not.toContain(`name = "${removedPackage}"`);
    }
    expect(lockfile.match(/name = "lopdf"/g) ?? []).toHaveLength(1);

    const { dependencies, features } = Bun.TOML.parse(manifest) as Manifest;
    // IF-164: document dependencies carry only the features the nodes use.
    expect(dependencies.comrak["default-features"]).toBeFalse();
    expect(dependencies.lopdf["default-features"]).toBeFalse();
    expect(dependencies.lopdf.features).toEqual(["chrono"]);
    expect(dependencies.parley["default-features"]).toBeFalse();
    expect(dependencies.parley.features).toEqual(["std", "complex-scripts"]);
    expect(dependencies.krilla["default-features"]).toBeFalse();
    expect(dependencies.krilla.features).toEqual(["raster-images"]);
    // IF-137: shared stores speak TLS through Rustls only; the insecure Redis
    // bypass and native TLS never enter the manifest.
    expect(features.postgres).toContain("sqlx/tls-rustls");
    expect(features.postgres).toContain("dep:rustls");
    expect(features.redis).toContain("redis/tokio-rustls-comp");
    expect(features.redis).toContain("dep:rustls");
    expect(dependencies.rustls.optional).toBeTrue();
    expect(dependencies.rustls.features).toContain("ring");
    expect(manifest).not.toContain("tls-rustls-insecure");
    expect(manifest).not.toContain("tls-native-tls");
  });

  test("optional features stay real and every full variant carries all of them", async () => {
    const { dependencies, features } = Bun.TOML.parse(await read("Cargo.toml")) as Manifest;
    // IF-164: pdf-render gates the Pdfium dependency; cjk-fonts is opt-in.
    expect(dependencies["pdfium-render"].optional).toBeTrue();
    expect(features["pdf-render"]).toEqual(["dep:pdfium-render"]);
    expect(features["cjk-fonts"]).toEqual([]);
    expect(features.default).toEqual(["pdf-render"]);
    // The -full archives, the container image, the feature job and the local
    // gate agree on the optional feature set.
    expect(await read(".github/workflows/release.yml")).toContain(
      "matrix.suffix == '-full' && '--features postgres,redis,cjk-fonts'",
    );
    expect((await read("Dockerfile")).match(/^ARG FEATURES="postgres redis cjk-fonts"$/gm)).toHaveLength(2);
    expect(await read(".github/workflows/ci.yml")).toContain("--features postgres,redis,cjk-fonts");
    expect(await read("scripts/integration_gate.sh")).toContain("--features postgres,redis,cjk-fonts");
  });

  test("third-party actions are pinned to commit SHAs with their version noted", async () => {
    const files = [...new Bun.Glob(".github/workflows/*.yml").scanSync(repository)];
    expect(files.length).toBeGreaterThan(0);
    for (const file of files) {
      const source = await read(file);
      for (const [, reference] of source.matchAll(/uses:\s*([^\s#]+)/g)) {
        if (reference.startsWith("./")) continue;
        if (reference.startsWith("dtolnay/rust-toolchain@")) continue;
        expect(reference, `${file}: ${reference}`).toMatch(/^[^@]+@[0-9a-f]{40}$/);
      }
      for (const line of source.split("\n").filter((line) => /uses:\s*[^.][^\s]*@[0-9a-f]{40}/.test(line))) {
        expect(line, line).toMatch(/# v?\d/);
      }
    }
    const ci = Bun.YAML.parse(await read(".github/workflows/ci.yml")) as { permissions?: Record<string, string> };
    expect(ci.permissions).toEqual({ contents: "read" });
    const renovate = JSON.parse(await read("renovate.json")) as {
      extends: string[];
      packageRules: Array<{ matchPackageNames?: string[]; pinDigests?: boolean }>;
    };
    expect(renovate.extends).toContain("helpers:pinGitHubActionDigests");
    expect(renovate.packageRules.some((rule) => rule.matchPackageNames?.includes("dtolnay/rust-toolchain") && rule.pinDigests === false)).toBeTrue();
  });

  test("only guarded tag releases publish, with write access scoped to the publishing jobs", async () => {
    const workflow = Bun.YAML.parse(await read(".github/workflows/release.yml")) as {
      on: Record<string, unknown>;
      permissions: Record<string, string>;
      jobs: Record<string, {
        uses?: string;
        with?: Record<string, string>;
        secrets?: string;
        needs?: string;
        permissions?: Record<string, string>;
        steps?: Array<{ run?: string }>;
      }>;
    };
    expect(workflow.on).toEqual({ push: { tags: ["v*"] } });
    expect(workflow.permissions).toEqual({ contents: "read" });
    expect(workflow.jobs.build.needs).toBe("guard");
    expect(workflow.jobs.guard.steps?.some((step) => step.run?.includes("git merge-base --is-ancestor"))).toBeTrue();
    expect(workflow.jobs.release.needs).toBe("build");
    expect(workflow.jobs.container.needs).toBe("release");
    expect(workflow.jobs.container.uses).toBe("./.github/workflows/container-release.yml");
    expect(workflow.jobs.container.with?.release_tag).toBe("${{ github.ref_name }}");
    expect(workflow.jobs.container.secrets).toBe("inherit");
    for (const [name, job] of Object.entries(workflow.jobs)) {
      expect(job.permissions ?? workflow.permissions, name).toEqual({
        contents: name === "release" ? "write" : "read",
        ...(name === "container" ? { packages: "write" } : {}),
      });
    }
  });

  test("no CI job or step may ignore its own failure", async () => {
    const workflow = Bun.YAML.parse(await read(".github/workflows/ci.yml")) as {
      jobs: Record<string, { "continue-on-error"?: unknown; steps: Array<{ "continue-on-error"?: unknown }> }>;
    };
    for (const [name, job] of Object.entries(workflow.jobs)) {
      expect(job["continue-on-error"], name).toBeUndefined();
      for (const step of job.steps) {
        expect(step["continue-on-error"], name).toBeUndefined();
      }
    }
  });

  test("integration stores never use a fixed PostgreSQL password", async () => {
    const gate = await read("scripts/integration_gate.sh");
    const ci = await read(".github/workflows/ci.yml");
    expect(gate).not.toContain("POSTGRES_PASSWORD=postgres");
    expect(gate).not.toContain("postgres://postgres:postgres@");
    expect(ci).not.toContain("POSTGRES_PASSWORD: postgres");
    expect(ci).not.toContain("postgres://postgres:postgres@");
    const workflow = Bun.YAML.parse(ci) as {
      jobs: Record<string, {
        env?: Record<string, string>;
        services?: Record<string, { env?: Record<string, string> }>;
      }>;
    };
    const storage = workflow.jobs["rust-features"];
    const password = storage.services?.postgres.env?.POSTGRES_PASSWORD ?? "";
    // The service password is derived per run, never a literal.
    expect(password).toContain("${{");
    expect(storage.env?.DATABASE_URL).toContain(`:${password}@`);
  });

  test("container publication is gated by the store smoke and never moves latest from develop", async () => {
    const workflow = Bun.YAML.parse(await read(".github/workflows/container.yml")) as {
      on: { push: { paths: string[] } };
      jobs: Record<string, {
        steps: Array<{
          id?: string;
          run?: string;
          "continue-on-error"?: boolean;
          with?: { load?: boolean; push?: boolean; tags?: string; flavor?: string };
        }>;
      }>;
    };
    const steps = workflow.jobs.publish.steps;
    const metadata = steps.find((step) => step.id === "metadata")?.with;
    // Never let develop or an arbitrary manually selected ref move latest.
    expect(metadata?.flavor).toBe("latest=false");
    expect(metadata?.tags?.trim().split("\n")).toEqual([
      "type=sha,format=long,prefix=sha-",
      "type=raw,value=latest,enable=${{ github.ref == 'refs/heads/main' }}",
      "type=raw,value=develop,enable=${{ github.ref == 'refs/heads/develop' }}",
    ]);
    // The candidate is loaded locally, smoke-tested, and only then pushed.
    const candidate = steps.findIndex((step) => step.with?.load === true);
    const acceptance = steps.findIndex((step) => step.run?.includes("scripts/test_container_store.ts"));
    const publication = steps.findIndex((step) => step.with?.push === true);
    expect(candidate).toBeGreaterThan(-1);
    expect(acceptance).toBeGreaterThan(candidate);
    expect(publication).toBeGreaterThan(acceptance);
    expect(steps[acceptance]["continue-on-error"]).not.toBe(true);
    expect(steps[candidate].with?.push).not.toBe(true);
    expect(steps[publication].with?.tags).toBe("${{ steps.metadata.outputs.tags }}");
    expect(workflow.on.push.paths).toContain("scripts/test_container_store.ts");
  });

  test("active Rust toolchain pins stay aligned", async () => {
    const toolchain = Bun.TOML.parse(await read("rust-toolchain.toml")) as { toolchain: { channel: string } };
    const version = toolchain.toolchain.channel;
    const ci = await read(".github/workflows/ci.yml");
    const release = await read(".github/workflows/release.yml");
    const dockerfile = await read("Dockerfile");

    expect(version).toMatch(/^\d+\.\d+\.\d+$/);
    for (const source of [ci, release]) {
      const pins = source.match(/dtolnay\/rust-toolchain@\d+\.\d+\.\d+/g) ?? [];
      expect(pins.length).toBeGreaterThan(0);
      expect(new Set(pins)).toEqual(new Set([`dtolnay/rust-toolchain@${version}`]));
    }
    expect(dockerfile).toContain(`FROM rust:${version}-slim-bookworm@sha256:`);
  });

  test("the integration gate package-cleans before work and after failure", async () => {
    const fakeBin = await mkdtemp(join(tmpdir(), "ironflow-gate-policy-"));
    const commandLog = join(fakeBin, "cargo.log");
    const cargo = join(fakeBin, "cargo");

    try {
      await writeFile(cargo, `#!/usr/bin/env bash
printf '%s\\n' "$*" >> "$IRONFLOW_TEST_CARGO_LOG"
if [[ "$1" == "fmt" ]]; then exit 42; fi
`);
      await chmod(cargo, 0o755);

      for (const command of ["cargo-audit", "python3", "bun", "actionlint", "docker", "openssl"]) {
        const path = join(fakeBin, command);
        await writeFile(path, "#!/bin/sh\nexit 0\n");
        await chmod(path, 0o755);
      }

      const process = Bun.spawn(["/bin/bash", "scripts/integration_gate.sh"], {
        cwd: repository,
        env: {
          ...Bun.env,
          PATH: `${fakeBin}:${Bun.env.PATH}`,
          IRONFLOW_TEST_CARGO_LOG: commandLog,
        },
        stdout: "ignore",
        stderr: "ignore",
      });

      expect(await process.exited).toBe(42);
      expect((await Bun.file(commandLog).text()).trim().split("\n")).toEqual([
        "clean --package ironflow",
        "fmt --all -- --check",
        "clean --package ironflow",
      ]);
    } finally {
      await rm(fakeBin, { recursive: true, force: true });
    }
  });
});
