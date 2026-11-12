// IF-204: the notices file that ships in every release archive and in the
// container image is rendered from the gate's own shipped-graph definition.
// These tests run without Cargo: the real-graph freshness check is the
// `check` command that runs next to the license gate.
import { describe, expect, test } from "bun:test";
import { join } from "node:path";
import { ALLOWED_LICENSES, shippedPackages, type Metadata } from "../check_licenses";
import {
  LICENSE_PREFERENCE,
  NOTICES_FILE,
  appliedLicenses,
  compareNotices,
  licenseText,
  renderNotices,
} from "../third_party_notices";

const repository = join(import.meta.dir, "../..");

type Fixture = { name: string; version?: string; license?: string; repository?: string };

function metadata(
  packages: Fixture[],
  edges: { from: string; to: string; kind: null | "build" | "dev" }[],
): Metadata {
  const id = (pkg: Fixture) => `registry+https://example.invalid#${pkg.name}@${pkg.version ?? "1.0.0"}`;
  const root: Fixture = { name: "root", license: "MIT" };
  const all = [root, ...packages];
  const byName = (name: string) => all.filter((pkg) => pkg.name === name);
  return {
    packages: all.map((pkg) => ({
      id: id(pkg),
      name: pkg.name,
      version: pkg.version ?? "1.0.0",
      license: pkg.license ?? null,
      license_file: pkg.license ? null : "LICENSE",
      repository: pkg.repository ?? null,
    })),
    resolve: {
      root: id(root),
      nodes: all.map((pkg) => ({
        id: id(pkg),
        deps: edges
          .filter((edge) => edge.from === pkg.name)
          .flatMap((edge) => byName(edge.to).map((target) => ({ pkg: id(target), dep_kinds: [{ kind: edge.kind }] }))),
      })),
    },
  };
}

const graph = metadata(
  [
    { name: "dual", license: "MIT OR Apache-2.0", repository: "https://example.invalid/dual" },
    { name: "mozilla", license: "MPL-2.0", repository: "https://example.invalid/mozilla" },
    { name: "bare", license: "MIT" },
    { name: "devonly", license: "GPL-3.0-only", repository: "https://example.invalid/devonly" },
    { name: "both", license: "(MIT OR Apache-2.0) AND Unicode-3.0", repository: "https://example.invalid/both" },
    { name: "twice", version: "0.1.0", license: "BSD-3-Clause", repository: "https://example.invalid/twice" },
    { name: "twice", version: "0.2.0", license: "BSD-3-Clause", repository: "https://example.invalid/twice" },
    { name: "reviewed", repository: "https://example.invalid/reviewed" },
  ],
  [
    { from: "root", to: "dual", kind: null },
    { from: "root", to: "mozilla", kind: null },
    { from: "root", to: "devonly", kind: "dev" },
    { from: "root", to: "both", kind: "build" },
    { from: "dual", to: "bare", kind: null },
    { from: "dual", to: "twice", kind: null },
    { from: "mozilla", to: "reviewed", kind: null },
  ],
);
const exceptions = { "reviewed@1.0.0": "Reviewed: the license file is the ISC text." };

/// Rows of the crates table only; the MPL-2.0 table repeats some crates.
function rows(notices: string): string[] {
  const start = notices.indexOf("## Crates");
  const end = notices.indexOf("\n## ", start + 1);
  return notices
    .slice(start, end === -1 ? undefined : end)
    .split("\n")
    .filter((line) => /^\| [A-Za-z0-9_-]+ \| \d/.test(line));
}

