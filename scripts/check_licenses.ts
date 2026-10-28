#!/usr/bin/env bun
// Fail closed when a dependency that ships inside IronFlow binaries carries a
// license outside the reviewed permissive allowlist. IronFlow is MIT licensed
// and distributes statically linked binaries and container images, so every
// normal and build dependency reachable with all features is checked;
// dev-dependencies never ship and are ignored.

export type LicenseExpr =
  | { kind: "id"; id: string }
  | { kind: "or" | "and"; items: LicenseExpr[] };

export type Package = {
  id: string;
  name: string;
  version: string;
  license: string | null;
  license_file: string | null;
};

export type Metadata = {
  packages: Package[];
  resolve: {
    root: string | null;
    nodes: { id: string; deps: { pkg: string; dep_kinds: { kind: string | null }[] }[] }[];
  };
};

/// SPDX identifiers (and `WITH` exceptions) accepted in shipped dependencies.
export const ALLOWED_LICENSES: ReadonlySet<string> = new Set([
  "0BSD",
  "Apache-2.0",
  "Apache-2.0 WITH LLVM-exception",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "BSL-1.0",
  "bzip2-1.0.6",
  "CC0-1.0",
  "CDLA-Permissive-2.0",
  "ISC",
  "MIT",
  "MIT-0",
  "MPL-2.0",
  "Unicode-3.0",
  "Unicode-DFS-2016",
  "Unlicense",
  "Zlib",
]);

/// Packages without a usable SPDX expression, keyed `name@version`, with the
/// reviewed license-file finding. Entries must disappear with the package.
export const LICENSE_EXCEPTIONS: Readonly<Record<string, string>> = {};

type Token = { type: "open" | "close" | "or" | "and" | "with" | "id"; text: string };

