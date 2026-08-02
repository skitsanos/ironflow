#!/usr/bin/env python3
"""Reject apply_patch edits to repository paths that may contain secrets."""

from __future__ import annotations

import json
import re
import sys
from pathlib import PurePosixPath


PATCH_PATH = re.compile(
    r"^\*\*\* (?:Add|Update|Delete) File:\s*(?P<path>.+?)\s*$",
    re.MULTILINE,
)
MOVE_PATH = re.compile(r"^\*\*\* Move to:\s*(?P<path>.+?)\s*$", re.MULTILINE)
PRIVATE_KEY_SUFFIXES = {".key", ".p12", ".pem", ".pfx"}


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
    if path.suffix.lower() in PRIVATE_KEY_SUFFIXES:
        return "private-key and certificate-container files are protected"
    return None


def deny(path: PurePosixPath, reason: str) -> None:
    print(
        json.dumps(
            {
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": f"Blocked protected path '{path}': {reason}.",
                }
            }
        )
    )


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, TypeError):
        return 0

    if payload.get("tool_name") != "apply_patch":
        return 0
    command = payload.get("tool_input", {}).get("command")
    if not isinstance(command, str):
        return 0

    matches = [*PATCH_PATH.finditer(command), *MOVE_PATH.finditer(command)]
    for match in matches:
        path = normalize(match.group("path"))
        if reason := protected_reason(path):
            deny(path, reason)
            return 0
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
