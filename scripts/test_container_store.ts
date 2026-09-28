/** Run against a built image; owns and removes only its disposable fixtures. */
import { randomUUID } from "node:crypto";

const image = Bun.argv[2];
if (!image || image.startsWith("-")) {
  throw new Error("Usage: bun --no-env-file scripts/test_container_store.ts <image>");
}
const prefix = `ironflow-json-smoke-${randomUUID()}`;
const containers = new Set<string>();
const volumes = new Set<string>();
const children = new Set<ReturnType<typeof Bun.spawn>>();
let stopping = false;

async function docker(args: string[], timeout = 60_000): Promise<string> {
  const child = Bun.spawn(["docker", ...args], {
    stdout: "pipe", stderr: "pipe", signal: AbortSignal.timeout(timeout),
  });
  children.add(child);
  try {
    const [code, stdout, stderr] = await Promise.all([
      child.exited, new Response(child.stdout).text(), new Response(child.stderr).text(),
    ]);
    if (code !== 0) throw new Error(`docker ${args[0]} failed: ${stderr.trim()} ${stdout.trim()}`);
    return stdout.trim();
  } finally {
    children.delete(child);
  }
}

async function cleanup() {
  for (const child of children) child.kill();
  await Promise.allSettled([...containers].map(name => docker(["rm", "-f", name])));
  await Promise.allSettled([...volumes].map(name => docker(["volume", "rm", name])));
}
for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.on(signal, () => {
    if (stopping) return;
    stopping = true;
    void cleanup().finally(() => process.exit(signal === "SIGINT" ? 130 : 143));
  });
}

async function ready(url: string) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(1500) });
      if (response.ok) return;
    } catch { /* Server may still be starting. */ }
    await Bun.sleep(200);
  }
  throw new Error("Container API readiness timed out");
}

try {
  for (const uid of [1001, 100123]) {
    const name = `${prefix}-${uid}`;
    const volume = `${name}-data`;
    volumes.add(volume);
    await docker(["volume", "create", volume]);
    containers.add(name);
    await docker([
      "run", "-d", "--name", name, "--platform", "linux/amd64",
      ...(uid === 1001 ? [] : ["--user", `${uid}:0`]), "-p", "127.0.0.1::3000", "-v", `${volume}:/data`,
      "-e", "IRONFLOW_ALLOW_UNAUTHENTICATED_API=true", image,
    ]);
    const binding = await docker(["port", name, "3000/tcp"]);
    const port = binding.match(/^127\.0\.0\.1:(\d+)$/)?.[1];
    if (!port) throw new Error("Expected a loopback-only container port");
    const url = `http://127.0.0.1:${port}`;
    await ready(`${url}/health/ready`);
    const response = await fetch(`${url}/flows/run`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        source: 'local flow = Flow.new("container_store")\nflow:step("hello", nodes.log({message="Hello from IronFlow!"}))\nreturn flow',
      }),
      signal: AbortSignal.timeout(30_000),
    });
    const result = await response.json() as { run_id?: string; status?: string };
    if (!response.ok || result.status !== "success" || !result.run_id) {
      throw new Error(`UID ${uid}: workflow failed: ${JSON.stringify(result)}`);
    }
    const mode = await docker(["exec", name, "stat", "-c", "%u:%a", "/data/runs"]);
    if (mode !== `${uid}:700`) throw new Error(`Unexpected private store ownership/mode: ${mode}`);
    await docker(["stop", name]);
    await docker(["rm", name]);
    containers.delete(name);
    const reader = `${name}-reader`;
    containers.add(reader);
    const inspected = await docker([
      "run", "--rm", "--name", reader, "--platform", "linux/amd64",
      "--user", `${uid}:0`, "-v", `${volume}:/data`, image, "inspect", result.run_id,
    ]);
    containers.delete(reader);
    if (!inspected.includes(result.run_id) || !inspected.toLowerCase().includes("success")) {
      throw new Error(`UID ${uid}: persisted run was not readable after server removal`);
    }
    console.log(`PASS UID ${uid}: default JSON store, mode 0700, execution and restart persistence`);
  }
} finally {
  await cleanup();
}
