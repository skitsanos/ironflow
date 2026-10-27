#!/usr/bin/env bun

const repositories = ["ghcr.io/skitsanos/ironflow", "docker.io/skitsanos/ironflow"];
const digestPattern = /^sha256:[a-f0-9]{64}$/;
const revisionPattern = /^[a-f0-9]{40}$/;

export function releaseVersion(tag: string): string {
  if (tag.trim() !== tag || !/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag)) {
    throw new Error("release_tag must be a stable vX.Y.Z Git tag");
  }
  return tag.slice(1);
}

export function requireDigest(value: unknown): string {
  if (typeof value !== "string" || value.length !== 71 || !digestPattern.test(value)) {
    throw new Error("expected a sha256 image digest");
  }
  return value;
}

export function requireVersionTarget(existing: string | undefined, expected: string): void {
  if (existing !== undefined && existing !== expected) {
    throw new Error("release version already points to another digest; refusing to overwrite it");
  }
}

function command(args: string[], allowMissing = false): string | undefined {
  const result = Bun.spawnSync(args, { stdout: "pipe", stderr: "pipe" });
  const stderr = new TextDecoder().decode(result.stderr).trim();
  if (result.exitCode !== 0) {
    if (allowMissing && /manifest unknown|manifest_unknown|:\s*not found\b|unexpected status.*404/i.test(stderr)) {
      return undefined;
    }
    throw new Error(`${args[0]} ${args[1]} failed: ${stderr}`);
  }
  return new TextDecoder().decode(result.stdout).trim();
}

function manifestDigest(reference: string, allowMissing = false): string | undefined {
  const output = command([
    "docker", "buildx", "imagetools", "inspect", reference,
    "--format", "{{json .Manifest}}",
  ], allowMissing);
  if (output === undefined) return undefined;
  return requireDigest(JSON.parse(output).digest);
}

function manifestVersion(revision: string): string {
  const source = command(["git", "show", `${revision}:Cargo.toml`])!;
  return (Bun.TOML.parse(source) as { package: { version: string } }).package.version;
}

async function publishedDigest(reference: string): Promise<string> {
  // A release can finish before the main Container workflow has published.
  for (let attempt = 0; attempt < 20; attempt++) {
    const digest = manifestDigest(reference, true);
    if (digest) return digest;
    if (attempt < 19) await Bun.sleep(15_000);
  }
  throw new Error(`image is not published yet: ${reference}; rerun after Container succeeds`);
}

function sourceRevision(reference: string): string {
  const output = command([
    "docker", "buildx", "imagetools", "inspect", reference,
    "--format", "{{json .Image}}",
  ])!;
  const image = JSON.parse(output);
  const config = image.config ?? image["linux/amd64"]?.config;
  const revision = config?.Labels?.["org.opencontainers.image.revision"];
  if (typeof revision !== "string" || revision.length !== 40 || !revisionPattern.test(revision)) {
    throw new Error("published image has no valid source revision label");
  }
  return revision;
}

async function main(): Promise<void> {
  const tag = process.env.RELEASE_TAG ?? "";
  const version = releaseVersion(tag);
  const explicitDigest = process.env.SOURCE_DIGEST || undefined;
  if (explicitDigest) requireDigest(explicitDigest);

  const releaseRevision = command(["git", "rev-parse", "--verify", `refs/tags/${tag}^{commit}`])!;
  if (!revisionPattern.test(releaseRevision)) throw new Error("invalid release revision");
  command(["git", "merge-base", "--is-ancestor", releaseRevision, "origin/main"]);
  if (manifestVersion(releaseRevision) !== version) {
    throw new Error("Git tag does not match the released Cargo.toml version");
  }

  const source = explicitDigest ? `@${explicitDigest}` : `:sha-${releaseRevision}`;
  const digest = await publishedDigest(`${repositories[0]}${source}`);
  if (explicitDigest && digest !== explicitDigest) throw new Error("source digest changed");
  for (const repository of repositories) {
    if (await publishedDigest(`${repository}${source}`) !== digest) {
      throw new Error("source image digests do not match across registries");
    }
  }

  const revision = sourceRevision(`${repositories[0]}@${digest}`);
  if (!explicitDigest && revision !== releaseRevision) {
    throw new Error("commit image does not match the Git release revision");
  }
  command(["git", "merge-base", "--is-ancestor", revision, "origin/main"]);
  if (manifestVersion(revision) !== version) throw new Error("image source has another package version");
  // An explicit backfill may include documentation/workflow changes after release.
  command([
    "git", "diff", "--quiet", releaseRevision, revision, "--", ".",
    ":(exclude)README.md", ":(exclude)docs/**", ":(exclude).github/**",
  ]);

  const targets = repositories.map(repository => `${repository}:${version}`);
  // Check both destinations before changing either one. Matching tags are idempotent.
  const existing = targets.map(target => manifestDigest(target, true));
  for (const target of existing) requireVersionTarget(target, digest);
  for (let index = 0; index < targets.length; index++) {
    const target = targets[index]!;
    if (existing[index] === undefined) {
      command([
        "docker", "buildx", "imagetools", "create", "--prefer-index=false",
        "--tag", target, `${repositories[index]}@${digest}`,
      ]);
    }
    if (manifestDigest(target) !== digest) throw new Error(`promoted digest differs: ${target}`);
    console.log(`${target} -> ${digest}`);
  }

  if (process.env.GITHUB_STEP_SUMMARY) {
    const file = Bun.file(process.env.GITHUB_STEP_SUMMARY);
    const previous = await file.exists() ? await file.text() : "";
    await Bun.write(file, `${previous}\n### Container release ${version}\n\n\`\`\`text\n${targets.map(target => `${target}@${digest}`).join("\n")}\n\`\`\`\n`);
  }
}

if (import.meta.main) {
  main().catch(error => {
    console.error(error instanceof Error ? error.message : "container release tagging failed");
    process.exit(1);
  });
}