describe("license choice", () => {
  test("the preference order is exactly the gate's allowlist", () => {
    expect(new Set(LICENSE_PREFERENCE)).toEqual(ALLOWED_LICENSES);
    expect(new Set(LICENSE_PREFERENCE).size).toBe(LICENSE_PREFERENCE.length);
    expect(LICENSE_PREFERENCE[0]).toBe("MIT");
  });

  test("OR takes the preferred allowed option, AND keeps every operand, others are never chosen", () => {
    expect(appliedLicenses("MIT OR Apache-2.0")).toEqual(["MIT"]);
    expect(appliedLicenses("Apache-2.0/MIT")).toEqual(["MIT"]);
    expect(appliedLicenses("Apache-2.0 OR BSL-1.0")).toEqual(["Apache-2.0"]);
    expect(appliedLicenses("MIT OR Apache-2.0 OR LGPL-2.1-or-later")).toEqual(["MIT"]);
    expect(appliedLicenses("Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT")).toEqual(["MIT"]);
    expect(appliedLicenses("(MIT OR Apache-2.0) AND Unicode-3.0")).toEqual(["MIT", "Unicode-3.0"]);
    expect(appliedLicenses("Apache-2.0 AND ISC")).toEqual(["Apache-2.0", "ISC"]);
    expect(appliedLicenses("MIT AND BSD-3-Clause")).toEqual(["MIT", "BSD-3-Clause"]);
    expect(appliedLicenses("CC0-1.0 OR MIT-0")).toEqual(["MIT-0"]);
    expect(appliedLicenses("Apache-2.0 WITH LLVM-exception")).toEqual(["Apache-2.0 WITH LLVM-exception"]);
    expect(() => appliedLicenses("GPL-3.0-only")).toThrow("GPL-3.0-only");
    expect(() => appliedLicenses("MIT AND GPL-2.0-only")).toThrow();
    expect(() => appliedLicenses("MIT OR")).toThrow();
  });

  test("every allowed license has a normalized canonical text and a missing one fails closed", () => {
    for (const id of ALLOWED_LICENSES) {
      const text = licenseText(id);
      expect(text.length, id).toBeGreaterThan(400);
      expect(text, id).not.toContain("\r");
      expect(text, id).toMatch(/[^\n]\n$/);
      expect(text, id).not.toMatch(/[ \t]+\n/);
    }
    expect(licenseText("Apache-2.0 WITH LLVM-exception")).toContain("LLVM Exceptions to the Apache 2.0 License");
    expect(() => licenseText("Fake-1.0")).toThrow("Fake-1.0");
  });
});

