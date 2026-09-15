import { describe, expect, test } from "bun:test";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const repository = join(import.meta.dir, "../..");

describe("repository integration policy", () => {
  test("CI runs automatically for pushes and pull requests to main and develop", async () => {
    const source = await Bun.file(join(repository, ".github/workflows/ci.yml")).text();
    const workflow = Bun.YAML.parse(source) as Record<string, unknown>;
    const triggers = workflow.on as Record<string, unknown>;
    const push = triggers.push as Record<string, unknown>;
    const pullRequest = triggers.pull_request as Record<string, unknown>;
    expect(push.branches).toEqual(["main", "develop"]);
    expect(pullRequest.branches).toEqual(["main", "develop"]);
    expect(push.paths).toBeUndefined();
    expect(pullRequest.paths).toBeUndefined();
    expect(pullRequest["paths-ignore"]).toEqual(push["paths-ignore"]);
    // Documentation-only edits skip CI; anything that can change the build does not.
    const ignored = (path: string) =>
      (push["paths-ignore"] as string[]).some((pattern) => new Bun.Glob(pattern).match(path));
    expect(ignored("README.md")).toBeTrue();
    expect(ignored(".claude/settings.json")).toBeTrue();
    expect(ignored("src/main.rs")).toBeFalse();
    expect(ignored("Cargo.lock")).toBeFalse();
  });

  test("issue registry changes run the Bun policy gate", async () => {
    const source = await Bun.file(join(repository, ".github/workflows/ci.yml")).text();
    const workflow = Bun.YAML.parse(source) as {
      on: Record<string, { "paths-ignore": string[] }>;
      jobs: Record<string, { steps: Array<{ uses?: string; run?: string }> }>;
    };
    for (const event of ["push", "pull_request"]) {
      for (const path of ["docs/issues/IF-133.md", ".agents/skills/check-ironflow/SKILL.md", "AGENTS.md", "ISSUES.md", "rust-toolchain.toml", ".github/workflows/release.yml"]) {
        expect(workflow.on[event]["paths-ignore"].some((pattern) => new Bun.Glob(pattern).match(path))).toBeFalse();
      }
    }

    const commands = workflow.jobs.policy.steps
      .map((step) => step.run ?? "")
      .join("\n");
    expect(
      workflow.jobs.policy.steps.some(
        (step) => step.uses === "oven-sh/setup-bun@v2",
      ),
    ).toBeTrue();
    expect(commands).toContain("bun run scripts/validate_skills.ts");
    expect(commands).toContain("bun run scripts/issues_registry.ts check");
    expect(commands).toContain("bun test scripts/tests/*.test.ts");

    const gate = await Bun.file(join(repository, "scripts/integration_gate.sh")).text();
    expect(gate).toContain("bun run scripts/issues_registry.ts check");
  });

  test("hook-only changes trigger mandatory repository-policy validation", async () => {
    const workflow = Bun.YAML.parse(await Bun.file(join(repository, ".github/workflows/ci.yml")).text()) as {
      on: Record<string, { "paths-ignore": string[] }>;
      jobs: Record<string, {
        "continue-on-error"?: boolean;
        steps: Array<{ uses?: string; run?: string; "continue-on-error"?: boolean }>;
      }>;
    };
    for (const event of ["push", "pull_request"]) {
      for (const path of [".codex/config.toml", ".codex/hooks.json", ".codex/hooks/tests/test_hooks.py", ".githooks/pre-commit"]) {
        expect(workflow.on[event]["paths-ignore"].some((pattern) => new Bun.Glob(pattern).match(path))).toBeFalse();
      }
    }
    const job = workflow.jobs.policy;
    expect(job["continue-on-error"]).not.toBeTrue();
    for (const command of [
      "python3 -B -m unittest discover -s .codex/hooks/tests -p 'test_*.py' -v",
      "python3 -B -m unittest discover -s scripts/tests -p 'test_*.py' -v",
      "python3 -B scripts/check_module_size.py",
      "sh -n .githooks/pre-commit",
      "bash -n .githooks/pre-push scripts/integration_gate.sh",
    ]) {
      const step = job.steps.find((step) => step.run?.split("\n").includes(command));
      expect(step).toBeDefined();
      expect(step?.["continue-on-error"]).not.toBeTrue();
    }
    expect(job.steps.filter((step) => step.uses === "actions/checkout@v7")).toHaveLength(1);
    const lint = job.steps.find((step) => step.uses === "raven-actions/actionlint@v2");
    expect(lint).toBeDefined();
    expect(lint?.["continue-on-error"]).not.toBeTrue();
  });

  test("only pull requests share a cancellable concurrency group", async () => {
    const workflow = Bun.YAML.parse(await Bun.file(join(repository, ".github/workflows/ci.yml")).text()) as {
      concurrency: { group: string; "cancel-in-progress": string };
    };
    // Pull requests share a per-ref group that newer runs cancel; every other
    // event gets a run-unique group so pushes to main and develop run to completion.
    expect(workflow.concurrency.group).toContain("github.ref");
    expect(workflow.concurrency.group).toContain("github.run_id");
    expect(workflow.concurrency["cancel-in-progress"]).toContain("pull_request");
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
    expect(manifest).toContain('comrak = { version = "0.55", default-features = false }');
    expect(manifest).toContain('lopdf = { version = "0.45", default-features = false, features = ["chrono"] }');
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
      "lukemathwalker/cargo-chef:0.1.78-rust-1.98.1-slim-bookworm@sha256:" +
        "c4b714a1feca5c0784fd063171c8f20803dfc8d940e0fb91b6a6b96e495c2e21 AS chef",
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
      `cache-to: type=registry,ref=${cacheReference},mode=max\n`,
    );
    expect(workflowSource).not.toContain("cache-to: type=gha");
  });

  test("active Rust toolchain pins stay aligned", async () => {
    const toolchainSource = await Bun.file(join(repository, "rust-toolchain.toml")).text();
    const toolchain = Bun.TOML.parse(toolchainSource) as { toolchain: { channel: string } };
    const version = toolchain.toolchain.channel;
    const ci = await Bun.file(join(repository, ".github/workflows/ci.yml")).text();
    const release = await Bun.file(join(repository, ".github/workflows/release.yml")).text();
    const dockerfile = await Bun.file(join(repository, "Dockerfile")).text();

    expect(version).toMatch(/^\d+\.\d+\.\d+$/);
    for (const source of [ci, release]) {
      const pins = source.match(/dtolnay\/rust-toolchain@\d+\.\d+\.\d+/g) ?? [];
      expect(pins.length).toBeGreaterThan(0);
      expect(new Set(pins)).toEqual(new Set([`dtolnay/rust-toolchain@${version}`]));
    }
    expect(dockerfile).toContain(`-rust-${version}-slim-bookworm@sha256:`);
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
    const validation = workflow.jobs["validate-examples"];

    expect(linuxBuild).toBeDefined();
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
      "build-macos",
      "windows-release-cache",
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
      "rust-default",
      "rust-features",
      "test-macos",
    ]);
  });

  test("the local integration gate bounds workspace artifact growth", async () => {
    const gate = await Bun.file(join(repository, "scripts/integration_gate.sh")).text();
    const cleanup = "cargo clean --package ironflow";
    const prune = 'echo "[integration] pruning stale IronFlow artifacts"';
    const firstRustGate = "cargo fmt --all -- --check";
    const defaultCleanup = 'echo "[integration] reclaiming default Rust artifacts"';
    const audit = "cargo audit --deny warnings";
    const integrationCleanup =
      'echo "[integration] reclaiming pre-integration Rust artifacts"';
    const featureTests =
      "cargo test --all-targets --features postgres,redis -- --test-threads=1";

    expect(gate).toContain(cleanup);
    expect(gate).not.toMatch(/cargo clean(?:\s*(?:\n|$)|\s+--workspace)/);
    expect(gate).toContain("export CARGO_INCREMENTAL=0");
    expect(gate).toContain("trap cleanup EXIT");
    expect(gate.indexOf(prune)).toBeLessThan(gate.indexOf(firstRustGate));
    expect(gate.indexOf(audit)).toBeLessThan(gate.indexOf(defaultCleanup));
    expect(gate.indexOf(defaultCleanup)).toBeLessThan(
      gate.indexOf('echo "[integration] feature-enabled Rust gates"'),
    );
    expect(gate.indexOf("Validated $example_count Lua examples.")).toBeLessThan(
      gate.indexOf(integrationCleanup),
    );
    expect(gate.indexOf(integrationCleanup)).toBeLessThan(gate.indexOf(featureTests));
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
