#!/usr/bin/env bun

export type DevelopmentVersion = {
  major: number;
  minor: number;
  patch: number;
  development?: number;
};

const VERSION_PATTERN = /^(\d+)\.(\d+)\.(\d+)(?:-dev\.(\d+))?$/;

export function parseVersion(value: string): DevelopmentVersion {
  const match = VERSION_PATTERN.exec(value.trim());
  if (!match) {
    throw new Error(`unsupported version '${value}'; expected X.Y.Z or X.Y.Z-dev.N`);
  }

  const version: DevelopmentVersion = {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
  };
  if (match[4] !== undefined) version.development = Number(match[4]);
  return version;
}

export function formatVersion(version: DevelopmentVersion): string {
  const stable = `${version.major}.${version.minor}.${version.patch}`;
  return version.development === undefined
    ? stable
    : `${stable}-dev.${version.development}`;
}

export function compareVersions(left: DevelopmentVersion, right: DevelopmentVersion): number {
  for (const key of ["major", "minor", "patch"] as const) {
    if (left[key] !== right[key]) return left[key] < right[key] ? -1 : 1;
  }

  if (left.development === right.development) return 0;
  if (left.development === undefined) return 1;
  if (right.development === undefined) return -1;
  return left.development < right.development ? -1 : 1;
}

export function nextVersion(current: DevelopmentVersion, increment: string): DevelopmentVersion {
  if (increment === "next") {
    if (current.development === undefined) {
      throw new Error("'next' requires an existing X.Y.Z-dev.N version");
    }
    return { ...current, development: current.development + 1 };
  }

  const next = { major: current.major, minor: current.minor, patch: current.patch, development: 1 };
  if (increment === "major") return { major: next.major + 1, minor: 0, patch: 0, development: 1 };
  if (increment === "minor") return { ...next, minor: next.minor + 1, patch: 0 };
  if (increment === "patch") return { ...next, patch: next.patch + 1 };
  throw new Error(`unknown bump '${increment}'; expected major, minor, patch, or next`);
}

export function stableVersion(current: DevelopmentVersion): DevelopmentVersion {
  if (current.development === undefined) throw new Error(`${formatVersion(current)} is already stable`);
  return { major: current.major, minor: current.minor, patch: current.patch };
}

export function manifestVersion(source: string): string {
  const header = /^\[package\]\s*$/m.exec(source);
  if (!header || header.index === undefined) throw new Error("Cargo.toml [package] section was not found");
  const remainder = source.slice(header.index + header[0].length);
  const nextSection = remainder.search(/^\[/m);
  const packageSection = nextSection === -1 ? remainder : remainder.slice(0, nextSection);
  const version = packageSection?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
  if (!version) throw new Error("Cargo.toml [package] version was not found");
  return version;
}

export function lockVersion(source: string): string {
  const packages = source.split(/^\[\[package\]\]\s*$/m).slice(1);
  const ironflow = packages.find((entry) => /^name\s*=\s*"ironflow"\s*$/m.test(entry));
  const version = ironflow?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
  if (!version) throw new Error("Cargo.lock ironflow package version was not found");
  return version;
}

export function replaceManifestVersion(source: string, version: string): string {
  const current = manifestVersion(source);
  return source.replace(
    new RegExp(`(^\\[package\\]\\s*$[\\s\\S]*?^version\\s*=\\s*)"${escapeRegex(current)}"`, "m"),
    `$1"${version}"`,
  );
}

export function replaceLockVersion(source: string, version: string): string {
  const current = lockVersion(source);
  return source.replace(
    new RegExp(`(^\\[\\[package\\]\\]\\s*$[\\s\\S]*?^name\\s*=\\s*"ironflow"\\s*$[\\s\\S]*?^version\\s*=\\s*)"${escapeRegex(current)}"`, "m"),
    `$1"${version}"`,
  );
}

function escapeRegex(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function gitShow(revision: string, path: string): string {
  const result = Bun.spawnSync(["git", "show", `${revision}:${path}`], {
    stdout: "pipe",
    stderr: "pipe",
  });
  if (result.exitCode !== 0) {
    throw new Error(new TextDecoder().decode(result.stderr).trim() || `cannot read ${path} at ${revision}`);
  }
  return new TextDecoder().decode(result.stdout);
}

export function validateDevelopmentVersion(
  localManifest: string,
  localLock: string,
  remoteManifest?: string,
): string {
  const localValue = manifestVersion(localManifest);
  const local = parseVersion(localValue);
  if (local.development === undefined) {
    throw new Error(`develop must use a prerelease version; found ${localValue}`);
  }
  const locked = lockVersion(localLock);
  if (locked !== localValue) {
    throw new Error(`Cargo.lock version ${locked} does not match Cargo.toml ${localValue}`);
  }

  if (remoteManifest !== undefined) {
    const remoteValue = manifestVersion(remoteManifest);
    if (compareVersions(local, parseVersion(remoteValue)) <= 0) {
      throw new Error(`local version ${localValue} must be newer than remote develop version ${remoteValue}`);
    }
  }
  return localValue;
}

async function bump(increment: string): Promise<void> {
  const manifestPath = "Cargo.toml";
  const lockPath = "Cargo.lock";
  const manifest = await Bun.file(manifestPath).text();
  const lock = await Bun.file(lockPath).text();
  const current = manifestVersion(manifest);
  if (lockVersion(lock) !== current) throw new Error("Cargo.toml and Cargo.lock versions differ");
  const version = formatVersion(nextVersion(parseVersion(current), increment));
  await Bun.write(manifestPath, replaceManifestVersion(manifest, version));
  await Bun.write(lockPath, replaceLockVersion(lock, version));
  console.log(`IronFlow development version: ${current} -> ${version}`);
}

async function finalize(): Promise<void> {
  const manifestPath = "Cargo.toml";
  const lockPath = "Cargo.lock";
  const manifest = await Bun.file(manifestPath).text();
  const lock = await Bun.file(lockPath).text();
  const current = manifestVersion(manifest);
  if (lockVersion(lock) !== current) throw new Error("Cargo.toml and Cargo.lock versions differ");
  const version = formatVersion(stableVersion(parseVersion(current)));
  await Bun.write(manifestPath, replaceManifestVersion(manifest, version));
  await Bun.write(lockPath, replaceLockVersion(lock, version));
  console.log(`IronFlow release version: ${current} -> ${version}`);
}

function check(localRevision: string, remoteRevision?: string): void {
  const zeroRevision = remoteRevision !== undefined && /^0+$/.test(remoteRevision);
  const version = validateDevelopmentVersion(
    gitShow(localRevision, "Cargo.toml"),
    gitShow(localRevision, "Cargo.lock"),
    remoteRevision === undefined || zeroRevision ? undefined : gitShow(remoteRevision, "Cargo.toml"),
  );
  console.log(`Development version check passed: ${version}`);
}

async function main(): Promise<void> {
  const [command, ...args] = Bun.argv.slice(2);
  if (command === "bump" && args.length === 1) return bump(args[0]);
  if (command === "finalize" && args.length === 0) return finalize();
  if (command === "check" && (args.length === 1 || args.length === 2)) return check(args[0], args[1]);
  throw new Error(
    "usage: bun scripts/development_version.ts bump <major|minor|patch|next>\n" +
      "       bun scripts/development_version.ts finalize\n" +
      "       bun scripts/development_version.ts check <local-sha> [remote-sha]",
  );
}

if (import.meta.main) {
  main().catch((error) => {
    console.error(`development version: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  });
}
