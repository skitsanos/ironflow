// IF-164: the release profile and the optional feature set are part of the
// shipped-binary contract; these tests pin them so a manifest edit cannot
// silently grow the default binary or make a feature flag decorative again.
import { expect, test } from "bun:test";
import { join } from "node:path";

const repository = join(import.meta.dir, "../..");

type Manifest = {
  features: Record<string, string[]>;
  dependencies: Record<string, string | { version?: string; optional?: boolean }>;
  profile?: Record<string, Record<string, unknown>>;
};

async function manifest(): Promise<Manifest> {
  return Bun.TOML.parse(await Bun.file(join(repository, "Cargo.toml")).text()) as Manifest;
}

test("release profile strips symbols and links with LTO, keeping unwinding panics", async () => {
  const { profile } = await manifest();
  expect(profile?.release).toEqual({ strip: true, lto: "fat", "codegen-units": 1 });
  // `catch_unwind` in the executor coordinator depends on the default panic strategy.
  expect(profile?.release?.panic).toBeUndefined();
  expect(Object.keys(profile ?? {})).toEqual(["release"]);
});

test("pdf-render gates the pdfium dependency and cjk-fonts is opt-in", async () => {
  const { features, dependencies } = await manifest();
  const pdfium = dependencies["pdfium-render"];
  expect(typeof pdfium === "object" && pdfium.optional).toBeTrue();
  expect(features["pdf-render"]).toEqual(["dep:pdfium-render"]);
  expect(features["cjk-fonts"]).toEqual([]);
  expect(features.default).toEqual(["pdf-render"]);
  expect(features.default).not.toContain("cjk-fonts");
});

test("full release variants and the container image carry every optional feature", async () => {
  const release = await Bun.file(join(repository, ".github/workflows/release.yml")).text();
  expect(release).toContain("matrix.suffix == '-full' && '--features postgres,redis,cjk-fonts'");
  const dockerfile = await Bun.file(join(repository, "Dockerfile")).text();
  expect(dockerfile.match(/^ARG FEATURES="postgres redis cjk-fonts"$/gm)).toHaveLength(2);
  const ci = await Bun.file(join(repository, ".github/workflows/ci.yml")).text();
  expect(ci).toContain("--features postgres,redis,cjk-fonts");
});
