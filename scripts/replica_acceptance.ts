#!/usr/bin/env bun

const project = "ironflow-replica-acceptance";
const composeFile = "deploy/replica/docker-compose.yml";
const a = "http://127.0.0.1:3101";
const b = "http://127.0.0.1:3102";
const proxy = "http://127.0.0.1:3100";

if (process.env.IRONFLOW_RUN_REPLICA_ACCEPTANCE !== "1") {
  throw new Error(
    "Refusing destructive replica acceptance without IRONFLOW_RUN_REPLICA_ACCEPTANCE=1",
  );
}

function compose(...args: string[]): string {
  const command = [
    "docker",
    "compose",
    "--project-name",
    project,
    "--file",
    composeFile,
    ...args,
  ];
  const result = Bun.spawnSync(command, { stdout: "pipe", stderr: "pipe" });
  const stdout = result.stdout.toString();
  const stderr = result.stderr.toString();
  if (result.exitCode !== 0) {
    throw new Error(`${command.join(" ")} failed\n${stdout}${stderr}`);
  }
  return stdout;
}

function serviceIsRunning(service: string): boolean {
  const running = compose("ps", "--status", "running", "--services");
  return running.split(/\s+/).includes(service);
}

async function waitFor(
  description: string,
  probe: () => Promise<boolean>,
  timeoutMs: number,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      if (await probe()) return;
    } catch {
      // Containers and sockets legitimately disappear during fault injection.
    }
    await Bun.sleep(500);
  }
  throw new Error(`Timed out waiting for ${description}`);
}

async function ready(base: string): Promise<boolean> {
  const response = await fetch(`${base}/health/ready`, {
    signal: AbortSignal.timeout(2_000),
  });
  return response.status === 200;
}

function runIdForKey(key: string): string {
  return `idem-${new Bun.CryptoHasher("sha256").update(key).digest("hex")}`;
}

function runRequest(key: string, file: string): Promise<Response> {
  return fetch(`${a}/flows/run`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "idempotency-key": key,
    },
    body: JSON.stringify({ file }),
    signal: AbortSignal.timeout(30_000),
  });
}

function triggerRunRequest(key: string, file: string): void {
  const result = Bun.spawnSync(
    [
      "curl",
      "--silent",
      "--show-error",
      "--request",
      "POST",
      "--max-time",
      "1",
      "--header",
      "content-type: application/json",
      "--header",
      `idempotency-key: ${key}`,
      "--data",
      JSON.stringify({ file }),
      `${a}/flows/run`,
    ],
    { stdout: "ignore", stderr: "ignore" },
  );
  // curl 28 is the intentional one-second client timeout. A very fast test
  // flow may complete with zero; every other exit is an infrastructure error.
  if (result.exitCode !== 0 && result.exitCode !== 28) {
    throw new Error(`curl trigger exited with ${result.exitCode}`);
  }
}

async function runInfo(base: string, runId: string): Promise<any> {
  const response = await fetch(`${base}/runs/${runId}`, {
    signal: AbortSignal.timeout(2_000),
  });
  if (!response.ok) throw new Error(`GET ${runId} returned ${response.status}`);
  return response.json();
}

async function waitForStatus(
  base: string,
  runId: string,
  expected: string,
  timeoutMs: number,
): Promise<void> {
  await waitFor(
    `${runId} to become ${expected}`,
    async () => (await runInfo(base, runId)).status === expected,
    timeoutMs,
  );
}

async function assertProxyUsesBothReplicas(): Promise<void> {
  const upstreams = new Set<string>();
  for (let index = 0; index < 8; index += 1) {
    const response = await fetch(`${proxy}/health/live`);
    if (!response.ok) throw new Error(`proxy health returned ${response.status}`);
    upstreams.add(response.headers.get("x-ironflow-upstream") ?? "");
  }
  if (upstreams.size !== 2) {
    throw new Error(`round-robin proxy reached ${[...upstreams].join(", ")}`);
  }
}

async function assertScheduleUniqueness(): Promise<void> {
  const response = await fetch(`${b}/runs?limit=100`);
  if (!response.ok) throw new Error(`run listing returned ${response.status}`);
  const listing: any = await response.json();
  const occurrences = new Map<string, number>();
  for (const summary of listing.runs ?? []) {
    const info = await runInfo(b, summary.id);
    const instant = info.ctx?._schedule_instant;
    if (typeof instant === "string") {
      occurrences.set(instant, (occurrences.get(instant) ?? 0) + 1);
    }
  }
  if (occurrences.size === 0) throw new Error("scheduler produced no acceptance run");
  for (const [instant, count] of occurrences) {
    if (count !== 1) throw new Error(`schedule ${instant} produced ${count} runs`);
  }
}

async function main(): Promise<void> {
  compose("down", "--volumes", "--remove-orphans");
  try {
    compose("build", "ironflow-a", "ironflow-b");
    compose("up", "--detach");
    await waitFor("replica A readiness", () => ready(a), 90_000);
    await waitFor("replica B readiness", () => ready(b), 90_000);
    await waitFor("proxy readiness", () => ready(proxy), 30_000);
    await assertProxyUsesBothReplicas();
    console.log("[replica] cold start and round-robin routing passed");

    const crossKey = "replica-cross-read";
    const first = await runRequest(crossKey, "quick.lua");
    if (!first.ok) throw new Error(`initial run returned ${first.status}`);
    const created: any = await first.json();
    const visible = await runInfo(b, created.run_id);
    if (visible.status !== "success") throw new Error("replica B did not see success");
    const retry = await fetch(`${b}/flows/run`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "idempotency-key": crossKey,
      },
      body: JSON.stringify({ file: "quick.lua" }),
    });
    const retried: any = await retry.json();
    if (!retry.ok || retried.run_id !== created.run_id) {
      throw new Error("cross-replica idempotent retry created another run");
    }
    console.log("[replica] cross-replica visibility and idempotency passed");

    const termKey = "replica-sigterm";
    const termRun = runIdForKey(termKey);
    triggerRunRequest(termKey, "hold.lua");
    await waitForStatus(b, termRun, "running", 30_000);
    compose("kill", "--signal", "SIGTERM", "ironflow-a");
    await waitForStatus(b, termRun, "cancelled", 35_000);
    await waitFor(
      "replica A process exit",
      async () => !serviceIsRunning("ironflow-a"),
      30_000,
    );
    console.log("[replica] SIGTERM drain passed");

    compose("start", "ironflow-a");
    await waitFor("restarted replica A readiness", () => ready(a), 90_000);
    console.log("[replica] terminated replica restarted and became ready");

    const killKey = "replica-sigkill";
    const killedRun = runIdForKey(killKey);
    triggerRunRequest(killKey, "hold.lua");
    await waitForStatus(b, killedRun, "running", 30_000);
    compose("kill", "--signal", "SIGKILL", "ironflow-a");
    await waitForStatus(b, killedRun, "stalled", 135_000);
    console.log("[replica] SIGKILL lease reconciliation passed");

    if (!(await ready(b))) throw new Error("surviving replica lost readiness");
    await assertScheduleUniqueness();
    console.log("Replica acceptance passed: routing, durable visibility, TERM drain, KILL reconciliation, and schedule uniqueness.");
  } finally {
    compose("down", "--volumes", "--remove-orphans");
  }
}

await main();
