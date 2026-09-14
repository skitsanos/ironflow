import { expect, test } from "bun:test";
import { join } from "node:path";

const root = join(import.meta.dir, "../..");

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
