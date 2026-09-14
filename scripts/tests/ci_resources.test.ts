import { expect, test } from "bun:test";
import { join } from "node:path";

const repository = join(import.meta.dir, "../..");
const validationJobs = ["rust-default", "rust-features"];
const resourceEnvironment = {
  CARGO_INCREMENTAL: "0",
  CARGO_BUILD_JOBS: "2",
  CARGO_PROFILE_DEV_DEBUG: "line-tables-only",
  CARGO_PROFILE_TEST_DEBUG: "line-tables-only",
};

type Step = { name?: string; run?: string; if?: string };
type Workflow = {
  env?: Record<string, string>;
  jobs: Record<string, { env?: Record<string, string>; steps: Step[] }>;
};

async function readWorkflow(name: string): Promise<Workflow> {
  return Bun.YAML.parse(
    await Bun.file(join(repository, `.github/workflows/${name}.yml`)).text(),
  ) as Workflow;
}

test("Linux validation bounds artifacts without changing other build profiles", async () => {
  const workflow = await readWorkflow("ci");
  expect(workflow.env?.RUSTFLAGS).toBe("-Dwarnings");
  for (const [name, job] of Object.entries(workflow.jobs)) {
    const environment = { ...workflow.env, ...job.env };
    for (const [key, value] of Object.entries(resourceEnvironment)) {
      if (validationJobs.includes(name)) {
        expect(environment[key]).toBe(value);
      } else {
        expect(environment[key]).toBeUndefined();
      }
    }
    expect(environment.RUSTFLAGS).toBe("-Dwarnings");
    const profileKeys = Object.keys(environment).filter((key) => key.startsWith("CARGO_PROFILE_"));
    expect(profileKeys.sort()).toEqual(
      validationJobs.includes(name) ? ["CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG"] : [],
    );
  }

  const manifest = Bun.TOML.parse(await Bun.file(join(repository, "Cargo.toml")).text());
  expect(manifest.profile).toBeUndefined();
  const release = await readWorkflow("release");
  for (const job of Object.values(release.jobs)) {
    const environment = { ...release.env, ...job.env };
    for (const key of Object.keys(resourceEnvironment)) {
      expect(environment[key]).toBeUndefined();
    }
  }
});

test("Linux validation reports resources before and after compilation, including failures", async () => {
  const workflow = await readWorkflow("ci");
  for (const name of validationJobs) {
    const steps = workflow.jobs[name].steps;
    const before = steps.findIndex((step) => step.name === "Runner resources before validation");
    const after = steps.findIndex((step) => step.name === "Runner resources after validation");
    const compilations = steps.flatMap((step, index) =>
      /cargo (?:clippy|test)/.test(step.run ?? "") ? [index] : [],
    );
    expect(before).toBeGreaterThanOrEqual(0);
    expect(compilations.length).toBeGreaterThan(0);
    expect(before).toBeLessThan(Math.min(...compilations));
    expect(after).toBeGreaterThan(Math.max(...compilations));
    expect(steps[after].if).toBe("always()");
    for (const index of [before, after]) {
      expect(steps[index].run).toContain("df -h .");
      expect(steps[index].run).toContain("free -h");
    }
    expect(steps[after].run).toContain("du -sh target");
    expect(steps[after].run).toContain("if [[ -d target ]]");
  }
});
