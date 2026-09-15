import assert from "node:assert/strict";
import { join, resolve } from "node:path";
import { InterruptedError, command, createFixture, interrupt, interrupted, throwIfInterrupted, track } from "./storage_tls/fixture";

// This gate owns its servers, certificates, credentials and storage. It never
// inherits a database URL or .env file from the developer's environment.
const binary = resolve(process.argv[2] ?? "target/debug/ironflow");
assert(await Bun.file(binary).exists(), "Build ironflow with --features postgres,redis first");

// Bun does not run `finally` blocks when a signal terminates it, so Ctrl-C is
// answered explicitly: stop every spawned process, remove the disposable
// containers and keys exactly once, then exit 130 (SIGINT) or 143 (SIGTERM).
let fixture: Awaited<ReturnType<typeof createFixture>> | undefined;
function redact(text: string) {
  return fixture ? text.replaceAll(fixture.password, "[redacted]") : text;
}
function exitInterrupted(error?: unknown): never {
  const { signal, code } = interrupted()!;
  if (error !== undefined && !(error instanceof InterruptedError)) {
    // Cleanup itself failed, so disposable resources may remain: fail loudly.
    console.error(redact(error instanceof Error ? error.message : String(error)));
    process.exit(1);
  }
  console.error(`TLS gate interrupted by ${signal}; disposable containers and keys were removed`);
  process.exit(code);
}
async function shutdown(): Promise<never> {
  try { await fixture!.cleanup(); } catch (error) { exitInterrupted(error); }
  exitInterrupted();
}
// During setup createFixture answers the signal itself and removes whatever it
// already created before rejecting; only the exit code is decided here.
fixture = await createFixture().catch((error: unknown) => {
  if (interrupted()) exitInterrupted(error);
  throw error;
});
const { root, password, cleanup, postgresPort, redisPort } = fixture;
const onSignal = (signal: string) => { interrupt(signal); void shutdown(); };
process.once("SIGINT", onSignal);
process.once("SIGTERM", onSignal);

const flow = "local f = Flow.new('tls-probe'); f:step('answer', nodes.code({source='return {answer = 42}'})); return f";
const baseEnv = { PATH: process.env.PATH ?? "", HOME: root, TMPDIR: root, RUST_LOG: "info", NO_COLOR: "1" };

function pgUrl(mode: string, ca = "ca.pem", host = "localhost") {
  const url = new URL(`postgres://postgres:${password}@${host}:${postgresPort}/ironflow_tls`);
  url.searchParams.set("sslmode", mode);
  if (mode !== "require" && mode !== "disable") url.searchParams.set("sslrootcert", join(root, ca));
  return url.toString();
}
function storeEnv(backend: string, url: string, ca = "ca.pem") {
  return { ...baseEnv, IRONFLOW_STORE: backend, IRONFLOW_EVENT_STORE: backend, IRONFLOW_STORE_URL: url, IRONFLOW_EVENT_STORE_URL: url, REDIS_URL: url, REDIS_PREFIX: "if137:", SSL_CERT_FILE: join(root, ca) };
}
async function cli(env: Record<string, string>, args: string[], accept: boolean, label: string) {
  const result = await command([binary, ...args], { cwd: root, env, timeout: 45_000 });
  assert(!result.timedOut, `${label}: timed out instead of accepting/rejecting the connection`);
  assert(result.code !== 101 && !result.stderr.includes("panicked at"), `${label}: connection handling panicked`);
  assert.equal(result.code === 0, accept, `${label}: unexpected exit ${result.code}: ${result.stderr.replaceAll(password, "[redacted]")}`);
  console.log(`PASS ${label}`);
  return result.stdout;
}
function finalContext(stdout: string): Record<string, unknown> {
  const marker = "\nContext:\n";
  const offset = stdout.lastIndexOf(marker);
  assert(offset >= 0, "ironflow run did not print its final context");
  return JSON.parse(stdout.slice(offset + marker.length));
}
async function start(env: Record<string, string>) {
  throwIfInterrupted();
  const child = track(Bun.spawn([binary, "serve", "--host", "127.0.0.1", "--port", "0"], { cwd: root, env: { ...env, IRONFLOW_API_KEY: password, IRONFLOW_ALLOW_ADHOC_FLOWS: "true" }, stdout: "pipe", stderr: "pipe" }));
  let logs = "";
  async function drain(stream: ReadableStream<Uint8Array>) {
    for await (const chunk of stream) logs = (logs + new TextDecoder().decode(chunk)).slice(-32_768);
  }
  const stdout = drain(child.stdout);
  const stderr = drain(child.stderr);
  async function stop() {
    child.kill("SIGTERM");
    const timer = setTimeout(() => child.kill("SIGKILL"), 5000);
    try { await child.exited; } finally { clearTimeout(timer); }
    await stdout;
    await stderr;
  }
  try {
    const deadline = Date.now() + 25_000;
    while (Date.now() < deadline && child.exitCode === null) {
      throwIfInterrupted();
      const address = logs.match(/listening on (127\.0\.0\.1:\d+)/)?.[1];
      if (address) {
        const request = async (path: string, body?: unknown) => {
          const response = await fetch(`http://${address}${path}`, { method: body === undefined ? "GET" : "POST", headers: { authorization: `Bearer ${password}`, "content-type": "application/json" }, body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(10_000) });
          assert(response.ok, `${path}: HTTP ${response.status}`);
          return response;
        };
        return { request, stop };
      }
      await Bun.sleep(100);
    }
    throw new Error(`TLS-backed server failed to start: ${logs.replaceAll(password, "[redacted]")}`);
  } catch (error) { await stop(); throw error; }
}
async function workflow(env: Record<string, string>, label: string) {
  const server = await start(env);
  let id: string;
  try {
    await server.request("/health/ready");
    const run = await (await server.request("/flows/run", { source: flow })).json();
    assert.equal(run.status, "success");
    id = run.run_id;
  } finally { await server.stop(); }
  const restarted = await start(env);
  try {
    const record = await (await restarted.request(`/runs/${id!}`)).json();
    assert.equal(record.ctx.answer, 42);
    assert.equal(record.status, "success");
    const replay = await (await restarted.request(`/runs/${id!}/events`)).text();
    assert(replay.includes("event: run_finished"), "Events must survive a server restart");
  } finally { await restarted.stop(); }
  await cli(env, ["inspect", id!], true, `${label}: CLI reads persisted workflow`);
  console.log(`PASS ${label}: TLS-backed serve workflow, restart, state and event replay`);
}