function tokenize(expression: string): Token[] {
  const tokens: Token[] = [];
  for (const raw of expression.replace(/\//g, " OR ").replace(/([()])/g, " $1 ").split(/\s+/)) {
    if (!raw) continue;
    const upper = raw.toUpperCase();
    if (raw === "(") tokens.push({ type: "open", text: raw });
    else if (raw === ")") tokens.push({ type: "close", text: raw });
    else if (upper === "OR") tokens.push({ type: "or", text: raw });
    else if (upper === "AND") tokens.push({ type: "and", text: raw });
    else if (upper === "WITH") tokens.push({ type: "with", text: raw });
    else if (/^[A-Za-z0-9.+-]+$/.test(raw)) tokens.push({ type: "id", text: raw });
    else throw new Error(`unexpected license token '${raw}'`);
  }
  return tokens;
}

/// Parse an SPDX license expression. Legacy `A/B` means `A OR B`.
export function parseLicense(expression: string): LicenseExpr {
  const tokens = tokenize(expression);
  let index = 0;
  const peek = () => tokens[index];
  const next = (): Token => {
    const token = tokens[index++];
    if (!token) throw new Error("license expression ended early");
    return token;
  };
  const primary = (): LicenseExpr => {
    const token = next();
    if (token.type === "open") {
      const inner = orExpr();
      if (next().type !== "close") throw new Error("missing ')' in license expression");
      return inner;
    }
    if (token.type !== "id") throw new Error(`expected a license identifier, found '${token.text}'`);
    if (peek()?.type === "with") {
      next();
      const exception = next();
      if (exception.type !== "id") throw new Error("WITH requires an exception identifier");
      return { kind: "id", id: `${token.text} WITH ${exception.text}` };
    }
    return { kind: "id", id: token.text };
  };
  const andExpr = (): LicenseExpr => {
    const items = [primary()];
    while (peek()?.type === "and") {
      next();
      items.push(primary());
    }
    return items.length === 1 ? items[0] : { kind: "and", items };
  };
  const orExpr = (): LicenseExpr => {
    const items = [andExpr()];
    while (peek()?.type === "or") {
      next();
      items.push(andExpr());
    }
    return items.length === 1 ? items[0] : { kind: "or", items };
  };
  if (!tokens.length) throw new Error("empty license expression");
  const parsed = orExpr();
  if (index !== tokens.length) throw new Error(`unexpected '${tokens[index].text}' in license expression`);
  return parsed;
}

function allowed(expr: LicenseExpr, allowlist: ReadonlySet<string>): boolean {
  if (expr.kind === "id") return allowlist.has(expr.id);
  if (expr.kind === "or") return expr.items.some((item) => allowed(item, allowlist));
  return expr.items.every((item) => allowed(item, allowlist));
}

/// True only when every required operand is on the allowlist. Expressions that
/// cannot be parsed are never allowed.
export function licenseAllowed(expression: string, allowlist: ReadonlySet<string>): boolean {
  try {
    return allowed(parseLicense(expression), allowlist);
  } catch {
    return false;
  }
}

/// Packages reachable from the root through normal or build dependencies.
export function shippedPackages(metadata: Metadata): Package[] {
  const root = metadata.resolve.root;
  if (!root) throw new Error("cargo metadata has no root package");
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const packages = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
  const seen = new Set<string>();
  const pending = [root];
  while (pending.length) {
    const id = pending.pop()!;
    if (seen.has(id)) continue;
    seen.add(id);
    const node = nodes.get(id);
    if (!node) throw new Error(`cargo metadata resolve graph is missing ${id}`);
    for (const dep of node.deps) {
      if (dep.dep_kinds.some((kind) => kind.kind === null || kind.kind === "normal" || kind.kind === "build")) {
        pending.push(dep.pkg);
      }
    }
  }
  seen.delete(root);
  return [...seen]
    .map((id) => {
      const pkg = packages.get(id);
      if (!pkg) throw new Error(`cargo metadata packages are missing ${id}`);
      return pkg;
    })
    .sort((left, right) => left.name.localeCompare(right.name) || left.version.localeCompare(right.version));
}

/// Human-readable violations for the shipped graph; empty means compliant.
export function evaluate(
  metadata: Metadata,
  allowlist: ReadonlySet<string>,
  exceptions: Readonly<Record<string, string>>,
): string[] {
  const violations: string[] = [];
  const usedExceptions = new Set<string>();
  for (const pkg of shippedPackages(metadata)) {
    const key = `${pkg.name}@${pkg.version}`;
    if (Object.hasOwn(exceptions, key)) {
      usedExceptions.add(key);
      continue;
    }
    if (!pkg.license) {
      violations.push(`${pkg.name} ${pkg.version}: no license expression and no reviewed exception`);
      continue;
    }
    let expr: LicenseExpr;
    try {
      expr = parseLicense(pkg.license);
    } catch (error) {
      violations.push(`${pkg.name} ${pkg.version}: license '${pkg.license}' could not be parsed: ${String(error instanceof Error ? error.message : error)}`);
      continue;
    }
    if (!allowed(expr, allowlist)) {
      violations.push(`${pkg.name} ${pkg.version}: license '${pkg.license}' is not allowed`);
    }
  }
  for (const key of Object.keys(exceptions).sort()) {
    if (!usedExceptions.has(key)) {
      violations.push(`${key}: exception is stale because the package is not shipped`);
    }
  }
  return violations;
}

function main(): void {
  const result = Bun.spawnSync(
    ["cargo", "metadata", "--format-version", "1", "--locked", "--all-features"],
    { stdout: "pipe", stderr: "pipe" },
  );
  if (result.exitCode !== 0) {
    throw new Error(new TextDecoder().decode(result.stderr).trim() || "cargo metadata failed");
  }
  const metadata = JSON.parse(new TextDecoder().decode(result.stdout)) as Metadata;
  const violations = evaluate(metadata, ALLOWED_LICENSES, LICENSE_EXCEPTIONS);
  const shipped = shippedPackages(metadata).length;
  if (violations.length) {
    console.error(`Dependency license policy violations (${shipped} shipped packages checked):`);
    for (const violation of violations) console.error(`- ${violation}`);
    process.exit(1);
  }
  console.log(
    `Dependency license policy passed: ${shipped} shipped packages, ` +
      `${Object.keys(LICENSE_EXCEPTIONS).length} reviewed exceptions.`,
  );
}

if (import.meta.main) {
  try {
    main();
  } catch (error) {
    console.error(`license check failed closed: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  }
}
