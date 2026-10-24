import { expect, test } from "bun:test";
import { join } from "node:path";

const root = join(import.meta.dir, "../..");

test("resolved TLS verification and bundled roots include the eMudhra fixes", async () => {
  const lock = Bun.TOML.parse(await Bun.file(join(root, "Cargo.lock")).text()) as {
    package: Array<{ name: string; version: string }>;
  };
  for (const [name, minimum] of [
    ["rustls-webpki", "0.103.16"],
    ["webpki-roots", "1.0.10"],
    ["webpki-root-certs", "1.0.10"],
  ]) {
    const packages = lock.package.filter((entry) => entry.name === name);
    expect(packages.length).toBeGreaterThan(0);
    for (const entry of packages) {
      expect(Bun.semver.satisfies(entry.version, `>=${minimum}`)).toBeTrue();
    }
  }
});

test("both container stages require Debian's patched CA package", async () => {
  const dockerfile = await Bun.file(join(root, "Dockerfile")).text();
  const guard = "dpkg --compare-versions \"$(dpkg-query -W -f='${Version}' ca-certificates)\" ge '20250419~deb12u2'";
  expect(dockerfile.match(/20250419~deb12u2/g)).toHaveLength(2);
  for (const stage of [dockerfile.split(" AS chef\n")[1]?.split("FROM chef")[0], dockerfile.split(" AS runtime\n")[1]]) {
    expect(stage).toContain(guard);
    expect(stage?.indexOf("apt-get install")).toBeLessThan(stage?.indexOf(guard) ?? -1);
  }
});

test("AWS signing excludes SigV4a and its affected bigint dependency", async () => {
  const manifest = Bun.TOML.parse(await Bun.file(join(root, "Cargo.toml")).text()) as {
    dependencies: Record<string, { "default-features"?: boolean; features: string[] }>;
  };
  const s3 = manifest.dependencies["aws-sdk-s3"];
  expect(s3["default-features"]).toBeFalse();
  expect(s3.features).not.toContain("sigv4a");
  expect(s3.features).toContain("default-https-client");
  const lock = Bun.TOML.parse(await Bun.file(join(root, "Cargo.lock")).text()) as {
    package: Array<{ name: string; version: string }>;
  };
  for (const entry of lock.package.filter((entry) => entry.name === "crypto-bigint")) {
    expect(Bun.semver.satisfies(entry.version, ">=0.7.6")).toBeTrue();
  }
});

test("pinned Bun jobs include the patched embedded trust store", async () => {
  const workflow = Bun.YAML.parse(await Bun.file(join(root, ".github/workflows/ci.yml")).text()) as {
    jobs: Record<string, { steps: Array<{ uses?: string; with?: { "bun-version"?: string } }> }>;
  };
  for (const job of Object.values(workflow.jobs)) {
    for (const step of job.steps.filter((step) => step.uses?.startsWith("oven-sh/setup-bun@"))) {
      if (step.with?.["bun-version"]) {
        expect(Bun.semver.satisfies(step.with["bun-version"], ">=1.4.4")).toBeTrue();
      }
    }
  }
});