describe("rendered notices", () => {
  test("rendering is deterministic and independent of metadata order", () => {
    const first = renderNotices(graph, { exceptions });
    const second = renderNotices(graph, { exceptions });
    const shuffled: Metadata = {
      packages: [...graph.packages].reverse(),
      resolve: {
        root: graph.resolve.root,
        nodes: [...graph.resolve.nodes].reverse().map((node) => ({ ...node, deps: [...node.deps].reverse() })),
      },
    };
    expect(second).toBe(first);
    expect(renderNotices(shuffled, { exceptions })).toBe(first);
    expect(first.endsWith("\n")).toBeTrue();
  });

  test("every shipped crate appears exactly once with version, expression, applied license and source", () => {
    const notices = renderNotices(graph, { exceptions });
    const shipped = shippedPackages(graph);
    expect(shipped.length).toBe(7);
    expect(rows(notices)).toHaveLength(shipped.length);
    for (const pkg of shipped) {
      const matching = rows(notices).filter((row) => row.startsWith(`| ${pkg.name} | ${pkg.version} |`));
      expect(matching, `${pkg.name}@${pkg.version}`).toHaveLength(1);
    }
    expect(notices).toContain(`${shipped.length} crates`);
    expect(notices).not.toContain("devonly");
    expect(notices).toContain("| dual | 1.0.0 | MIT OR Apache-2.0 | MIT | https://example.invalid/dual |");
    expect(notices).toContain("| both | 1.0.0 | (MIT OR Apache-2.0) AND Unicode-3.0 | MIT, Unicode-3.0 | https://example.invalid/both |");
    expect(notices).toContain("| bare | 1.0.0 | MIT | MIT | https://crates.io/crates/bare/1.0.0 |");
    expect(notices).toContain("| twice | 0.1.0 |");
    expect(notices).toContain("| twice | 0.2.0 |");
    expect(notices).toContain("| reviewed | 1.0.0 | — | reviewed exception |");
    expect(notices).toContain("`reviewed@1.0.0`: Reviewed: the license file is the ISC text.");
  });

  test("MPL-2.0 crates carry a source-availability note and each applied license text appears once", () => {
    const notices = renderNotices(graph, { exceptions });
    const availability = notices.slice(notices.indexOf("## MPL-2.0 source availability"), notices.indexOf("## Reviewed exceptions"));
    expect(availability).toContain("section 3.2");
    expect(availability).toContain("| mozilla | 1.0.0 | https://example.invalid/mozilla | https://crates.io/crates/mozilla/1.0.0 |");
    expect(availability).not.toContain("| dual |");
    for (const id of ["MIT", "MPL-2.0", "Unicode-3.0", "BSD-3-Clause"]) {
      expect(notices.match(new RegExp(`^### ${id.replace(/[.-]/g, "\\$&")}$`, "gm")), id).toHaveLength(1);
      expect(notices).toContain(licenseText(id));
    }
    expect(notices).not.toContain("### Apache-2.0");
    expect(notices).toContain("Used by 3 crates.");
    const withoutMozilla = metadata(
      [{ name: "only", license: "MIT", repository: "https://example.invalid/only" }],
      [{ from: "root", to: "only", kind: null }],
    );
    expect(renderNotices(withoutMozilla)).not.toContain("## MPL-2.0 source availability");
    expect(renderNotices(withoutMozilla)).not.toContain("## Reviewed exceptions");
  });

  test("a crate without an allowed license or without a stored license text fails closed", () => {
    const copyleft = metadata(
      [{ name: "copyleft", license: "GPL-3.0-only", repository: "https://example.invalid/copyleft" }],
      [{ from: "root", to: "copyleft", kind: null }],
    );
    expect(() => renderNotices(copyleft)).toThrow("copyleft 1.0.0");
    const unlicensed = metadata([{ name: "unlicensed" }], [{ from: "root", to: "unlicensed", kind: null }]);
    expect(() => renderNotices(unlicensed)).toThrow("unlicensed 1.0.0");
    const novel = metadata(
      [{ name: "novel", license: "Fake-1.0", repository: "https://example.invalid/novel" }],
      [{ from: "root", to: "novel", kind: null }],
    );
    expect(() => renderNotices(novel, { preference: [...LICENSE_PREFERENCE, "Fake-1.0"] })).toThrow("Fake-1.0");
  });

  test("the freshness check names the regeneration command only when the file differs", () => {
    const rendered = renderNotices(graph, { exceptions });
    expect(compareNotices(rendered, rendered)).toBeNull();
    expect(compareNotices(rendered, null)).toContain("scripts/third_party_notices.ts write");
    expect(compareNotices(rendered, rendered.replace("| dual |", "| dual2 |"))).toContain("does not match");
  });
});

describe("committed notices", () => {
  test("every listed crate is a locked package and the stated count matches the rows", async () => {
    const notices = await Bun.file(join(repository, NOTICES_FILE)).text();
    const lockfile = Bun.TOML.parse(await Bun.file(join(repository, "Cargo.lock")).text()) as {
      package: { name: string; version: string }[];
    };
    const locked = new Set(lockfile.package.map((pkg) => `${pkg.name}@${pkg.version}`));
    const listed = rows(notices);
    expect(listed.length).toBeGreaterThan(100);
    expect(notices).toContain(`${listed.length} crates`);
    for (const row of listed) {
      const [name, version, , used] = row.slice(2, -2).split(" | ");
      expect(locked.has(`${name}@${version}`), row).toBeTrue();
      for (const id of used.split(", ")) {
        expect(id === "reviewed exception" || ALLOWED_LICENSES.has(id), row).toBeTrue();
      }
    }
  });
});
