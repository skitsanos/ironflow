import { describe, expect, test } from "bun:test";
import {
  parseIssuePage,
  renderCompatibilityIndex,
  renderRegistry,
  validateRoadmap,
  type Issue,
} from "../issues_registry";

const resolvedPage = `---
id: IF-101
title: Bounded example finding
priority: P2
status: resolved
area: Test policy
resolved: 2026-08-04
---
# IF-101 — Bounded example finding

## Outcome and validation

The bounded implementation outcome is recorded here with enough detail to
explain the contract. Focused negative and positive regression tests passed,
and no live service behavior was changed by this example finding.
`;

function issue(overrides: Partial<Issue> = {}): Issue {
  return {
    id: "IF-101",
    title: "Bounded example finding",
    priority: "P2",
    status: "resolved",
    area: "Test policy",
    resolved: "2026-08-04",
    path: "docs/issues/IF-101.md",
    body: resolvedPage,
    ...overrides,
  };
}

describe("issue registry", () => {
  test("parses a resolved issue with canonical metadata and evidence", () => {
    const parsed = parseIssuePage("docs/issues/IF-101.md", resolvedPage);
    expect(parsed.errors).toEqual([]);
    expect(parsed.issue).toMatchObject({
      id: "IF-101",
      priority: "P2",
      status: "resolved",
      resolved: "2026-08-04",
    });
  });

  test("rejects filename, lifecycle, and contract-heading drift", () => {
    const source = resolvedPage
      .replace("status: resolved", "status: in-progress")
      .replace("## Outcome and validation", "## Notes");
    const parsed = parseIssuePage("docs/issues/IF-102.md", source);
    expect(parsed.errors).toContain(
      "docs/issues/IF-102.md: id IF-101 must match filename IF-102",
    );
    expect(parsed.errors).toContain(
      "docs/issues/IF-102.md: unresolved issues cannot set resolved",
    );
    expect(parsed.errors).toContain(
      "docs/issues/IF-102.md: body must contain '## Required outcome and acceptance'",
    );
  });

  test("renders the complete registry but only active root findings", () => {
    const active = issue({
      id: "IF-102",
      title: "Active finding",
      status: "in-progress",
      resolved: undefined,
      path: "docs/issues/IF-102.md",
    });
    const registry = renderRegistry([issue(), active]);
    const compatibility = renderCompatibilityIndex([issue(), active]);

    expect(registry).toContain("Total findings: 2");
    expect(registry).toContain("[IF-101](./IF-101.md)");
    expect(registry).toContain("[IF-102](./IF-102.md)");
    expect(compatibility).not.toContain("IF-101.md");
    expect(compatibility).toContain("[IF-102](docs/issues/IF-102.md)");
    expect(compatibility).toContain("Run the complete integration gate only at branch integration");
  });

  test("accepts active committed roadmap entries", () => {
    const now = issue({
      id: "IF-100",
      title: "Metrics",
      status: "in-progress",
      resolved: undefined,
      path: "docs/issues/IF-100.md",
    });
    const next = issue({
      id: "IF-102",
      title: "Artifacts",
      status: "open",
      resolved: undefined,
      path: "docs/issues/IF-102.md",
    });
    const roadmap = `# IronFlow product roadmap

## Product posture

Bounded product posture.

## Capability maturity

Bounded maturity table.

## Now

### [IF-100](issues/IF-100.md) — Metrics

Current work.

## Next

### [IF-102](issues/IF-102.md) — Artifacts

Accepted follow-up.

## Later

Uncommitted candidates.
`;

    expect(validateRoadmap(roadmap, [now, next])).toEqual([]);
    const emptyNext = roadmap.replace(
      "### [IF-102](issues/IF-102.md) — Artifacts\n\nAccepted follow-up.",
      "_No committed initiative._",
    );
    expect(validateRoadmap(emptyNext, [now])).toEqual([]);
  });

  test("rejects missing horizons, broken targets, duplicates, and resolved work", () => {
    const roadmap = `# IronFlow product roadmap

## Product posture

Posture.

## Capability maturity

Maturity.

## Now

### [IF-101](issues/IF-102.md) — Broken target

### [IF-101](issues/IF-101.md) — Resolved work

### [IF-101](issues/IF-101.md) — Duplicate work

## Next

No committed entry.
`;

    const errors = validateRoadmap(roadmap, [issue()]);
    expect(errors).toContain("docs/ROADMAP.md: missing '## Later' section");
    expect(errors).toContain("docs/ROADMAP.md: roadmap label IF-101 targets IF-102");
    expect(errors).toContain("docs/ROADMAP.md: resolved issue IF-101 cannot remain in Now");
    expect(errors).toContain("docs/ROADMAP.md: duplicate committed roadmap entry IF-101");
    expect(errors).toContain(
      "docs/ROADMAP.md: 'Next' requires an active IF-NNN entry or explicit empty marker",
    );
  });
});
