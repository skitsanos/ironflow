import { describe, expect, test } from "bun:test";
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
});
