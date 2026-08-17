import { describe, expect, test } from "bun:test";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const repository = join(import.meta.dir, "../..");

describe("repository integration policy", () => {
  test("CI runs automatically only for pushes to main and develop", async () => {
    const source = await Bun.file(join(repository, ".github/workflows/ci.yml")).text();
    const workflow = Bun.YAML.parse(source) as Record<string, unknown>;
    const triggers = workflow.on as Record<string, unknown>;
    const push = triggers.push as Record<string, unknown>;
    expect(push.branches).toEqual(["main", "develop"]);
    expect(triggers.pull_request).toBeUndefined();
  });

  test("issue registry changes run the Bun policy gate", async () => {
    const source = await Bun.file(join(repository, ".github/workflows/ci.yml")).text();
    const workflow = Bun.YAML.parse(source) as {
      on: { push: { paths: string[] } };
      jobs: Record<string, { steps: Array<{ uses?: string; run?: string }> }>;
    };
    for (const path of ["docs/**", ".agents/**", "AGENTS.md", "ISSUES.md"]) {
      expect(workflow.on.push.paths).toContain(path);
    }

    const commands = workflow.jobs["repository-policy"].steps
      .map((step) => step.run ?? "")
      .join("\n");
    expect(
      workflow.jobs["repository-policy"].steps.some(
        (step) => step.uses === "oven-sh/setup-bun@v2",
      ),
    ).toBeTrue();
    expect(commands).toContain("bun run scripts/validate_skills.ts");
    expect(commands).toContain("bun run scripts/issues_registry.ts check");
    expect(commands).toContain("bun test scripts/tests/*.test.ts");

    const gate = await Bun.file(join(repository, "scripts/integration_gate.sh")).text();
    expect(gate).toContain("bun run scripts/issues_registry.ts check");
  });

  test("dependency warnings fail closed and removed dependencies stay absent", async () => {
    const manifest = await Bun.file(join(repository, "Cargo.toml")).text();
    const lockfile = await Bun.file(join(repository, "Cargo.lock")).text();
    const auditConfig = await Bun.file(join(repository, ".cargo/audit.toml")).text();
    const workflow = await Bun.file(join(repository, ".github/workflows/ci.yml")).text();
    const gate = await Bun.file(join(repository, "scripts/integration_gate.sh")).text();

    expect(workflow).toContain("cargo audit --deny warnings");
    expect(gate).toContain("cargo audit --deny warnings");
    expect(auditConfig).not.toContain('"RUSTSEC-2026-0192"');
    expect(manifest).toContain('comrak = { version = "0.54", default-features = false }');
    expect(manifest).toContain('lopdf = { version = "0.44", default-features = false, features = ["chrono"] }');
    for (const removedPackage of [
      "bincode",
      "paste",
      "pdf-extract",
      "syntect",
      "ttf-parser",
      "yaml-rust",
    ]) {
      expect(lockfile).not.toContain(`name = "${removedPackage}"`);
    }
    expect(lockfile.match(/name = "lopdf"/g) ?? []).toHaveLength(1);
    expect(lockfile).toContain('name = "event-listener"\nversion = "5.4.2"');
  });

  test("develop pre-push checks PRs, version, and the integration gate", async () => {
    const source = await Bun.file(join(repository, ".githooks/pre-push")).text();
    expect(source).toContain('remote_ref" == "refs/heads/develop');
    expect(source).toContain("scripts/check_incoming_prs.ts");
    expect(source).toContain("scripts/development_version.ts check");
    expect(source).toContain("scripts/integration_gate.sh");
  });

  test("PostgreSQL integration credentials are generated per run", async () => {
    const passwordVariable = "POSTGRES_PASSWORD";
    const formerFixedValue = "postgres";
    const formerPasswordAssignment = `${passwordVariable}=${formerFixedValue}`;
    const formerDatabaseUrl = `postgres://postgres:${formerFixedValue}@`;
    const gate = await Bun.file(join(repository, "scripts/integration_gate.sh")).text();
    expect(gate).toContain("secrets.token_urlsafe(32)");
    expect(gate).toContain('POSTGRES_PASSWORD=$postgres_password');
    expect(gate).toContain("postgres:$postgres_password@127.0.0.1");
    expect(gate).not.toContain(formerPasswordAssignment);
    expect(gate).not.toContain(formerDatabaseUrl);

    const source = await Bun.file(join(repository, ".github/workflows/ci.yml")).text();
    const workflow = Bun.YAML.parse(source) as {
      jobs: Record<string, {
        env?: Record<string, string>;
        services?: Record<string, { env?: Record<string, string> }>;
      }>;
    };
    const storage = workflow.jobs["rust-features"];
    const password = storage.services?.postgres.env?.POSTGRES_PASSWORD;
    const databaseUrl = storage.env?.DATABASE_URL;
    expect(password).toContain("${{ github.run_id }}");
    expect(password).toContain("${{ github.run_attempt }}");
    expect(databaseUrl).toContain(`:${password}@127.0.0.1`);
    expect(source).not.toContain(`${passwordVariable}: ${formerFixedValue}`);
    expect(source).not.toContain(formerDatabaseUrl);
  });

  test("container builds cache dependencies separately from IronFlow source", async () => {
    const dockerfile = await Bun.file(join(repository, "Dockerfile")).text();
    const workflowSource = await Bun.file(
      join(repository, ".github/workflows/container.yml"),
    ).text();

    expect(dockerfile).toContain(
      "lukemathwalker/cargo-chef:0.1.78-rust-1.97.1-slim-bookworm@sha256:" +
        "e406ad0baa7266cee09ca9f62f30d7ed330bdb25be9f337ff8090e7ae215f7fd AS chef",
    );
    expect(dockerfile).toContain("cargo chef prepare --recipe-path recipe.json");
    expect(dockerfile).toContain(
      'cargo chef cook --release --locked --features "${FEATURES}"',
    );

    const dependencyCook = dockerfile.indexOf("cargo chef cook");
    const applicationCopy = dockerfile.indexOf("COPY src ./src", dependencyCook);
    expect(dependencyCook).toBeGreaterThan(-1);
    expect(applicationCopy).toBeGreaterThan(dependencyCook);

    const cacheReference =
      "${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}:buildcache-amd64";
    expect(workflowSource).toContain(
      `cache-from: type=registry,ref=${cacheReference}`,
    );
    expect(workflowSource).toContain(
      `cache-to: type=registry,ref=${cacheReference},mode=max,` +
        "oci-mediatypes=true,image-manifest=true,compression=zstd," +
        "compression-level=15,force-compression=true",
    );
    expect(workflowSource).not.toContain("cache-to: type=gha");
  });

  test("example validation reuses only the Linux release build", async () => {
    const source = await Bun.file(join(repository, ".github/workflows/ci.yml")).text();
    const workflow = Bun.YAML.parse(source) as {
      jobs: Record<string, {
        needs?: string | string[];
        steps: Array<{ uses?: string; run?: string; with?: Record<string, string> }>;
      }>;
    };
    const linuxBuild = workflow.jobs["build-linux"];
    const macosBuild = workflow.jobs["build-macos"];
    const validation = workflow.jobs["validate-examples"];

    expect(linuxBuild).toBeDefined();
    expect(macosBuild).toBeDefined();
    expect(validation.needs).toBe("build-linux");
    expect(
      linuxBuild.steps.some((step) => step.uses === "actions/upload-artifact@v7"),
    ).toBeTrue();
    expect(
      validation.steps.some((step) => step.uses === "actions/download-artifact@v8"),
    ).toBeTrue();

    const validationCommands = validation.steps
      .map((step) => step.run ?? "")
      .join("\n");
    expect(validationCommands).toContain("./target/release/ironflow validate");
    expect(validationCommands).not.toContain("cargo build");
    expect(validationCommands).not.toContain("cargo test");
  });

  test("CI shares compilation within bounded Rust jobs", async () => {
    const source = await Bun.file(join(repository, ".github/workflows/ci.yml")).text();
    const workflow = Bun.YAML.parse(source) as {
      jobs: Record<string, {
        env?: Record<string, string>;
        services?: Record<string, unknown>;
        steps: Array<{ run?: string }>;
      }>;
    };
    const jobs = workflow.jobs;

    for (const removedJob of [
      "check",
      "clippy",
      "full-features",
      "redis-tests",
      "postgres-tests",
      "test",
    ]) {
      expect(jobs[removedJob]).toBeUndefined();
    }

    const defaultCommands = jobs["rust-default"].steps
      .map((step) => step.run)
      .filter(Boolean);
    expect(defaultCommands).toContain("cargo clippy --all-targets -- -D warnings");
    expect(defaultCommands).toContain("cargo test --all-targets");
    expect(defaultCommands).toContain("cargo test --doc");

    const featureJob = jobs["rust-features"];
    const featureCommands = featureJob.steps
      .map((step) => step.run)
      .filter(Boolean)
      .join("\n");
    expect(Object.keys(featureJob.services ?? {}).sort()).toEqual(["postgres", "redis"]);
    expect(featureJob.env?.IRONFLOW_REDIS_TEST_REQUIRED).toBe("1");
    expect(featureJob.env?.IRONFLOW_POSTGRES_TEST_REQUIRED).toBe("1");
    expect(featureCommands).toContain(
      "cargo clippy --all-targets --features postgres,redis -- -D warnings",
    );
    expect(featureCommands).toContain(
      "cargo test --all-targets --features postgres,redis -- --test-threads=1",
    );
    expect(featureCommands).not.toContain("cargo check");

    const compilingJobs = Object.entries(jobs)
      .filter(([, job]) =>
        job.steps.some((step) => /cargo (?:check|clippy|test|build)(?:\s|$)/.test(step.run ?? "")),
      )
      .map(([name]) => name)
      .sort();
    expect(compilingJobs).toEqual([
      "build-linux",
      "build-macos",
      "rust-default",
      "rust-features",
      "test-macos",
      "windows-release-cache",
    ]);
  });

  test("main primes one dependency-only cache for both Windows release variants", async () => {
    const ciSource = await Bun.file(join(repository, ".github/workflows/ci.yml")).text();
    const releaseSource = await Bun.file(
      join(repository, ".github/workflows/release.yml"),
    ).text();
    const ci = Bun.YAML.parse(ciSource) as {
      on: { push: { paths: string[] } };
      jobs: Record<string, {
        if?: string;
        "runs-on": string;
        steps: Array<{ uses?: string; run?: string; with?: Record<string, unknown> }>;
      }>;
    };
    const release = Bun.YAML.parse(releaseSource) as {
      jobs: Record<string, {
        env?: Record<string, string>;
        strategy?: { matrix?: { include?: Array<Record<string, unknown>> } };
        steps: Array<{ uses?: string; with?: Record<string, unknown> }>;
      }>;
    };

    expect(ci.on.push.paths).toContain(".github/workflows/release.yml");
    const primer = ci.jobs["windows-release-cache"];
    expect(primer.if).toBe("github.ref == 'refs/heads/main'");
    expect(primer["runs-on"]).toBe("windows-latest");

    const sharedKey = "release-x86_64-pc-windows-msvc";
    const primerCache = primer.steps.find((step) => step.uses === "Swatinem/rust-cache@v2");
    expect(primerCache?.with?.["shared-key"]).toBe(sharedKey);
    expect(primerCache?.with?.["cache-workspace-crates"]).toBeFalse();
    expect(primer.steps.some((step) => step.uses === "actions/upload-artifact@v7")).toBeFalse();

    const primerCommands = primer.steps.map((step) => step.run ?? "").join("\n");
    expect(primerCommands).toContain(
      "cargo build --release --target x86_64-pc-windows-msvc\n",
    );
    expect(primerCommands).toContain(
      "cargo build --release --target x86_64-pc-windows-msvc --features postgres,redis",
    );

    const releaseBuild = release.jobs.build;
    const variants = releaseBuild.strategy?.matrix?.include ?? [];
    expect(variants).toHaveLength(8);
    for (const target of [
      "x86_64-unknown-linux-musl",
      "x86_64-apple-darwin",
      "aarch64-apple-darwin",
      "x86_64-pc-windows-msvc",
    ]) {
      expect(
        variants
          .filter((entry) => entry.target === target)
          .map((entry) => entry.artifact_suffix)
          .sort(),
      ).toEqual(["", "-full"]);
    }
    const windowsVariants = variants.filter(
      (entry) => entry.target === "x86_64-pc-windows-msvc",
    );
    expect(windowsVariants).toHaveLength(2);
    expect(windowsVariants.every((entry) => entry.cache_key === sharedKey)).toBeTrue();
    expect(windowsVariants.every((entry) => entry.save_cache === false)).toBeTrue();
    expect(windowsVariants.every((entry) => entry.rustflags === "-Dwarnings")).toBeTrue();
    expect(releaseBuild.env?.RUSTFLAGS).toBe("${{ matrix.rustflags }}");

    const releaseCache = releaseBuild.steps.find(
      (step) => step.uses === "Swatinem/rust-cache@v2",
    );
    expect(releaseCache?.with?.["shared-key"]).toBe("${{ matrix.cache_key }}");
    expect(releaseCache?.with?.["cache-workspace-crates"]).toBeFalse();
    expect(releaseCache?.with?.["save-if"]).toBe("${{ matrix.save_cache }}");
  });

  test("the local integration gate bounds workspace artifact growth", async () => {
    const gate = await Bun.file(join(repository, "scripts/integration_gate.sh")).text();
    const cleanup = "cargo clean --package ironflow";
    const prune = 'echo "[integration] pruning stale IronFlow artifacts"';
    const firstRustGate = "cargo fmt --all -- --check";

    expect(gate).toContain(cleanup);
    expect(gate).not.toMatch(/cargo clean(?:\s*(?:\n|$)|\s+--workspace)/);
    expect(gate).toContain("export CARGO_INCREMENTAL=0");
    expect(gate).toContain("trap cleanup EXIT");
    expect(gate.indexOf(prune)).toBeLessThan(gate.indexOf(firstRustGate));
    expect(gate).toContain('echo "[integration] removing gate-owned IronFlow artifacts"');
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

      for (const command of ["cargo-audit", "python3", "bun", "actionlint", "docker"]) {
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
