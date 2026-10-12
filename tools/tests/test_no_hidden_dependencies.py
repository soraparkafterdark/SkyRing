"""The local profile runs the tool tests with `unittest` on whatever `python3` is first on PATH,
including a bare interpreter (the swarm's worktrees resolve to Homebrew's Python, which has no
pytest). A tool or a tool test that imports a third-party module turns that into a gate failure
that looks like a broken product. Everything under tools/ must use the standard library and the
repository's own modules only."""

import ast
from pathlib import Path
import sys
import unittest

TOOLS = Path(__file__).resolve().parents[1]
LOCAL = {path.stem for path in TOOLS.glob("*.py")} | {path.stem for path in (TOOLS / "tests").glob("*.py")} | {"tools"}
STANDARD = set(sys.stdlib_module_names) | {"__future__"}


def imported_roots(path: Path) -> set[str]:
    roots: set[str] = set()
    for node in ast.walk(ast.parse(path.read_text(encoding="utf-8"), filename=str(path))):
        if isinstance(node, ast.Import):
            roots.update(alias.name.split(".")[0] for alias in node.names)
        elif isinstance(node, ast.ImportFrom) and node.level == 0 and node.module:
            roots.add(node.module.split(".")[0])
    return roots


class HiddenDependencyTests(unittest.TestCase):
    def test_tools_and_their_tests_import_only_the_standard_library_and_each_other(self):
        offenders = {}
        for path in sorted([*TOOLS.glob("*.py"), *(TOOLS / "tests").glob("*.py")]):
            outside = sorted(imported_roots(path) - STANDARD - LOCAL)
            if outside:
                offenders[path.relative_to(TOOLS.parent).as_posix()] = outside
        self.assertEqual(offenders, {}, "third-party imports under tools/: " + ", ".join(
            f"{name} imports {', '.join(modules)}" for name, modules in offenders.items()))

    def test_the_guard_sees_a_third_party_import(self):
        probe = TOOLS / "tests" / "_probe_not_a_test.py"
        probe.write_text("import pytest\nimport json\nfrom pathlib import Path\n", encoding="utf-8")
        try:
            self.assertEqual(imported_roots(probe) - STANDARD - LOCAL, {"pytest"})
        finally:
            probe.unlink()


if __name__ == "__main__":
    unittest.main()
