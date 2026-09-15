import { expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { postgresReadinessCommand, readinessTimeout, waitForReady } from "../storage_tls/fixture";

const root = join(import.meta.dir, "../..");

test("PostgreSQL readiness authenticates over verified TCP instead of the bootstrap socket", () => {
  const args = postgresReadinessCommand("test-postgres", "test-only-password");
  expect(args.slice(0, 6)).toEqual(["docker", "exec", "-e", "PGPASSWORD=test-only-password", "test-postgres", "psql"]);
  expect(args).toContain("-X");
  expect(args).toContain("ON_ERROR_STOP=1");
  expect(args[args.indexOf("--dbname") + 1]).toBe("host=localhost hostaddr=127.0.0.1 user=postgres dbname=ironflow_tls sslmode=verify-full sslrootcert=/tls/ca.pem connect_timeout=3");
  expect(args.slice(-2)).toEqual(["--command", "SELECT 1"]);
  expect(args).not.toContain("pg_isready");
});

test("readiness waits past socket readiness and failed queries for the exact query result", async () => {
  const args = postgresReadinessCommand("test-postgres", "test-only-password");
  const responses = [
    { code: 0, stdout: "accepting connections\n", stderr: "", timedOut: false },
    { code: 1, stdout: "1\n", stderr: "connection failed", timedOut: false },
    { code: 0, stdout: "11\n", stderr: "", timedOut: false },
    { code: 0, stdout: "1\n", stderr: "", timedOut: false },
  ];
  let calls = 0;
  await waitForReady(args, "1", async (received, options) => {
    expect(received).toEqual(args);
    expect(options?.timeout).toBeLessThanOrEqual(5000);
    return responses[calls++];
  });
  expect(calls).toBe(4);
});

test("readiness rejects timed-out output and stops at the deadline without leaking diagnostics", async () => {
  let calls = 0;
  const failure = await waitForReady(["probe", "test-only-password"], "1", async (_args, options) => {
    calls++;
    expect(options?.timeout).toBeLessThanOrEqual(50);
    return { code: 0, stdout: "1\n", stderr: "test-only-password", timedOut: true };
  }, 50).then(() => null, error => error);
  expect(failure).toBeInstanceOf(Error);
  expect(failure.message).toBe("Disposable TLS store did not become ready");
  expect(calls).toBeGreaterThan(0);
});

test("readiness accepts a genuine success that lands just after the deadline", async () => {
  let calls = 0;
  await waitForReady(["probe"], "1", async (_args, options) => {
    calls++;
    // The probe answers correctly, but only ~20 ms after the deadline passed.
    await Bun.sleep((options?.timeout ?? 50) + 20);
    return { code: 0, stdout: "1\n", stderr: "", timedOut: false };
  }, 50);
  expect(calls).toBe(1);
});

test("readiness keeps the per-probe clamp inside a budget that fits slow image starts", async () => {
  expect(readinessTimeout).toBeGreaterThanOrEqual(90_000);
  let clamp = 0;
  await waitForReady(["probe"], "1", async (_args, options) => {
    clamp = options?.timeout ?? 0;
    return { code: 0, stdout: "1\n", stderr: "", timedOut: false };
  });
  expect(clamp).toBe(5000);
});

test("shared-store features include TLS without the insecure Redis bypass", async () => {
  const manifest = Bun.TOML.parse(await Bun.file(join(root, "Cargo.toml")).text()) as {
    features: Record<string, string[]>;
    dependencies: Record<string, { optional?: boolean; features?: string[] }>;
  };
  expect(manifest.features.postgres).toContain("sqlx/tls-rustls");
  expect(manifest.features.postgres).toContain("dep:rustls");
  expect(manifest.features.redis).toContain("redis/tokio-rustls-comp");
  expect(manifest.features.redis).toContain("dep:rustls");
  expect(manifest.dependencies.rustls.optional).toBeTrue();
  expect(manifest.dependencies.rustls.features).toContain("ring");
  expect(JSON.stringify(manifest)).not.toContain("tls-rustls-insecure");
  expect(JSON.stringify(manifest)).not.toContain("tls-native-tls");
});

test("TLS acceptance runs after the full-feature build in local and CI gates", async () => {
  const workflow = Bun.YAML.parse(await Bun.file(join(root, ".github/workflows/ci.yml")).text()) as {
    jobs: Record<string, { steps: { uses?: string; run?: string; "continue-on-error"?: boolean }[] }>;
  };
  const steps = workflow.jobs["rust-features"].steps;
  const tls = steps.findIndex(s => s.run === "bun --no-env-file scripts/test_store_tls.ts target/debug/ironflow");
  const build = steps.findIndex(s => s.run?.includes("cargo test --all-targets --features postgres,redis"));
  expect(build).toBeGreaterThanOrEqual(0);
  expect(tls).toBeGreaterThan(build);
  expect(steps.slice(0, tls).some(s => s.uses === "oven-sh/setup-bun@v2")).toBeTrue();
  expect(steps[tls]["continue-on-error"]).not.toBeTrue();
  const gate = await Bun.file(join(root, "scripts/integration_gate.sh")).text();
  expect(gate).toContain("docker openssl");
  expect(gate).toContain("bun --no-env-file scripts/test_store_tls.ts target/debug/ironflow");
});

test("Ctrl-C stops the gate, removes only its containers and keys, and exits 130", async () => {
  const fakeBin = await mkdtemp(join(tmpdir(), "ironflow-tls-fake-"));
  const log = join(fakeBin, "docker.log");
  const started = join(fakeBin, "started");
  try {
    // docker is faked and records every invocation; openssl is real; the
    // "ironflow" binary records its pid and HOME (the fixture root), then sleeps
    // so the interrupt arrives while a tracked CLI child is running.
    await writeFile(join(fakeBin, "docker"), `#!/bin/sh
printf '%s\\n' "$*" >> "$IRONFLOW_TEST_DOCKER_LOG"
case "$*" in
  "run "*) echo 0123456789abcdef ;;
  "exec "*psql*) echo 1 ;;
  "exec "*redis-cli*) echo PONG ;;
  "exec "*--version*) echo "fake 0.0" ;;
  "port "*5432/tcp) echo 127.0.0.1:55432 ;;
  "port "*6379/tcp) echo 127.0.0.1:56379 ;;
esac
`);
    await writeFile(join(fakeBin, "ironflow"), `#!/bin/sh
printf '%s %s\\n' "$$" "$HOME" > "$(dirname "$0")/started"
exec sleep 60
`);
    for (const name of ["docker", "ironflow"]) await chmod(join(fakeBin, name), 0o755);
    const gate = Bun.spawn([process.execPath, "--no-env-file", join(root, "scripts/test_store_tls.ts"), join(fakeBin, "ironflow")], {
      cwd: root,
      env: { ...Bun.env, PATH: `${fakeBin}:${Bun.env.PATH}`, IRONFLOW_TEST_DOCKER_LOG: log },
      stdout: "pipe",
      stderr: "pipe",
    });
    const output = Promise.all([new Response(gate.stdout).text(), new Response(gate.stderr).text()]);
    const deadline = Date.now() + 30_000;
    let marker = "";
    while (!marker.includes("\n")) {
      if (gate.exitCode !== null || Date.now() > deadline) {
        throw new Error(`gate never reached the CLI phase (exit ${gate.exitCode}): ${(await output).join("\n")}`);
      }
      await Bun.sleep(50);
      marker = existsSync(started) ? await Bun.file(started).text() : "";
    }
    const [pid, fixtureRoot] = marker.trim().split(" ");
    expect(existsSync(join(fixtureRoot, "server.key"))).toBeTrue();

    gate.kill("SIGINT");
    expect(await gate.exited).toBe(130);
    const [, stderr] = await output;
    expect(stderr).toContain("interrupted by SIGINT");

    const lines = (await Bun.file(log).text()).trim().split("\n");
    const containers = lines.filter((line) => line.startsWith("run -d --name ")).map((line) => line.split(" ")[3]);
    expect(containers).toHaveLength(2);
    expect(containers[0]).toMatch(/^ironflow-tls-postgres-[0-9a-f-]{36}$/);
    expect(containers[1]).toMatch(/^ironflow-tls-redis-[0-9a-f-]{36}$/);
    expect(lines.filter((line) => line.startsWith("rm "))).toEqual([`rm -f -v ${containers.join(" ")}`]);
    expect(lines.at(-1)).toStartWith("rm -f -v ");
    expect(existsSync(fixtureRoot)).toBeFalse();

    let alive = true;
    for (let attempt = 0; alive && attempt < 40; attempt++) {
      try { process.kill(Number(pid), 0); await Bun.sleep(50); } catch { alive = false; }
    }
    expect(alive).toBeFalse();
  } finally {
    await rm(fakeBin, { recursive: true, force: true });
  }
}, 60_000);
