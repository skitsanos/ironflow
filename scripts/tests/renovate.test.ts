import { expect, test } from "bun:test";
import { join } from "node:path";

test("AWS deferrals exclude only the incompatible releases from PR 182", async () => {
  const config = await Bun.file(join(import.meta.dir, "../../renovate.json")).json();
  const deferred = [
    ["aws-sdk-s3", "1.147.0", "1.146.1", "1.147.1"],
    ["aws-sdk-s3vectors", "1.37.0", "1.36.0", "1.37.1"],
    ["aws-smithy-types", "1.7.0", "1.6.3", "1.7.1"],
  ];
  expect(config.packageRules.filter(rule => rule.allowedVersions).length).toBe(deferred.length);
  for (const [name, blocked, previous, next] of deferred) {
    const rule = config.packageRules.find(rule => rule.matchPackageNames.includes(name));
    expect(rule.matchManagers).toEqual(["cargo"]);
    expect(rule.matchPackageNames).toEqual([name]);
    expect(rule.enabled).toBeUndefined();
    expect(rule.allowedVersions).toBe(`!/^${blocked.replaceAll(".", "\\.")}$/`);
    const excluded = new RegExp(rule.allowedVersions.slice(2, -1));
    expect(excluded.test(blocked)).toBeTrue();
    expect(excluded.test(previous)).toBeFalse();
    expect(excluded.test(next)).toBeFalse();
  }
});
