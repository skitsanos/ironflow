import { describe, expect, test } from "bun:test";
import { formatPullRequest, parsePullRequests } from "../check_incoming_prs";

describe("incoming pull requests", () => {
  test("parses the GitHub response", () => {
    const pullRequests = parsePullRequests(JSON.stringify([{
      number: 42,
      title: "Bound queue growth",
      headRefName: "fix/queue",
      isDraft: false,
      mergeable: "MERGEABLE",
      reviewDecision: "APPROVED",
      url: "https://github.test/pull/42",
    }]));
    expect(pullRequests).toHaveLength(1);
    expect(formatPullRequest(pullRequests[0])).toContain("#42 Bound queue growth");
    expect(formatPullRequest(pullRequests[0])).toContain("review APPROVED");
  });

  test("accepts an empty list and rejects malformed responses", () => {
    expect(parsePullRequests("[]")).toEqual([]);
    expect(() => parsePullRequests("{}")) .toThrow("unexpected PR response");
    expect(() => parsePullRequests('[{"number": 1}]')).toThrow("missing required fields");
  });
});