try {
  await Bun.write(join(root, "probe.lua"), flow);
  for (const mode of ["require", "verify-ca", "verify-full"]) {
    await cli(storeEnv("postgres", pgUrl(mode)), ["run", join(root, "probe.lua")], true, `PostgreSQL ${mode}: CLI workflow and migrations`);
  }
  await workflow(storeEnv("postgres", pgUrl("verify-full")), "PostgreSQL verify-full");
  for (const [label, url] of [
    ["plaintext", pgUrl("disable")],
    ["wrong CA", pgUrl("verify-full", "wrong-ca.pem")],
    ["wrong hostname", pgUrl("verify-full", "ca.pem", "127.0.0.1")],
  ]) await cli(storeEnv("postgres", url), ["list"], false, `PostgreSQL rejects ${label}`);

  // The SQL node must find the process-wide TLS provider even when no TLS
  // storage backend installed it, so these runs keep the default (JSON) store.
  for (const mode of ["require", "verify-ca", "verify-full"]) {
    const probe = join(root, `db-query-${mode}.lua`);
    await Bun.write(probe, `local f = Flow.new('tls-db-query'); f:step('answer', nodes.db_query({connection = '${pgUrl(mode)}', query = 'SELECT 1 AS answer'})); return f`);
    const context = finalContext(await cli(baseEnv, ["run", probe], true, `db_query ${mode}: TLS provider is process-wide`));
    assert.deepEqual(context.rows, [{ answer: 1 }], `db_query ${mode}: unexpected rows`);
    assert.equal(context.rows_success, true, `db_query ${mode}: rows_success`);
  }

  const redis = `rediss://:${password}@localhost:${redisPort}/0`;
  await workflow(storeEnv("redis", redis), "Redis verified TLS");
  for (const [label, url, ca] of [
    ["plaintext", redis.replace("rediss:", "redis:"), "ca.pem"],
    ["wrong CA", redis, "wrong-ca.pem"],
    ["wrong hostname", redis.replace("localhost", "127.0.0.1"), "ca.pem"],
    ["insecure bypass", `${redis}#insecure`, "ca.pem"],
  ]) await cli(storeEnv("redis", url, ca), ["list"], false, `Redis rejects ${label}`);
  console.log("TLS storage acceptance passed; no shared or production services were used.");
} catch (error) {
  if (!interrupted()) throw error;
} finally {
  if (interrupted()) await shutdown();
  await cleanup();
}
