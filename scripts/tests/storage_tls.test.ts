import { expect, test } from "bun:test";
import { join } from "node:path";
import { postgresReadinessCommand, waitForReady } from "../storage_tls/fixture";

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
