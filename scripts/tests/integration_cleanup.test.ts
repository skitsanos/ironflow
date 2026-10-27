import { expect, test } from "bun:test";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const repository = join(import.meta.dir, "../..");

async function cleanupProbe(redis: string, postgres: string, dockerExit = 0) {
  const root = await mkdtemp(join(tmpdir(), "ironflow-cleanup-policy-"));
  try {
    const dockerLog = join(root, "docker.log");
    const cargoLog = join(root, "cargo.log");
    for (const [command, log, exit] of [["cargo", cargoLog, 0], ["docker", dockerLog, dockerExit]] as const) {
      const path = join(root, command);
      await writeFile(path, `#!/bin/sh\nprintf '%s\\n' "$@" > '${log}'\nexit ${exit}\n`);
      await chmod(path, 0o755);
    }
    const source = await Bun.file(join(repository, "scripts/integration_gate.sh")).text();
    const boundary = source.indexOf("for command in cargo ");
    expect(boundary).toBeGreaterThan(0);
    // Exercise the real cleanup/traps, without running the integration workload.
    const child = Bun.spawn(["/bin/bash", "-c", `${source.slice(0, boundary)}
redis_container='${redis}'
postgres_container='${postgres}'
exit 42
`], {
      cwd: repository,
      env: { ...Bun.env, PATH: `${root}:${Bun.env.PATH}` },
      stdout: "pipe", stderr: "pipe",
    });
    const [code, stdout, stderr] = await Promise.all([
      child.exited, new Response(child.stdout).text(), new Response(child.stderr).text(),
    ]);
    expect(code, `${stdout}\n${stderr}`).toBe(42);
    expect(await Bun.file(cargoLog).text()).toBe("clean\n--package\nironflow\n");
    return await Bun.file(dockerLog).exists()
      ? (await Bun.file(dockerLog).text()).slice(0, -1).split("\n")
      : [];
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

for (const [redis, postgres] of [
  ["", ""], ["owned-redis", ""], ["", "owned-postgres"], ["owned-redis", "owned-postgres"],
]) {
  test(`integration cleanup removes only owned containers/anonymous volumes: ${redis}/${postgres}`, async () => {
    const owned = [redis, postgres].filter(Boolean);
    expect(await cleanupProbe(redis, postgres)).toEqual(owned.length ? ["rm", "-f", "-v", ...owned] : []);
  });
}

test("container cleanup failure preserves the gate exit and still reclaims package artifacts", async () => {
  expect(await cleanupProbe("owned-redis", "owned-postgres", 7)).toEqual([
    "rm", "-f", "-v", "owned-redis", "owned-postgres",
  ]);
});
