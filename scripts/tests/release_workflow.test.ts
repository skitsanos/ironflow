import { expect, test } from "bun:test";
import { join } from "node:path";

type Step = {
  name?: string;
  uses?: string;
  run?: string;
  if?: string;
  shell?: string;
  with?: Record<string, unknown>;
};
type Platform = { target: string; os: string };
type Job = {
  name?: string;
  needs?: string;
  "runs-on"?: string;
  permissions?: Record<string, string>;
  strategy?: { matrix: { platform: Platform[]; suffix: string[] } };
  steps: Step[];
};
type Workflow = {
  on: Record<string, unknown>;
  env: Record<string, string>;
  permissions: Record<string, string>;
  jobs: Record<string, Job>;
};

const platforms: Platform[] = [
  { target: "x86_64-unknown-linux-musl", os: "ubuntu-latest" },
  { target: "x86_64-apple-darwin", os: "macos-latest" },
  { target: "aarch64-apple-darwin", os: "macos-latest" },
  { target: "x86_64-pc-windows-msvc", os: "windows-latest" },
];
const target = "${{ matrix.platform.target }}";
const suffix = "${{ matrix.suffix }}";

async function readWorkflow(): Promise<Workflow> {
  return Bun.YAML.parse(
    await Bun.file(join(import.meta.dir, "../../.github/workflows/release.yml")).text(),
  ) as Workflow;
}

test("release has eight unique target and feature combinations on the correct runners", async () => {
  const { jobs } = await readWorkflow();
  const matrix = jobs.build.strategy!.matrix;
  // Separate axes form a Cartesian product; include-only targets do not.
  expect(Object.keys(matrix).sort()).toEqual(["platform", "suffix"]);
  expect(matrix.platform).toEqual(platforms);
  expect(matrix.suffix).toEqual(["", "-full"]);
  const variants = matrix.platform.flatMap((platform) =>
    matrix.suffix.map((suffix) => `${platform.target}${suffix}`),
  );
  expect(variants).toHaveLength(8);
  expect(new Set(variants).size).toBe(8);
  expect(jobs.build.name).toBe(`Build ${target}${suffix}`);
  expect(jobs.build["runs-on"]).toBe("${{ matrix.platform.os }}");
});

test("release builds both feature variants with locked dependencies and strict warnings", async () => {
  const workflow = await readWorkflow();
  expect(workflow.env.RUSTFLAGS).toBe("-Dwarnings");
  const steps = workflow.jobs.build.steps;
  const builds = steps.filter((step) => /cargo build/.test(step.run ?? ""));
  expect(builds).toHaveLength(1);
  const [build] = builds;
  // One unconditional build per matrix cell: locked, cross-targeted, features chosen by the suffix axis.
  expect(build.if).toBeUndefined();
  expect(build.run).toContain("--locked");
  expect(build.run).toContain(`--target ${target}`);
  expect(build.run).toContain("matrix.suffix == '-full' && '--features postgres,redis'");
  expect(steps.find((step) => step.uses?.startsWith("dtolnay/rust-toolchain@"))?.with?.targets).toBe(target);
  const musl = steps.find((step) => /musl-tools/.test(step.run ?? ""));
  expect(musl?.if).toContain("matrix.platform.target == 'x86_64-unknown-linux-musl'");
  const cache = steps.find((step) => step.uses?.startsWith("Swatinem/rust-cache@"));
  const sharedKey = String(cache?.with?.["shared-key"]);
  expect(sharedKey).toContain(target);
  expect(sharedKey).toContain(suffix);
});

test("release packages and uploads distinct nonempty artifacts for every variant", async () => {
  const { jobs } = await readWorkflow();
  const steps = jobs.build.steps;
  const unix = steps.find((step) => /\btar\b/.test(step.run ?? ""));
  const windows = steps.find((step) => /Compress-Archive/.test(step.run ?? ""));
  expect(unix?.if).toMatch(/runner\.os\s*!=\s*'Windows'/);
  expect(windows?.if).toMatch(/runner\.os\s*==\s*'Windows'/);
  expect(windows?.shell).toBe("pwsh");
  for (const step of [unix, windows]) {
    // Archive names carry target and suffix so the eight variants never collide.
    expect(step?.run).toContain(target);
    expect(step?.run).toContain(suffix);
  }
  const upload = steps.find((step) => step.uses?.startsWith("actions/upload-artifact@"));
  expect(upload?.with?.["if-no-files-found"]).toBe("error");
  const name = String(upload?.with?.name);
  expect(name).toContain(target);
  expect(name).toContain(suffix);
  expect(String(upload?.with?.path)).toContain(target);
});

test("only guarded tag releases can publish and only the publishing job can write", async () => {
  const workflow = await readWorkflow();
  expect(workflow.on).toEqual({ push: { tags: ["v*"] } });
  expect(workflow.permissions).toEqual({ contents: "read" });
  expect(workflow.jobs.build.needs).toBe("guard");
  expect(workflow.jobs.guard.steps.some((step) => step.run?.includes("git merge-base --is-ancestor"))).toBeTrue();
  expect(workflow.jobs.release.needs).toBe("build");
  for (const [name, job] of Object.entries(workflow.jobs)) {
    expect(job.permissions ?? workflow.permissions).toEqual({
      contents: name === "release" ? "write" : "read",
    });
  }
});
