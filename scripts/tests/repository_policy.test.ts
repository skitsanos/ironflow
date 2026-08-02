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
    const postgres = workflow.jobs["postgres-tests"];
    const password = postgres.services?.postgres.env?.POSTGRES_PASSWORD;
    const databaseUrl = postgres.env?.DATABASE_URL;
    expect(password).toContain("${{ github.run_id }}");
    expect(password).toContain("${{ github.run_attempt }}");
    expect(databaseUrl).toContain(`:${password}@127.0.0.1`);
    expect(source).not.toContain(`${passwordVariable}: ${formerFixedValue}`);
    expect(source).not.toContain(formerDatabaseUrl);
  });
});
