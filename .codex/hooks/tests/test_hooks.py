from __future__ import annotations

import json
import subprocess
import tomllib
import unittest
from pathlib import Path


HOOKS = Path(__file__).resolve().parents[1]
PROTECT_PATHS = HOOKS / "protect_paths.py"
SESSION_CONTEXT = HOOKS / "session_context.py"
REPOSITORY = HOOKS.parents[1]


def run_hook(script: Path, payload: dict[str, object]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["python3", str(script)],
        input=json.dumps(payload),
        text=True,
        capture_output=True,
        check=False,
        cwd=REPOSITORY,
    )


def patch_payload(path: str) -> dict[str, object]:
    return {
        "tool_name": "apply_patch",
        "tool_input": {"command": f"*** Begin Patch\n*** Update File: {path}\n*** End Patch\n"},
    }


class ProtectPathsTests(unittest.TestCase):
    def assert_blocked(self, path: str) -> None:
        result = run_hook(PROTECT_PATHS, patch_payload(path))
        self.assertEqual(result.returncode, 0, result.stderr)
        output = json.loads(result.stdout)
        hook = output["hookSpecificOutput"]
        self.assertEqual(hook["permissionDecision"], "deny")
        self.assertIn(path, hook["permissionDecisionReason"])

    def test_blocks_environment_and_secret_paths(self) -> None:
        for path in (".env", "config/.env.local", "secrets/token.txt", ".git/config"):
            with self.subTest(path=path):
                self.assert_blocked(path)

    def test_blocks_private_key_extensions(self) -> None:
        for path in ("server.pem", "certs/private.key", "bundle.p12", "bundle.pfx"):
            with self.subTest(path=path):
                self.assert_blocked(path)

    def test_allows_source_and_environment_template(self) -> None:
        for path in ("src/main.rs", ".env.example", "docs/security.md"):
            with self.subTest(path=path):
                result = run_hook(PROTECT_PATHS, patch_payload(path))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, "")

    def test_ignores_other_tool_names(self) -> None:
        payload = patch_payload(".env")
        payload["tool_name"] = "Bash"
        result = run_hook(PROTECT_PATHS, payload)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")


class SessionContextTests(unittest.TestCase):
    def test_reports_secret_safe_repository_context(self) -> None:
        result = run_hook(SESSION_CONTEXT, {"cwd": str(REPOSITORY)})
        self.assertEqual(result.returncode, 0, result.stderr)
        output = json.loads(result.stdout)
        hook = output["hookSpecificOutput"]
        self.assertEqual(hook["hookEventName"], "SessionStart")
        self.assertIn("IronFlow session context", hook["additionalContext"])
        self.assertNotIn(".env", hook["additionalContext"])


class ConfigurationTests(unittest.TestCase):
    def test_project_config_enables_stable_hooks(self) -> None:
        config = tomllib.loads((REPOSITORY / ".codex" / "config.toml").read_text())
        self.assertIs(config["features"]["hooks"], True)

    def test_hook_manifest_references_existing_repo_scripts(self) -> None:
        manifest = json.loads((REPOSITORY / ".codex" / "hooks.json").read_text())
        commands = [
            hook["command"]
            for groups in manifest["hooks"].values()
            for group in groups
            for hook in group["hooks"]
        ]
        self.assertTrue(commands)
        self.assertTrue(all("git rev-parse --show-toplevel" in command for command in commands))
        self.assertTrue(any("session_context.py" in command for command in commands))
        self.assertTrue(any("protect_paths.py" in command for command in commands))


if __name__ == "__main__":
    unittest.main()
