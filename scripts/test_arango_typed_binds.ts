// Run: bun --no-env-file scripts/test_arango_typed_binds.ts [path/to/ironflow]
// Uses only an already-local arangodb:latest image and a disposable local server.
// Never loads .env or inherits database/storage credentials. No Cargo invocation.
import assert from "node:assert/strict";
import { randomBytes, randomUUID } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const repository = resolve(import.meta.dir, "..");
const binary = resolve(process.argv[2] ?? join(repository, "target/debug/ironflow"));
const id = randomUUID();
const container = `ironflow-if138-${id}`;
const label = `com.ironflow.if138=${id}`;
const password = randomBytes(32).toString("hex");
const authorization = `Basic ${Buffer.from(`root:${password}`).toString("base64")}`;
const dockerEnv: Record<string, string> = {};
for (const key of ["PATH", "HOME", "DOCKER_HOST", "DOCKER_CONTEXT", "DOCKER_CONFIG", "DOCKER_TLS_VERIFY", "DOCKER_CERT_PATH"]) {
    if (process.env[key]) dockerEnv[key] = process.env[key]!;
}
const stop = new AbortController();
const children = new Set<ReturnType<typeof Bun.spawn>>();
let root: string | undefined;
let containerId: string | undefined;
let volumes: string[] = [];

function redact(message: string): string {
    return message.replaceAll(password, "[REDACTED]").replaceAll(authorization, "[REDACTED]");
}

async function command(args: string[], env = dockerEnv, cleanup = false) {
    if (!cleanup) stop.signal.throwIfAborted();
    const child = Bun.spawn(args, {
        cwd: root ?? tmpdir(), env, stdout: "pipe", stderr: "pipe",
    });
    children.add(child);
    const timer = setTimeout(() => child.kill("SIGKILL"), 60_000);
    try {
        const [stdout, stderr, code] = await Promise.all([
            new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited,
        ]);
        if (!cleanup) stop.signal.throwIfAborted();
        assert.equal(code, 0, `${args[0]} exited ${code}: ${redact(stderr)}`);
        return stdout.trim();
    } finally {
        clearTimeout(timer);
        children.delete(child);
    }
}

function interrupt() {
    stop.abort(new Error("Verification interrupted"));
    for (const child of children) child.kill("SIGTERM");
}
process.once("SIGINT", interrupt);
process.once("SIGTERM", interrupt);

async function cleanup() {
    let failure: unknown;
    try {
        // The ownership label and UUID name must both match before removal.
        const owned = await command([
            "docker", "ps", "-aq", "--no-trunc", "--filter", `name=^/${container}$`,
            "--filter", `label=${label}`,
        ], dockerEnv, true);
        if (owned) {
            assert(!owned.includes("\n"), "Ownership filter matched multiple containers");
            if (containerId) assert.equal(owned, containerId);
            await command(["docker", "rm", "--force", "--volumes", owned], dockerEnv, true);
            assert.equal(await command(["docker", "ps", "-aq", "--filter", `id=${owned}`], dockerEnv, true), "");
            for (const volume of volumes) {
                const remaining = await command([
                    "docker", "volume", "ls", "--format", "{{.Name}}", "--filter", `name=${volume}`,
                ], dockerEnv, true);
                assert(!remaining.split("\n").includes(volume), "Owned anonymous volume remains");
            }
            console.log(`PASS cleanup: owned container and ${volumes.length} anonymous volumes removed`);
        }
    } catch (error) {
        failure = error;
    } finally {
        if (root) {
            await rm(root, { recursive: true, force: true });
            console.log("PASS cleanup: unique temporary directory removed");
        }
    }
    if (failure) throw failure;
}

