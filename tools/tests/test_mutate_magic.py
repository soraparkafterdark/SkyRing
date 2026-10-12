from __future__ import annotations

from pathlib import Path
import sys
import unittest

TOOLS = Path(__file__).resolve().parents[1]
ROOT = TOOLS.parent
sys.path.insert(0, str(TOOLS))
import mutate_magic  # noqa: E402


class MutantTests(unittest.TestCase):
    def test_every_mutant_still_matches_the_source_exactly_once(self) -> None:
        """A mutant that no longer matches would report 'NOT APPLICABLE' and prove nothing."""
        for name, (path, original, replacement) in mutate_magic.MUTANTS.items():
            with self.subTest(mutant=name):
                source = (ROOT / "core" / path).read_text(encoding="utf-8")
                self.assertEqual(source.count(original), 1, f"{name}: {original!r} in core/{path}")
                self.assertNotEqual(original, replacement)

    def test_the_default_set_has_controls_and_real_mutants(self) -> None:
        names = list(mutate_magic.MUTANTS)
        self.assertTrue(any(n.startswith("EQUIVALENT") for n in names))
        self.assertTrue(any(n.startswith("CONTROL") for n in names))
        self.assertGreaterEqual(sum(1 for n in names if not n.startswith(("EQUIVALENT", "CONTROL"))), 8)


if __name__ == "__main__":
    unittest.main()
