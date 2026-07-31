#!/usr/bin/env bun

export type IncomingPullRequest = {
  number: number;
  title: string;
  headRefName: string;
  isDraft: boolean;
  mergeable: string;
  reviewDecision: string;
  url: string;
};

export function parsePullRequests(source: string): IncomingPullRequest[] {
  const parsed: unknown = JSON.parse(source);
  if (!Array.isArray(parsed)) throw new Error("GitHub returned an unexpected PR response");
  return parsed.map((item, index) => {
    if (typeof item !== "object" || item === null) throw new Error(`PR result ${index} is not an object`);
    const pr = item as Record<string, unknown>;
    if (typeof pr.number !== "number" || typeof pr.title !== "string" || typeof pr.url !== "string") {
      throw new Error(`PR result ${index} is missing required fields`);
    }
    return {
      number: pr.number,
      title: pr.title,
      headRefName: typeof pr.headRefName === "string" ? pr.headRefName : "unknown",
      isDraft: pr.isDraft === true,
      mergeable: typeof pr.mergeable === "string" ? pr.mergeable : "UNKNOWN",
      reviewDecision: typeof pr.reviewDecision === "string" && pr.reviewDecision ? pr.reviewDecision : "NONE",
      url: pr.url,
    };
  });
}

export function formatPullRequest(pr: IncomingPullRequest): string {
  const draft = pr.isDraft ? ", draft" : "";
  return `#${pr.number} ${pr.title} (${pr.headRefName}; ${pr.mergeable}; review ${pr.reviewDecision}${draft})\n  ${pr.url}`;
}

function listIncomingPullRequests(): IncomingPullRequest[] {
  const result = Bun.spawnSync([
    "gh",
    "pr",
    "list",
    "--base",
    "develop",
    "--state",
    "open",
    "--json",
    "number,title,headRefName,isDraft,mergeable,reviewDecision,url",
  ], { stdout: "pipe", stderr: "pipe" });
  if (result.exitCode !== 0) {
    const detail = new TextDecoder().decode(result.stderr).trim();
    throw new Error(detail || "gh could not inspect incoming pull requests");
  }
  return parsePullRequests(new TextDecoder().decode(result.stdout));
}

function main(): void {
  const pullRequests = listIncomingPullRequests();
  if (pullRequests.length === 0) {
    console.log("Incoming PR check passed: no open pull requests target develop.");
    return;
  }

  console.error("Push blocked: resolve every open pull request targeting develop first:");
  for (const pullRequest of pullRequests) console.error(formatPullRequest(pullRequest));
  console.error("Merge, close, or integrate and close these PRs before retrying the develop push.");
  process.exit(1);
}

if (import.meta.main) {
  try {
    main();
  } catch (error) {
    console.error(`incoming PR check failed closed: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  }
}
