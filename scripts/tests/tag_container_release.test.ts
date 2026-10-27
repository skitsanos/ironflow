import { expect, test } from "bun:test";
import { releaseVersion, requireDigest, requireVersionTarget } from "../tag_container_release";

test("version tags accept stable releases and reject prereleases and command-like input", () => {
  expect(releaseVersion("v1.20.0")).toBe("1.20.0");
  for (const tag of ["1.20.0", "v01.20.0", "v1.20.1-dev.1", "v1.20.0\n", "v1.20.0;echo bad"]) {
    expect(() => releaseVersion(tag)).toThrow();
  }
});

test("only full sha256 digests are accepted", () => {
  const digest = `sha256:${"a".repeat(64)}`;
  expect(requireDigest(digest)).toBe(digest);
  for (const value of ["latest", "sha256:abc", `sha256:${"a".repeat(64)}\n`]) {
    expect(() => requireDigest(value)).toThrow();
  }
});

test("version tag promotion is idempotent and refuses an existing different image", () => {
  const digest = `sha256:${"a".repeat(64)}`;
  expect(() => requireVersionTarget(undefined, digest)).not.toThrow();
  expect(() => requireVersionTarget(digest, digest)).not.toThrow();
  expect(() => requireVersionTarget(`sha256:${"b".repeat(64)}`, digest)).toThrow("refusing to overwrite");
});
