#!/usr/bin/env python3
"""Reject apply_patch edits to repository paths that may contain secrets."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path, PurePosixPath


PATCH_PATH = re.compile(
    r"^\*\*\* (?:Add|Update|Delete) File:\s*(?P<path>.+?)\s*$",
    re.MULTILINE,
)
MOVE_PATH = re.compile(r"^\*\*\* Move to:\s*(?P<path>.+?)\s*$", re.MULTILINE)
PRIVATE_KEY_SUFFIXES = {".key", ".p12", ".pem", ".pfx"}
PRIVATE_KEY_NAMES = {"id_rsa", "id_dsa", "id_ecdsa", "id_ed25519"}


def normalize(raw_path: str) -> PurePosixPath:
    return PurePosixPath(raw_path.strip().replace("\\", "/"))


def protected_reason(path: PurePosixPath) -> str | None:
    lowered_parts = tuple(part.lower() for part in path.parts)
    name = path.name.lower()

    if ".git" in lowered_parts:
        return ".git internals must not be edited directly"
    if "secrets" in lowered_parts:
        return "secret material must not be stored in the repository"
    if name == ".env" or (name.startswith(".env.") and name != ".env.example"):
        return "runtime environment files are protected; edit .env.example instead"
    if path.suffix.lower() in PRIVATE_KEY_SUFFIXES or name in PRIVATE_KEY_NAMES:
        return "private-key and certificate-container files are protected"
    return None


def deny(reason: str) -> None:
    print(
        json.dumps(
            {
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                }
            }
        )
    )


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, TypeError):
        deny("Cannot inspect malformed protected-path hook input.")
        return 0

    if not isinstance(payload, dict):
        deny("Cannot inspect malformed protected-path hook input.")
        return 0
    if payload.get("tool_name") != "apply_patch":
        return 0
    tool_input = payload.get("tool_input")
    command = tool_input.get("command") if isinstance(tool_input, dict) else None
    if not isinstance(command, str):
        deny("Cannot inspect apply_patch without a string tool_input.command.")
        return 0

    matches = [*PATCH_PATH.finditer(command), *MOVE_PATH.finditer(command)]
    if not matches:
        deny("Cannot inspect apply_patch without explicit file paths.")
        return 0
    cwd = payload.get("cwd")
    if cwd is not None and not isinstance(cwd, str):
        deny("Cannot resolve apply_patch paths without a valid working directory.")
        return 0
    root = Path(cwd) if cwd else Path.cwd()
    for match in matches:
        path = normalize(match.group("path"))
        if reason := protected_reason(path):
            deny(f"Blocked protected path '{path}': {reason}.")
            return 0
        try:
            resolved = (root / Path(path)).resolve(strict=False)
        except (OSError, RuntimeError, ValueError):
            deny("Cannot safely resolve an apply_patch target.")
            return 0
        if reason := protected_reason(normalize(str(resolved))):
            deny(f"Blocked protected target of '{path}': {reason}.")
            return 0
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
