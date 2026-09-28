import { expect, test } from "bun:test";
import { join } from "node:path";

test("container publication waits for runtime storage acceptance", async () => {
  const source = await Bun.file(join(import.meta.dir, "../../.github/workflows/container.yml")).text();
  const workflow = Bun.YAML.parse(source) as {
    on: { push: { paths: string[] } };
    jobs: { publish: { steps: Array<{
      name?: string; run?: string; id?: string; "continue-on-error"?: boolean;
      with?: { load?: boolean; push?: boolean; tags?: string };
    }> } };
  };
  const steps = workflow.jobs.publish.steps;
  const candidate = steps.findIndex(step => step.with?.load === true);
  const acceptance = steps.findIndex(step => step.run?.includes("scripts/test_container_store.ts"));
  const publication = steps.findIndex(step => step.with?.push === true);
  expect(candidate).toBeGreaterThan(-1);
  expect(acceptance).toBeGreaterThan(candidate);
  expect(publication).toBeGreaterThan(acceptance);
  expect(steps[acceptance]["continue-on-error"]).not.toBe(true);
  expect(steps[candidate].with?.push).not.toBe(true);
  expect(steps[publication].with?.tags).toBe("${{ steps.metadata.outputs.tags }}");
  expect(workflow.on.push.paths).toContain("scripts/test_container_store.ts");
});
