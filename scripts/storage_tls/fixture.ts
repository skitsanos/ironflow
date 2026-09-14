import { chmod, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { randomBytes, randomUUID } from "node:crypto";

export async function command(args: string[], options: { cwd?: string; env?: Record<string, string>; timeout?: number } = {}) {
  const child = Bun.spawn(args, { cwd: options.cwd, env: options.env, stdout: "pipe", stderr: "pipe" });
  const stdout = new Response(child.stdout).text();
  const stderr = new Response(child.stderr).text();
  let timedOut = false;
  const timer = setTimeout(() => { timedOut = true; child.kill("SIGKILL"); }, options.timeout ?? 120_000);
  try {
    const code = await child.exited;
    return { code, stdout: await stdout, stderr: await stderr, timedOut };
  } finally { clearTimeout(timer); }
}

export async function successful(args: string[], cwd?: string) {
  const result = await command(args, { cwd });
  if (result.code !== 0 || result.timedOut) throw new Error(`${args[0]} ${args[1]} failed: ${result.stderr}`);
  return result.stdout.trim();
}

export async function createFixture() {
  const root = await mkdtemp(join(tmpdir(), "ironflow-tls-"));
  const suffix = randomUUID();
  const postgres = `ironflow-tls-postgres-${suffix}`;
  const redis = `ironflow-tls-redis-${suffix}`;
  const owned: string[] = [];
  const password = randomBytes(24).toString("hex");
  async function cleanup() {
    try {
      if (owned.length) await successful(["docker", "rm", "-f", "-v", ...owned]);
    } finally { await rm(root, { recursive: true, force: true }); }
  }
  try {
    await chmod(root, 0o700);
    for (const name of ["ca", "wrong-ca"]) {
      await successful(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-sha256", "-days", "2", "-subj", `/CN=IronFlow test ${name}`, "-addext", "basicConstraints=critical,CA:TRUE", "-addext", "keyUsage=critical,keyCertSign,cRLSign", "-keyout", `${name}.key`, "-out", `${name}.pem`], root);
    }
    await successful(["openssl", "req", "-new", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=localhost", "-keyout", "server.key", "-out", "server.csr"], root);
    await Bun.write(join(root, "server.ext"), "subjectAltName=DNS:localhost\nextendedKeyUsage=serverAuth\nkeyUsage=digitalSignature,keyEncipherment\nbasicConstraints=CA:FALSE\n");
    await successful(["openssl", "x509", "-req", "-in", "server.csr", "-CA", "ca.pem", "-CAkey", "ca.key", "-CAcreateserial", "-days", "2", "-sha256", "-extfile", "server.ext", "-out", "server.pem"], root);
    await Bun.write(join(root, "pg_hba.conf"), "local all all trust\nhostssl all all 0.0.0.0/0 scram-sha-256\nhostnossl all all 0.0.0.0/0 reject\n");
    await chmod(join(root, "server.key"), 0o600);
    for (const image of ["postgres:latest", "redis:latest"]) await successful(["docker", "pull", image]);
    const mount = `type=bind,src=${root},dst=/tls,readonly`;
    owned.push(postgres);
    await successful(["docker", "run", "-d", "--name", postgres, "-p", "127.0.0.1::5432", "--mount", mount, "-e", `POSTGRES_PASSWORD=${password}`, "-e", "POSTGRES_DB=ironflow_tls", "--entrypoint", "sh", "postgres:latest", "-c", "install -o postgres -g postgres -m 600 /tls/server.key /tmp/ironflow.key\ninstall -o postgres -g postgres -m 644 /tls/server.pem /tmp/ironflow.pem\ninstall -o postgres -g postgres -m 644 /tls/pg_hba.conf /tmp/ironflow-hba.conf\nexec docker-entrypoint.sh postgres -c ssl=on -c ssl_cert_file=/tmp/ironflow.pem -c ssl_key_file=/tmp/ironflow.key -c hba_file=/tmp/ironflow-hba.conf"]);
    owned.push(redis);
    await successful(["docker", "run", "-d", "--name", redis, "-p", "127.0.0.1::6379", "--mount", mount, "--entrypoint", "sh", "redis:latest", "-c", "install -o redis -g redis -m 600 /tls/server.key /tmp/ironflow.key\ninstall -o redis -g redis -m 644 /tls/server.pem /tmp/ironflow.pem\ninstall -o redis -g redis -m 644 /tls/ca.pem /tmp/ironflow-ca.pem\nexec docker-entrypoint.sh redis-server --port 0 --tls-port 6379 --tls-cert-file /tmp/ironflow.pem --tls-key-file /tmp/ironflow.key --tls-ca-cert-file /tmp/ironflow-ca.pem --tls-auth-clients no --requirepass \"$1\"", "fixture", password]);
    async function ready(args: string[], match: string) {
      for (let attempt = 0; attempt < 60; attempt++) {
        const result = await command(args, { timeout: 5000 });
        if (result.code === 0 && result.stdout.includes(match)) return;
        await Bun.sleep(500);
      }
      throw new Error("Disposable TLS store did not become ready");
    }
    await ready(["docker", "exec", postgres, "pg_isready", "-U", "postgres", "-d", "ironflow_tls"], "accepting connections");
    await ready(["docker", "exec", "-e", `REDISCLI_AUTH=${password}`, redis, "redis-cli", "--tls", "--cacert", "/tmp/ironflow-ca.pem", "--sni", "localhost", "PING"], "PONG");
    async function port(name: string, exposed: string) {
      const binding = await successful(["docker", "port", name, exposed]);
      const match = binding.match(/^127\.0\.0\.1:(\d+)$/);
      if (!match) throw new Error("Fixture port must be bound to loopback only");
      return Number(match[1]);
    }
    console.log(await successful(["docker", "exec", postgres, "postgres", "--version"]));
    console.log(await successful(["docker", "exec", redis, "redis-server", "--version"]));
    return { root, password, postgresPort: await port(postgres, "5432/tcp"), redisPort: await port(redis, "6379/tcp"), cleanup };
  } catch (error) { await cleanup(); throw error; }
}
