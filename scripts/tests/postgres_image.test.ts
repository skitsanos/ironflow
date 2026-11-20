import { expect, test } from "bun:test";
import { join } from "node:path";
import { postgresImage } from "../storage_tls/fixture.ts";

// IF-263: the integration gate, CI, the live TLS fixture and the replica
// stack must run one PostgreSQL image, and Renovate must see every reference.
const repository = join(import.meta.dir, "../..");
const sources = {
  gate: "scripts/integration_gate.sh",
  ci: ".github/workflows/ci.yml",
  fixture: "scripts/storage_tls/fixture.ts",
  replica: "deploy/replica/docker-compose.yml",
} as const;

const read = (path: string) => Bun.file(join(repository, path)).text();
type Services = Record<string, { image?: string }>;

// Every literal `postgres:<tag>` image reference, skipping URLs such as
// postgres://postgres:secret@host and YAML keys such as `postgres:`.
function references(text: string): string[] {
  return [...text.matchAll(/(?<![\w/.:-])postgres:([A-Za-z0-9][\w.-]*)/g)].map((match) => match[0]);
}

// The image each source actually starts.
async function runtimeImages(): Promise<Record<keyof typeof sources, string | undefined>> {
  const gate = await read(sources.gate);
  const variable = gate.match(/^postgres_image="([^"]+)"$/m)?.[1];
  const pulled = /^docker pull "\$postgres_image"$/m.test(gate);
  const started = /^ {2}-p 127\.0\.0\.1::5432 "\$postgres_image" >\/dev\/null$/m.test(gate);
  const ci = Bun.YAML.parse(await read(sources.ci)) as { jobs: Record<string, { services?: Services }> };
  const compose = Bun.YAML.parse(await read(sources.replica)) as { services: Services };
  return {
    gate: pulled && started ? variable : undefined,
    ci: ci.jobs["rust-features"]?.services?.postgres?.image,
    fixture: postgresImage,
    replica: compose.services.postgres?.image,
  };
}

test("the gate, CI, the TLS fixture and the replica stack run one PostgreSQL major tag", async () => {
  const images = await runtimeImages();
  const expected = images.replica;
  expect(expected).toMatch(/^postgres:\d+-alpine$/);
  expect(images).toEqual({ gate: expected, ci: expected, fixture: expected, replica: expected });
  for (const path of Object.values(sources)) {
    expect(new Set(references(await read(path)))).toEqual(new Set([expected!]));
  }
});

test("the reference scan ignores PostgreSQL URLs and YAML keys but catches a second image", () => {
  const text = 'url="postgres://postgres:$secret@db/x"\nservices:\n  postgres:\n    image: postgres:19-alpine\n';
  expect(references(text)).toEqual(["postgres:19-alpine"]);
  expect(references(`${text}docker run postgres:latest\n`)).toEqual(["postgres:19-alpine", "postgres:latest"]);
  expect(references("DATABASE_URL: postgres://postgres:ironflow-1@127.0.0.1/x")).toEqual([]);
});

type CustomManager = {
  managerFilePatterns: string[];
  matchStrings: string[];
  depNameTemplate?: string;
  datasourceTemplate?: string;
};

// Renovate's regex manager: the first manager whose file pattern matches
// extracts named groups from the file text.
async function regexExtractions(path: string) {
  const config = await Bun.file(join(repository, "renovate.json")).json();
  const managers = (config.customManagers as CustomManager[]).filter((manager) =>
    manager.managerFilePatterns.some((pattern) => new RegExp(pattern.slice(1, -1)).test(path)));
  const text = await read(path);
  return managers.flatMap((manager) => manager.matchStrings.flatMap((source) =>
    [...text.matchAll(new RegExp(source, "g"))].map((match) => ({
      depName: match.groups?.depName ?? manager.depNameTemplate,
      currentValue: match.groups?.currentValue,
      datasource: manager.datasourceTemplate,
    }))));
}

test("Renovate's regex manager reads the PostgreSQL tag from the gate and the TLS fixture", async () => {
  const [name, tag] = postgresImage.split(":");
  for (const path of [sources.gate, sources.fixture]) {
    expect(await regexExtractions(path)).toEqual([{ depName: name, currentValue: tag, datasource: "docker" }]);
  }
});
