from __future__ import annotations

import importlib.util
import io
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "check_module_size.py"
SPEC = importlib.util.spec_from_file_location("check_module_size", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
CHECKER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = CHECKER
SPEC.loader.exec_module(CHECKER)


class ModuleSizeCheckerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        (self.root / "src").mkdir()

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write_module(self, relative: str, lines: int) -> None:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("".join(f"// line {index}\n" for index in range(lines)))

    def write_policy(self, exceptions: list[dict[str, object]]) -> Path:
        policy = self.root / "policy.json"
        policy.write_text(
            json.dumps(
                {
                    "version": 1,
                    "exception_budget": len(exceptions),
                    "exceptions": exceptions,
                },
                indent=2,
            )
            + "\n"
        )
        return policy

    def check(self, exceptions: list[dict[str, object]]) -> tuple[int, str]:
        output = io.StringIO()
        result = CHECKER.run_check(self.root, self.write_policy(exceptions), output)
        return result, output.getvalue()

    @staticmethod
    def exception(path: str, max_lines: int) -> dict[str, object]:
        return {
            "path": path,
            "max_lines": max_lines,
            "rationale": "This cohesive responsibility has been explicitly reviewed.",
        }

    def test_clean_check_reports_largest_modules_and_review_guidance(self) -> None:
        self.write_module("src/small.rs", 12)
        self.write_module("src/beta.rs", 300)
        self.write_module("src/alpha.rs", 300)

        result, output = self.check([])

        self.assertEqual(result, 0)
        self.assertLess(output.index("src/alpha.rs"), output.index("src/beta.rs"))
        self.assertLess(output.index("src/beta.rs"), output.index("src/small.rs"))
        self.assertIn("LOC is a review trigger, not a design score", output)
        self.assertIn("cognitive complexity", output)

    def test_unlisted_module_above_target_requires_reviewed_exception(self) -> None:
        self.write_module("src/new_module.rs", 301)

        result, output = self.check([])

        self.assertEqual(result, 1)
        self.assertIn("Largest Rust production modules", output)
        self.assertIn("exceeds the 300-line target without a reviewed exception", output)

    def test_reviewed_exception_pins_a_module_ceiling(self) -> None:
        self.write_module("src/cohesive.rs", 320)

        result, output = self.check([self.exception("src/cohesive.rs", 320)])

        self.assertEqual(result, 0)
        self.assertIn("[reviewed ceiling: 320]", output)

    def test_allowlisted_module_cannot_grow(self) -> None:
        self.write_module("src/cohesive.rs", 321)

        result, output = self.check([self.exception("src/cohesive.rs", 320)])

        self.assertEqual(result, 1)
        self.assertIn("grew to 321 lines above its reviewed 320-line ceiling", output)

    def test_reduction_ratchets_the_reviewed_ceiling_downward(self) -> None:
        self.write_module("src/cohesive.rs", 319)

        result, output = self.check([self.exception("src/cohesive.rs", 320)])

        self.assertEqual(result, 1)
        self.assertIn("lower its reviewed 320-line ceiling", output)

    def test_hard_limit_is_unconditional(self) -> None:
        self.write_module("src/cohesive.rs", 401)

        result, output = self.check([self.exception("src/cohesive.rs", 400)])

        self.assertEqual(result, 1)
        self.assertIn("exceeds the unconditional 400-line hard limit", output)

    def test_exact_target_and_hard_limit_boundaries_are_accepted(self) -> None:
        self.write_module("src/target.rs", 300)
        self.write_module("src/cohesive.rs", 400)

        result, _ = self.check([self.exception("src/cohesive.rs", 400)])

        self.assertEqual(result, 0)

    def test_short_rationale_is_rejected(self) -> None:
        self.write_module("src/cohesive.rs", 320)
        exception = self.exception("src/cohesive.rs", 320)
        exception["rationale"] = "TBD"

        with self.assertRaisesRegex(CHECKER.PolicyError, "rationale"):
            CHECKER.load_policy(self.write_policy([exception]))

    def test_invalid_exception_paths_and_ceiling_are_rejected(self) -> None:
        invalid = [
            "../src/module.rs",
            "/src/module.rs",
            "src/../module.rs",
            "src\\module.rs",
            "tests/module.rs",
            "src/module.txt",
        ]
        for path in invalid:
            with self.subTest(path=path):
                with self.assertRaisesRegex(CHECKER.PolicyError, "src/\\*\\*/\\*\\.rs"):
                    CHECKER.load_policy(
                        self.write_policy([self.exception(path, 320)])
                    )

        with self.assertRaisesRegex(CHECKER.PolicyError, "between 301 and 400"):
            CHECKER.load_policy(
                self.write_policy([self.exception("src/module.rs", 401)])
            )

    def test_duplicate_and_unsorted_exception_paths_are_rejected(self) -> None:
        duplicate = self.exception("src/a.rs", 320)
        with self.assertRaisesRegex(CHECKER.PolicyError, "duplicate exception path"):
            CHECKER.load_policy(self.write_policy([duplicate, duplicate]))

        with self.assertRaisesRegex(CHECKER.PolicyError, "sorted by path"):
            CHECKER.load_policy(
                self.write_policy(
                    [
                        self.exception("src/z.rs", 320),
                        self.exception("src/a.rs", 320),
                    ]
                )
            )

    def test_missing_and_now_small_exceptions_are_stale(self) -> None:
        self.write_module("src/reduced.rs", 300)
        exceptions = [
            self.exception("src/missing.rs", 320),
            self.exception("src/reduced.rs", 320),
        ]

        result, output = self.check(exceptions)

        self.assertEqual(result, 1)
        self.assertIn("module does not exist", output)
        self.assertIn("remove its stale exception", output)

    def test_only_production_src_modules_are_scanned(self) -> None:
        self.write_module("src/lib.rs", 10)
        self.write_module("tests/large_fixture.rs", 900)

        result, output = self.check([])

        self.assertEqual(result, 0)
        self.assertNotIn("large_fixture.rs", output)

    def test_unterminated_final_line_and_crlf_are_counted_portably(self) -> None:
        unterminated = self.root / "src" / "unterminated.rs"
        crlf = self.root / "src" / "crlf.rs"
        unterminated.write_bytes(b"first\nsecond")
        crlf.write_bytes(b"first\r\nsecond\r\n")

        sizes = {item.path: item.lines for item in CHECKER.scan_modules(self.root)}

        self.assertEqual(sizes["src/unterminated.rs"], 2)
        self.assertEqual(sizes["src/crlf.rs"], 2)
        self.assertEqual(CHECKER.count_physical_lines(b""), 0)
        self.assertEqual(CHECKER.count_physical_lines(b"one\ntwo\n"), 2)

    @unittest.skipUnless(hasattr(os, "symlink"), "symlinks are unavailable")
    def test_file_and_directory_symlinks_are_rejected_instead_of_followed(self) -> None:
        outside = self.root / "outside.rs"
        outside.write_text("// outside\n")
        link = self.root / "src" / "linked.rs"
        os.symlink(outside, link)

        with self.assertRaisesRegex(CHECKER.PolicyError, "symlinks are not allowed"):
            CHECKER.scan_modules(self.root)

        link.unlink()
        outside_directory = self.root / "outside"
        outside_directory.mkdir()
        (outside_directory / "hidden.rs").write_text("// hidden\n")
        os.symlink(outside_directory, self.root / "src" / "linked_directory")

        with self.assertRaisesRegex(CHECKER.PolicyError, "symlinks are not allowed"):
            CHECKER.scan_modules(self.root)

    def test_exception_count_must_match_the_declared_budget(self) -> None:
        self.write_module("src/cohesive.rs", 320)
        policy = self.write_policy([self.exception("src/cohesive.rs", 320)])
        data = json.loads(policy.read_text())
        data["exception_budget"] = 2
        policy.write_text(json.dumps(data))

        with self.assertRaisesRegex(CHECKER.PolicyError, "explicit policy review"):
            CHECKER.load_policy(policy)

    def test_exception_budget_cannot_exceed_the_if034_baseline(self) -> None:
        exceptions = []
        for index in range(18):
            path = f"src/module_{index:02}.rs"
            self.write_module(path, 301)
            exceptions.append(self.exception(path, 301))

        with self.assertRaisesRegex(CHECKER.PolicyError, "fixed IF-034 baseline of 17"):
            CHECKER.load_policy(self.write_policy(exceptions))


if __name__ == "__main__":
    unittest.main()
