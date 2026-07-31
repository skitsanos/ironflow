import { describe, expect, test } from "bun:test";
import {
  compareVersions,
  formatVersion,
  lockVersion,
  manifestVersion,
  nextVersion,
  parseVersion,
  replaceLockVersion,
  replaceManifestVersion,
  stableVersion,
  validateDevelopmentVersion,
} from "../development_version";

const manifest = (version: string) => `[package]\nname = "ironflow"\nversion = "${version}"\n\n[dependencies]\n`;
const lock = (version: string) => `version = 4\n\n[[package]]\nname = "other"\nversion = "9.0.0"\n\n[[package]]\nname = "ironflow"\nversion = "${version}"\ndependencies = []\n`;

describe("development versions", () => {
  test("parses and formats stable and development versions", () => {
    expect(formatVersion(parseVersion("1.15.0"))).toBe("1.15.0");
    expect(formatVersion(parseVersion("1.16.0-dev.3"))).toBe("1.16.0-dev.3");
    expect(() => parseVersion("1.16.0-alpha.1")).toThrow("unsupported version");
  });

  test("bumps a stable base and advances an integration batch", () => {
    const stable = parseVersion("1.15.0");
    expect(formatVersion(nextVersion(stable, "patch"))).toBe("1.15.1-dev.1");
    expect(formatVersion(nextVersion(stable, "minor"))).toBe("1.16.0-dev.1");
    expect(formatVersion(nextVersion(stable, "major"))).toBe("2.0.0-dev.1");
    expect(formatVersion(nextVersion(parseVersion("1.16.0-dev.1"), "next"))).toBe("1.16.0-dev.2");
    expect(() => nextVersion(stable, "next")).toThrow("requires an existing");
  });

  test("finalizes a development candidate without changing its base", () => {
    expect(formatVersion(stableVersion(parseVersion("1.16.0-dev.4")))).toBe("1.16.0");
    expect(() => stableVersion(parseVersion("1.16.0"))).toThrow("already stable");
  });

  test("orders prereleases below their stable base and above older bases", () => {
    expect(compareVersions(parseVersion("1.16.0-dev.1"), parseVersion("1.15.0"))).toBe(1);
    expect(compareVersions(parseVersion("1.16.0-dev.1"), parseVersion("1.16.0"))).toBe(-1);
    expect(compareVersions(parseVersion("1.16.0-dev.2"), parseVersion("1.16.0-dev.1"))).toBe(1);
  });

  test("updates only the IronFlow manifest and lock package", () => {
    const updatedManifest = replaceManifestVersion(manifest("1.15.0"), "1.16.0-dev.1");
    const updatedLock = replaceLockVersion(lock("1.15.0"), "1.16.0-dev.1");
    expect(manifestVersion(updatedManifest)).toBe("1.16.0-dev.1");
    expect(lockVersion(updatedLock)).toBe("1.16.0-dev.1");
    expect(updatedLock).toContain('name = "other"\nversion = "9.0.0"');
  });

  test("reads a package section at end of file", () => {
    expect(manifestVersion('[package]\nname = "ironflow"\nversion = "2.0.0-dev.1"\n'))
      .toBe("2.0.0-dev.1");
  });

  test("requires matching committed files and a newer development version", () => {
    expect(validateDevelopmentVersion(manifest("1.16.0-dev.1"), lock("1.16.0-dev.1"), manifest("1.15.0")))
      .toBe("1.16.0-dev.1");
    expect(() => validateDevelopmentVersion(manifest("1.15.0"), lock("1.15.0"))).toThrow("prerelease");
    expect(() => validateDevelopmentVersion(manifest("1.16.0-dev.1"), lock("1.15.0"))).toThrow("does not match");
    expect(() => validateDevelopmentVersion(manifest("1.16.0-dev.1"), lock("1.16.0-dev.1"), manifest("1.16.0-dev.1")))
      .toThrow("must be newer");
  });
});
