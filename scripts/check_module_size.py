#!/usr/bin/env python3
"""Enforce IronFlow's reviewed Rust production-module size ratchet."""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import TextIO

TARGET_LINES = 300
HARD_LIMIT_LINES = 400
REPORT_COUNT = 20
MAX_EXCEPTION_BUDGET = 17
DEFAULT_POLICY = Path("scripts/module_size_policy.json")
MIN_RATIONALE_LENGTH = 20
REVIEW_NOTE = (
    "LOC is a review trigger, not a design score: review responsibility boundaries, "
    "cognitive complexity, and useful test extraction before changing an exception."
)


class PolicyError(ValueError):
    """The checked-in exception policy is malformed or unsafe."""


@dataclass(frozen=True)
class ExceptionRule:
    max_lines: int
    rationale: str


@dataclass(frozen=True)
class ModuleSize:
    path: str
    lines: int


@dataclass(frozen=True)
class Policy:
    exception_budget: int
    rules: dict[str, ExceptionRule]


def _reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise PolicyError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _load_json(path: Path) -> object:
    try:
        with path.open("r", encoding="utf-8") as handle:
            return json.load(handle, object_pairs_hook=_reject_duplicate_keys)
    except PolicyError:
        raise
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise PolicyError(f"cannot read {path}: {error}") from error


def _validated_exception(raw: object, index: int) -> tuple[str, ExceptionRule]:
    location = f"exceptions[{index}]"
    if not isinstance(raw, dict):
        raise PolicyError(f"{location} must be an object")
    expected = {"path", "max_lines", "rationale"}
    if set(raw) != expected:
        raise PolicyError(f"{location} must contain exactly {sorted(expected)}")

    path = raw["path"]
    if not isinstance(path, str):
        raise PolicyError(f"{location}.path must be a string")
    parsed = PurePosixPath(path)
    if (
        parsed.is_absolute()
        or str(parsed) != path
        or "\\" in path
        or len(parsed.parts) < 2
        or parsed.parts[0] != "src"
        or parsed.suffix != ".rs"
        or any(part in {"", ".", ".."} for part in parsed.parts)
    ):
        raise PolicyError(f"{location}.path must be a normalized src/**/*.rs path")

    max_lines = raw["max_lines"]
    if type(max_lines) is not int or not TARGET_LINES < max_lines <= HARD_LIMIT_LINES:
        raise PolicyError(
            f"{location}.max_lines must be between {TARGET_LINES + 1} "
            f"and {HARD_LIMIT_LINES}"
        )

    rationale = raw["rationale"]
    if not isinstance(rationale, str) or len(rationale.strip()) < MIN_RATIONALE_LENGTH:
        raise PolicyError(
            f"{location}.rationale must explain the reviewed responsibility boundary"
        )
    if rationale.strip().lower() in {"todo", "tbd", "placeholder"}:
        raise PolicyError(f"{location}.rationale cannot be a placeholder")
    return path, ExceptionRule(max_lines=max_lines, rationale=rationale.strip())


def load_policy(path: Path) -> Policy:
    raw = _load_json(path)
    expected = {"version", "exception_budget", "exceptions"}
    if not isinstance(raw, dict) or set(raw) != expected:
        raise PolicyError(f"policy must contain exactly {sorted(expected)}")
    if type(raw["version"]) is not int or raw["version"] != 1:
        raise PolicyError("unsupported module-size policy version")
    budget = raw["exception_budget"]
    if type(budget) is not int or budget < 0:
        raise PolicyError("policy exception_budget must be a non-negative integer")
    if budget > MAX_EXCEPTION_BUDGET:
        raise PolicyError(
            f"policy exception_budget {budget} exceeds the fixed IF-034 baseline "
            f"of {MAX_EXCEPTION_BUDGET}"
        )
    entries = raw["exceptions"]
    if not isinstance(entries, list):
        raise PolicyError("policy exceptions must be an array")

    rules: dict[str, ExceptionRule] = {}
    previous_path: str | None = None
    for index, entry in enumerate(entries):
        module_path, rule = _validated_exception(entry, index)
        if module_path in rules:
            raise PolicyError(f"duplicate exception path: {module_path}")
        if previous_path is not None and module_path <= previous_path:
            raise PolicyError("policy exceptions must be sorted by path")
        rules[module_path] = rule
        previous_path = module_path
    if len(rules) != budget:
        raise PolicyError(
            f"policy has {len(rules)} exceptions but exception_budget is {budget}; "
            "lowering or raising the budget requires an explicit policy review"
        )
    return Policy(exception_budget=budget, rules=rules)


