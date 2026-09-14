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
const artifact = `ironflow-${target}${suffix}`;
const archive = `ironflow-${"${{ github.ref_name }}"}-${target}${suffix}`;

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
  const buildSteps = steps.filter((step) => /cargo build/.test(step.run ?? ""));
  expect(buildSteps).toHaveLength(1);
  expect(buildSteps[0].if).toBeUndefined();
  expect(buildSteps[0].run).toBe(
    `cargo build --locked --release --target ${target} ${"${{ matrix.suffix == '-full' && '--features postgres,redis' || '' }}"}`,
  );
  expect(steps.find((step) => step.uses?.startsWith("dtolnay/rust-toolchain@"))?.with?.targets).toBe(target);
  const musl = steps.find((step) => step.name === "Install musl tools (Linux)");
  expect(musl?.if).toBe("matrix.platform.target == 'x86_64-unknown-linux-musl'");
  expect(musl?.run).toContain("musl-tools");

  const cache = steps.find((step) => step.uses === "Swatinem/rust-cache@v2");
  expect(cache?.with).toEqual({
    "shared-key": `release-${target}${suffix}`,
    "cache-workspace-crates": false,
  });
});

test("release packages and uploads distinct nonempty artifacts for every variant", async () => {
  const { jobs } = await readWorkflow();
  const unix = jobs.build.steps.find((step) => step.name === "Package (unix)");
  expect(unix?.if).toBe("runner.os != 'Windows'");
  expect(unix?.run).toBe(`tar czf ${archive}.tar.gz -C target/${target}/release ironflow`);
  const windows = jobs.build.steps.find((step) => step.name === "Package (windows)");
  expect(windows?.if).toBe("runner.os == 'Windows'");
  expect(windows?.shell).toBe("pwsh");
  expect(windows?.run).toBe(
    `Compress-Archive -Path "target/${target}/release/ironflow.exe" -DestinationPath "${archive}.zip"`,
  );
  expect(jobs.build.steps.find((step) => step.uses === "actions/upload-artifact@v7")?.with).toEqual({
    name: artifact,
    path: `${archive}.*`,
    "if-no-files-found": "error",
  });
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
