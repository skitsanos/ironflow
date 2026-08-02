#!/usr/bin/env python3
"""Provide small, secret-safe IronFlow context at Codex session boundaries."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


def git_output(cwd: Path, *args: str) -> str | None:
    try:
        result = subprocess.run(
            ["git", *args],
            cwd=cwd,
            check=True,
            capture_output=True,
            text=True,
            timeout=3,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return result.stdout.strip()


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, TypeError):
        payload = {}

    cwd = Path(payload.get("cwd") or Path.cwd())
    root = git_output(cwd, "rev-parse", "--show-toplevel")
    if root is None:
        return 0

    branch = git_output(cwd, "branch", "--show-current") or "detached HEAD"
    status = git_output(cwd, "status", "--porcelain=v1")
    changed = len(status.splitlines()) if status else 0
    context = (
        f"IronFlow session context: branch {branch}; {changed} changed or untracked paths. "
        "Read the repository AGENTS.md before editing, preserve existing worktree changes, "
        "use ISSUES.md as the IF issue ledger, and use repo-local skills from .agents/skills."
    )
    print(
        json.dumps(
            {
                "hookSpecificOutput": {
                    "hookEventName": "SessionStart",
                    "additionalContext": context,
                }
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
