import { describe, expect, test } from "bun:test";
import {
  ALLOWED_LICENSES,
  LICENSE_EXCEPTIONS,
  evaluate,
  licenseAllowed,
  parseLicense,
  shippedPackages,
  type Metadata,
} from "../check_licenses";

const permissive = new Set(["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "Unicode-DFS-2016"]);

function metadata(
  packages: { name: string; license?: string; licenseFile?: string }[],
  edges: { from: string; to: string; kind: null | "build" | "dev" }[],
): Metadata {
  const id = (name: string) => `registry+https://example.invalid#${name}@1.0.0`;
  return {
    packages: [
      { id: id("root"), name: "root", version: "1.0.0", license: "MIT", license_file: null },
      ...packages.map((pkg) => ({
        id: id(pkg.name),
        name: pkg.name,
        version: "1.0.0",
        license: pkg.license ?? null,
        license_file: pkg.licenseFile ?? null,
      })),
    ],
    resolve: {
      root: id("root"),
      nodes: ["root", ...packages.map((pkg) => pkg.name)].map((name) => ({
        id: id(name),
        deps: edges
          .filter((edge) => edge.from === name)
          .map((edge) => ({ pkg: id(edge.to), dep_kinds: [{ kind: edge.kind }] })),
      })),
    },
  };
}

describe("license expressions", () => {
  test("parses SPDX operators, parentheses, exceptions, and legacy slashes", () => {
    expect(parseLicense("MIT OR Apache-2.0")).toEqual({
      kind: "or",
      items: [{ kind: "id", id: "MIT" }, { kind: "id", id: "Apache-2.0" }],
    });
    expect(parseLicense("(MIT OR Apache-2.0) AND Unicode-DFS-2016")).toEqual({
      kind: "and",
      items: [
        { kind: "or", items: [{ kind: "id", id: "MIT" }, { kind: "id", id: "Apache-2.0" }] },
        { kind: "id", id: "Unicode-DFS-2016" },
      ],
    });
    expect(parseLicense("Apache-2.0 WITH LLVM-exception OR MIT")).toEqual({
      kind: "or",
      items: [{ kind: "id", id: "Apache-2.0 WITH LLVM-exception" }, { kind: "id", id: "MIT" }],
    });
    expect(parseLicense("MIT/Apache-2.0")).toEqual(parseLicense("MIT OR Apache-2.0"));
    expect(parseLicense("Apache-2.0 / MIT")).toEqual(parseLicense("Apache-2.0 OR MIT"));
  });

  test("rejects malformed expressions instead of allowing them", () => {
    for (const malformed of ["", "MIT OR", "(MIT", "MIT AND AND Apache-2.0", "OR MIT"]) {
      expect(() => parseLicense(malformed)).toThrow();
      expect(licenseAllowed(malformed, permissive)).toBe(false);
    }
  });

  test("any allowed alternative satisfies OR and every operand must satisfy AND", () => {
    expect(licenseAllowed("MIT OR Apache-2.0", permissive)).toBe(true);
    expect(licenseAllowed("MIT OR Apache-2.0 OR LGPL-2.1-or-later", permissive)).toBe(true);
    expect(licenseAllowed("Apache-2.0 WITH LLVM-exception OR MIT", permissive)).toBe(true);
    expect(licenseAllowed("(MIT OR Apache-2.0) AND Unicode-DFS-2016", permissive)).toBe(true);
    expect(licenseAllowed("GPL-3.0+", permissive)).toBe(false);
    expect(licenseAllowed("MIT AND GPL-2.0-only", permissive)).toBe(false);
    expect(licenseAllowed("Apache-2.0 WITH Unknown-exception", permissive)).toBe(false);
  });

  test("the repository allowlist contains no copyleft licenses", () => {
    expect(ALLOWED_LICENSES.size).toBeGreaterThan(5);
    for (const license of ALLOWED_LICENSES) {
      expect(license).not.toMatch(/GPL|AGPL|LGPL|SSPL|EUPL|OSL|CC-BY-SA|CC-BY-NC/i);
    }
    for (const rationale of Object.values(LICENSE_EXCEPTIONS)) {
      expect(rationale.trim().length).toBeGreaterThanOrEqual(20);
    }
  });
});

describe("shipped dependency graph", () => {
  test("includes normal and build dependencies but not dev-only dependencies", () => {
    const data = metadata(
      [
        { name: "runtime", license: "MIT" },
        { name: "buildtool", license: "MIT" },
        { name: "devonly", license: "GPL-3.0-only" },
        { name: "transitive", license: "MIT" },
      ],
      [
        { from: "root", to: "runtime", kind: null },
        { from: "root", to: "buildtool", kind: "build" },
        { from: "root", to: "devonly", kind: "dev" },
        { from: "runtime", to: "transitive", kind: null },
      ],
    );
    expect(shippedPackages(data).map((pkg) => pkg.name).sort()).toEqual([
      "buildtool",
      "runtime",
      "transitive",
    ]);
    expect(evaluate(data, permissive, {})).toEqual([]);
  });

  test("copyleft, missing, and malformed licenses are violations", () => {
    const data = metadata(
      [
        { name: "copyleft", license: "GPL-3.0+" },
        { name: "unlicensed", licenseFile: "LICENSE" },
        { name: "broken", license: "MIT OR" },
      ],
      [
        { from: "root", to: "copyleft", kind: null },
        { from: "root", to: "unlicensed", kind: null },
        { from: "root", to: "broken", kind: "build" },
      ],
    );
    const violations = evaluate(data, permissive, {});
    expect(violations).toHaveLength(3);
    expect(violations.join("\n")).toContain("copyleft 1.0.0: license 'GPL-3.0+' is not allowed");
    expect(violations.join("\n")).toContain("unlicensed 1.0.0: no license expression");
    expect(violations.join("\n")).toContain("broken 1.0.0");
  });

  test("reviewed exceptions cover exactly one package version and must stay in use", () => {
    const data = metadata(
      [{ name: "unlicensed", licenseFile: "LICENSE" }],
      [{ from: "root", to: "unlicensed", kind: null }],
    );
    const exceptions = { "unlicensed@1.0.0": "Reviewed: license file is BSD-3-Clause text." };
    expect(evaluate(data, permissive, exceptions)).toEqual([]);
    expect(evaluate(data, permissive, { "unlicensed@0.9.0": "Reviewed: old version only." })).toEqual([
      "unlicensed 1.0.0: no license expression and no reviewed exception",
      "unlicensed@0.9.0: exception is stale because the package is not shipped",
    ]);
  });
});