async function verify() {
    assert(await Bun.file(binary).exists(), "Provide an existing compiled IronFlow binary");
    const contextHost = await command(["docker", "context", "inspect", "--format", "{{.Endpoints.docker.Host}}"]);
    const host = dockerEnv.DOCKER_CONTEXT ? contextHost : (dockerEnv.DOCKER_HOST ?? contextHost);
    assert(host.startsWith("unix://") || host.startsWith("npipe://"), "Verification requires a local Docker socket");
    const image = JSON.parse(await command([
        "docker", "image", "inspect", "arangodb:latest", "--format",
        '{"id":{{json .Id}},"os":{{json .Os}},"architecture":{{json .Architecture}}}',
    ]));
    root = await mkdtemp(join(tmpdir(), "ironflow-if138-"));
    const cliEnv: Record<string, string> = {
        PATH: process.env.PATH ?? "", HOME: root, TMPDIR: root, NO_COLOR: "1",
        IRONFLOW_STORE: "json", IRONFLOW_EVENT_STORE: "json",
        ARANGODB_USERNAME: "root", ARANGODB_PASSWORD: password,
    };
    const ironflowVersion = await command([binary, "--version"], cliEnv);
    containerId = await command([
        "docker", "create", "--pull=never", "--name", container, "--label", label,
        "--publish", "127.0.0.1::8529", "--memory", "1g", "--cpus", "2",
        "--env", "ARANGO_ROOT_PASSWORD", image.id,
    ], { ...dockerEnv, ARANGO_ROOT_PASSWORD: password });
    const mounts = JSON.parse(await command(["docker", "inspect", "--format", "{{json .Mounts}}", containerId]));
    volumes = mounts.filter((mount: { Type: string }) => mount.Type === "volume")
        .map((mount: { Name: string }) => mount.Name);
    await command(["docker", "start", containerId]);
    const ports = JSON.parse(await command([
        "docker", "inspect", "--format", "{{json .NetworkSettings.Ports}}", containerId,
    ]));
    const bindings = ports["8529/tcp"];
    assert.equal(bindings.length, 1);
    assert.equal(bindings[0].HostIp, "127.0.0.1");
    const port = Number(bindings[0].HostPort);
    assert(Number.isInteger(port) && port > 0 && port <= 65535);
    const url = `http://127.0.0.1:${port}`;
    cliEnv.ARANGODB_URL = url;
    const database = `if138_${id.replaceAll("-", "")}`;
    const collection = `docs_${id.replaceAll("-", "")}`;
    cliEnv.ARANGODB_DATABASE = database;

    async function api(path: string, body?: unknown) {
        const response = await fetch(`${url}${path}`, {
            method: body === undefined ? "GET" : "POST", redirect: "error",
            headers: { authorization, "content-type": "application/json" },
            body: body === undefined ? undefined : JSON.stringify(body),
            signal: AbortSignal.any([stop.signal, AbortSignal.timeout(5000)]),
        });
        assert(response.ok, `${path}: HTTP ${response.status}`);
        const result = await response.json();
        assert(!result.error, `${path}: ArangoDB error ${result.errorNum}`);
        return result;
    }

    let version: { version: string; server: string } | undefined;
    const deadline = Date.now() + 90_000;
    while (Date.now() < deadline) {
        stop.signal.throwIfAborted();
        try { version = await api("/_api/version"); break; } catch {
            await Bun.sleep(500);
        }
    }
    assert(version?.version, "Disposable ArangoDB did not become ready within 90 seconds");
    console.log(JSON.stringify({ ironflow: ironflowVersion, arangodb: version.version,
        image: "arangodb:latest", imageId: image.id, platform: `${image.os}/${image.architecture}`,
        bun: Bun.version, container, endpoint: url }));
    const unauthenticated = await fetch(`${url}/_api/version`, {
        redirect: "error", signal: AbortSignal.timeout(5000),
    });
    assert.equal(unauthenticated.status, 401, "Disposable server must require authentication");
    await unauthenticated.arrayBuffer();
    await api("/_api/database", { name: database });
    await api(`/_db/${database}/_api/collection`, { name: collection });

    const flow = join(root, "typed_ingestion.lua");
    await Bun.write(flow, `local flow = Flow.new("if138_live_ingestion")
flow:step("ingest", nodes.arangodb_aql({
    query = [[FOR d IN @docs FILTER @enabled LIMIT @limit
        UPSERT { _key: d._key } INSERT d UPDATE d IN @@collection
        RETURN { key: NEW._key, value: NEW.value, inserted: OLD == null }]],
    bindVars_key = "ingest_binds", batchSize = 10, output_key = "ingested"
}))
return flow
`);
    async function cli(path: string, binds?: unknown) {
        await command([binary, "validate", path, "--strict"], cliEnv);
        const args = [binary, "run", path, "--store-dir", join(root!, `runs-${randomUUID()}`)];
        if (binds !== undefined) args.push("--context", JSON.stringify({ ingest_binds: binds }));
        const stdout = await command(args, cliEnv);
        assert(stdout.includes("Status: success"), "IronFlow workflow did not succeed");
        const marker = "\nContext:\n";
        const offset = stdout.lastIndexOf(marker);
        assert(offset >= 0, "IronFlow did not return its final context");
        return JSON.parse(stdout.slice(offset + marker.length));
    }
    const bind = (value: number, enabled = true) => ({
        docs: [{ _key: "one", value }, { _key: "two", value }, { _key: "excluded", value }],
        limit: 2, enabled, "@collection": collection,
    });
    const inserted = await cli(flow, bind(1));
    assert.equal(inserted.ingested_count, 2);
    assert.equal(inserted.ingested_has_more, false);
    assert.deepEqual(inserted.ingested_result.map((row: { key: string }) => row.key).sort(), ["one", "two"]);
    assert(inserted.ingested_result.every((row: { value: number; inserted: boolean }) => row.value === 1 && row.inserted === true));
    console.log("PASS first CLI UPSERT: 2 inserts; numeric limit excludes third document");

    const updated = await cli(flow, bind(2));
    assert.equal(updated.ingested_count, 2);
    assert.equal(updated.ingested_has_more, false);
    assert(updated.ingested_result.every((row: { value: number; inserted: boolean }) => row.value === 2 && row.inserted === false));
    console.log("PASS second CLI UPSERT: 2 updates, not inserts");

    const disabled = await cli(flow, bind(99, false));
    assert.equal(disabled.ingested_count, 0);
    assert.equal(disabled.ingested_has_more, false);
    assert.deepEqual(disabled.ingested_result, []);
    const stored = await api(`/_db/${database}/_api/cursor`, {
        query: "FOR d IN @@collection SORT d._key RETURN { key: d._key, value: d.value }",
        bindVars: { "@collection": collection }, batchSize: 10,
    });
    assert.equal(stored.hasMore, false);
    assert.deepEqual(stored.result, [{ key: "one", value: 2 }, { key: "two", value: 2 }]);
    console.log("PASS persisted collection: exactly 2 documents, no duplicates; false flag prevents writes");

    const example = await cli(join(repository, "examples/12-arangodb/aql_typed_bind_vars.lua"));
    assert.equal(example.typed_count, 2);
    assert.equal(example.typed_has_more, false);
    assert.deepEqual(example.typed_result, [
        { _key: "one", name: "First document", source: "typed-example" },
        { _key: "two", name: "Second document", source: "typed-example" },
    ]);
    console.log("PASS read-only typed example: real CLI, document array, numeric limit, boolean and object binds");
}

try {
    try { await verify(); } finally { await cleanup(); }
    console.log("IF-138 live ArangoDB acceptance passed; only disposable resources were used.");
} catch (error) {
    console.error(redact(error instanceof Error ? error.message : String(error)));
    process.exitCode = 1;
} finally {
    process.removeListener("SIGINT", interrupt);
    process.removeListener("SIGTERM", interrupt);
}
