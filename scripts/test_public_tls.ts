import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

// Opt-in network acceptance, not a default unit test. Network failure alone
// never counts as certificate rejection. No inherited credentials or proxies.
const binary = resolve(process.argv[2] ?? "target/release/ironflow");
assert(await Bun.file(binary).exists(), "Build ironflow before running this probe");
const directory = await mkdtemp(join(tmpdir(), "ironflow-public-tls-"));
let active: ReturnType<typeof Bun.spawn> | undefined;
let cleaning: Promise<void> | undefined;
const cleanup = () => cleaning ??= (async () => {
  if (active && active.exitCode === null) { active.kill(); await active.exited; }
  await rm(directory, { recursive: true, force: true });
})();
for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.once(signal, () => { void cleanup().then(() => process.exit(signal === "SIGINT" ? 130 : 143)); });
}

async function probe(url: string, accepted: boolean) {
  const path = join(directory, "probe.lua");
  await Bun.write(path, `local f = Flow.new('public-tls'); f:step('request', nodes.http_get({url=${JSON.stringify(url)}, proxy_mode='direct', timeout=15})); return f`);
  active = Bun.spawn([binary, "run", path], {
    cwd: directory,
    env: { PATH: process.env.PATH ?? "", HOME: directory, TMPDIR: directory, NO_COLOR: "1", RUST_LOG: "info" },
    stdout: "pipe", stderr: "pipe",
  });
  const child = active;
  let timedOut = false;
  const timer = setTimeout(() => { timedOut = true; child.kill("SIGKILL"); }, 25_000);
  try {
    const [code, stdout, stderr] = await Promise.all([
      child.exited, new Response(child.stdout).text(), new Response(child.stderr).text(),
    ]);
    assert(!timedOut, `${url}: timeout is inconclusive, not a TLS rejection`);
    assert.equal(code === 0, accepted, `${url}: unexpected exit ${code}: ${stdout}\n${stderr}`);
    if (!accepted) {
      assert.match(`${stdout}\n${stderr}`, /certificate|certificateverifyfailed|cert_revoked/i,
        `${url}: failure did not establish certificate rejection`);
    }
    console.log(`PASS IronFlow ${accepted ? "accepts valid TLS" : "rejects compromised chain"}: ${url}`);
  } finally { clearTimeout(timer); active = undefined; }
}

try {
  await probe("https://example.com/", true);
  for (const host of ["dv-g3b", "dv-g3b-aaa", "dv-g3b-noct", "dv-g3b-aaa-noct"]) {
    await probe(`https://${host}-emudhra.we-have-a-problem.com/`, false);
  }
} finally { await cleanup(); }
