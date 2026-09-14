from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


REPOSITORY = Path(__file__).resolve().parents[3]
HOOK = REPOSITORY / ".githooks/pre-commit"


class PreCommitRegistryTests(unittest.TestCase):
    def run_fixture(self, path: str, gate_status: int = 0, deleted: bool = False):
        # Git exports repository-local paths when invoking hooks, including worktree indexes.
        environment = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(["git", "init", "-q", str(root)], env=environment, check=True)
            target = root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text("fixture\n")
            subprocess.run(["git", "add", path], cwd=root, env=environment, check=True)
            if deleted:
                # Commit only the disposable test fixture, never the product checkout.
                subprocess.run([
                    "git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                    "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false", "commit", "-qm", "fixture",
                ], cwd=root, env=environment, check=True)
                target.unlink()
                subprocess.run(["git", "add", path], cwd=root, env=environment, check=True)
            binaries = root / "bin"
            binaries.mkdir()
            bun = binaries / "bun"
            bun.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$HOOK_TRACE"\nexit "$GATE_STATUS"\n')
            bun.chmod(0o700)
            trace = root / "trace"
            environment.update({"PATH": f"{binaries}{os.pathsep}{environment.get('PATH', '')}",
                                "HOOK_TRACE": str(trace), "GATE_STATUS": str(gate_status)})
            result = subprocess.run(["sh", str(HOOK)], cwd=root, env=environment,
                                    capture_output=True, text=True, check=False)
            return result, trace.read_text() if trace.exists() else ""

    def test_registry_surfaces_invoke_check(self) -> None:
        for path in ("docs/issues/IF-106.md", "docs/issues/README.md", "docs/ROADMAP.md", "ISSUES.md", "AGENTS.md"):
            with self.subTest(path=path):
                result, trace = self.run_fixture(path)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(trace, "run scripts/issues_registry.ts check\n")

    def test_registry_failure_stops_commit_hook(self) -> None:
        result, trace = self.run_fixture("docs/issues/IF-106.md", gate_status=1)
        self.assertEqual(result.returncode, 1)
        self.assertIn("issues_registry.ts check", trace)

    def test_deleted_issue_still_invokes_registry_check(self) -> None:
        result, trace = self.run_fixture("docs/issues/IF-106.md", deleted=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("issues_registry.ts check", trace)

    def test_unrelated_docs_do_not_invoke_registry_check(self) -> None:
        result, trace = self.run_fixture("docs/unrelated.md")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(trace, "")

    def test_fixtures_do_not_use_the_invoking_repositories_git_environment(self) -> None:
        environment = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(["git", "init", "-q", str(root)], env=environment, check=True)
            git_directory = root / ".git"
            with patch.dict(os.environ, {
                "GIT_DIR": str(git_directory),
                "GIT_COMMON_DIR": str(git_directory),
                "GIT_WORK_TREE": str(root),
                "GIT_INDEX_FILE": str(git_directory / "index"),
            }):
                result, trace = self.run_fixture("docs/issues/IF-106.md", deleted=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(trace, "run scripts/issues_registry.ts check\n")
            self.assertFalse((git_directory / "index").exists())
            head = subprocess.run(["git", "rev-parse", "--verify", "HEAD"], cwd=root,
                                  env=environment, capture_output=True, check=False)
            self.assertNotEqual(head.returncode, 0)
            bare = subprocess.run(["git", "config", "core.bare"], cwd=root, env=environment,
                                  capture_output=True, text=True, check=True)
            self.assertEqual(bare.stdout.strip(), "false")


if __name__ == "__main__":
    unittest.main()
