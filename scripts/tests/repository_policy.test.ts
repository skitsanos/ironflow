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
      "lukemathwalker/cargo-chef:0.1.77-rust-1.97.1-bookworm@sha256:" +
        "1689f62cfaa6603480356923cb5966544b2dd6ea523e30486bee4f149965d5bc AS chef",
    );
    expect(dockerfile).toContain("cargo chef prepare --recipe-path recipe.json");
    expect(dockerfile).toContain(
      'cargo chef cook --release --locked --features "${FEATURES}"',
    );

    const dependencyCook = dockerfile.indexOf("cargo chef cook");
    const applicationCopy = dockerfile.indexOf("COPY src ./src", dependencyCook);
    expect(dependencyCook).toBeGreaterThan(-1);
    expect(applicationCopy).toBeGreaterThan(dependencyCook);

    expect(workflowSource).toContain(
      "cache-from: type=gha,scope=ironflow-container-amd64",
    );
    expect(workflowSource).toContain(
      "cache-to: type=gha,scope=ironflow-container-amd64,mode=max",
    );
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
    ]);
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
