import { describe, expect, test } from "bun:test";
import { renderCanary } from "../../deploy/openshift/render";

const marker = "invalid.invalid/ironflow:replace-with-immutable-digest";
const digest = `sha256:${"a".repeat(64)}`;
const image = `ghcr.io/skitsanos/ironflow@${digest}`;

describe("OpenShift canary renderer", () => {
  test("replaces the only marker with an immutable GHCR reference", () => {
    expect(renderCanary(`image: ${marker}\n`, image)).toBe(
      `image: ${image}\n`,
    );
  });

  test("rejects tags, foreign repositories, and malformed digests", () => {
    expect(() => renderCanary(`image: ${marker}\n`, "ghcr.io/skitsanos/ironflow:develop")).toThrow();
    expect(() => renderCanary(`image: ${marker}\n`, `ghcr.io/other/ironflow@${digest}`)).toThrow();
    expect(() => renderCanary(`image: ${marker}\n`, "ghcr.io/skitsanos/ironflow@sha256:abc")).toThrow();
  });

  test("fails when the template marker is missing or ambiguous", () => {
    expect(() => renderCanary("kind: Deployment\n", image)).toThrow();
    expect(() => renderCanary(`${marker}\n${marker}\n`, image)).toThrow();
  });
});