def count_physical_lines(data: bytes) -> int:
    return data.count(b"\n") + int(bool(data) and not data.endswith(b"\n"))


def scan_modules(root: Path) -> list[ModuleSize]:
    source_root = root / "src"
    if source_root.is_symlink() or not source_root.is_dir():
        raise PolicyError(f"Rust source root does not exist: {source_root}")
    modules = []
    pending = [source_root]
    while pending:
        directory = pending.pop()
        try:
            entries = sorted(directory.iterdir(), key=lambda path: path.name)
        except OSError as error:
            raise PolicyError(f"cannot scan {directory}: {error}") from error
        for path in entries:
            if path.is_symlink():
                raise PolicyError(f"symlinks are not allowed under the Rust source root: {path}")
            if path.is_dir():
                pending.append(path)
                continue
            if not path.is_file():
                if path.suffix == ".rs":
                    raise PolicyError(f"Rust module is not a regular file: {path}")
                continue
            if path.suffix != ".rs":
                continue
            try:
                lines = count_physical_lines(path.read_bytes())
            except OSError as error:
                raise PolicyError(f"cannot read {path}: {error}") from error
            modules.append(ModuleSize(path.relative_to(root).as_posix(), lines))
    return sorted(modules, key=lambda module: (-module.lines, module.path))


def evaluate(
    modules: list[ModuleSize], rules: dict[str, ExceptionRule]
) -> list[str]:
    violations: list[str] = []
    sizes = {module.path: module.lines for module in modules}
    for path in rules.keys() - sizes.keys():
        violations.append(f"{path}: exception is stale because the module does not exist")

    for module in modules:
        rule = rules.get(module.path)
        if module.lines > HARD_LIMIT_LINES:
            violations.append(
                f"{module.path}: {module.lines} lines exceeds the unconditional "
                f"{HARD_LIMIT_LINES}-line hard limit"
            )
        elif rule is None and module.lines > TARGET_LINES:
            violations.append(
                f"{module.path}: {module.lines} lines exceeds the {TARGET_LINES}-line target "
                "without a reviewed exception"
            )
        elif rule is not None and module.lines > rule.max_lines:
            violations.append(
                f"{module.path}: grew to {module.lines} lines above its reviewed "
                f"{rule.max_lines}-line ceiling"
            )
        elif rule is not None and module.lines <= TARGET_LINES:
            violations.append(
                f"{module.path}: now fits the {TARGET_LINES}-line target; remove its stale exception"
            )
        elif rule is not None and module.lines < rule.max_lines:
            violations.append(
                f"{module.path}: shrank to {module.lines} lines; lower its reviewed "
                f"{rule.max_lines}-line ceiling so the improvement cannot regress"
            )
    return sorted(violations)


def run_check(root: Path, policy_path: Path, stream: TextIO) -> int:
    policy = load_policy(policy_path)
    rules = policy.rules
    modules = scan_modules(root)
    violations = evaluate(modules, rules)

    print(f"Largest Rust production modules ({len(modules)} scanned):", file=stream)
    for module in modules[:REPORT_COUNT]:
        rule = rules.get(module.path)
        annotation = f" [reviewed ceiling: {rule.max_lines}]" if rule else ""
        print(f"{module.lines:4}  {module.path}{annotation}", file=stream)
    print(REVIEW_NOTE, file=stream)

    if violations:
        print("Module-size policy violations:", file=stream)
        for violation in violations:
            print(f"- {violation}", file=stream)
        return 1
    print(
        f"Module-size policy passed: target <= {TARGET_LINES}, hard limit <= "
        f"{HARD_LIMIT_LINES}, reviewed exceptions {len(rules)}/"
        f"{policy.exception_budget}.",
        file=stream,
    )
    return 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    repository = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=repository)
    parser.add_argument("--policy", type=Path, default=DEFAULT_POLICY)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    root = args.root.resolve()
    policy_path = args.policy
    if not policy_path.is_absolute():
        policy_path = root / policy_path
    try:
        return run_check(root, policy_path, sys.stdout)
    except PolicyError as error:
        print(f"Invalid module-size policy: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
