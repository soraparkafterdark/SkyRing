#!/usr/bin/env python3
"""Prepare and verify isolated SkyRing task worktrees for coding agents."""

from __future__ import annotations

import argparse
import fnmatch
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "swarm" / "tasks.json"
TASK_FILE = ".swarm-task.json"
PROMPT_FILE = "SWARM_TASK.md"
TASK_ID = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
REQUIREMENTS = {"owner_dump", "skyrim_runtime", "crossover"}
CEILINGS = {"local", "windows", "source", "runtime", "release"}


class SandboxError(RuntimeError):
    pass


def run(command: list[str], cwd: Path = ROOT, check: bool = True) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        command,
        cwd=cwd,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if check and completed.returncode != 0:
        raise SandboxError(f"command failed ({completed.returncode}): {' '.join(command)}\n{completed.stdout}")
    return completed


def load_manifest(path: Path = DEFAULT_MANIFEST) -> dict[str, Any]:
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SandboxError(f"cannot read swarm manifest: {error}") from error
    validate_manifest(manifest)
    return manifest


def validate_manifest(manifest: dict[str, Any]) -> None:
    if manifest.get("schema_version") != 1:
        raise SandboxError("unsupported swarm manifest schema")
    checks = manifest.get("checks")
    if not isinstance(checks, dict) or not checks:
        raise SandboxError("swarm manifest must define checks")
    for name, command in checks.items():
        if not isinstance(name, str) or not isinstance(command, list) or not command:
            raise SandboxError("every swarm check must be a named non-empty argv list")
        if not all(isinstance(part, str) and part for part in command):
            raise SandboxError(f"invalid command for check {name!r}")

    tasks = manifest.get("tasks")
    if not isinstance(tasks, list) or not tasks:
        raise SandboxError("swarm manifest must define tasks")
    by_id: dict[str, dict[str, Any]] = {}
    for task in tasks:
        if not isinstance(task, dict):
            raise SandboxError("every swarm task must be an object")
        task_id = task.get("id")
        if not isinstance(task_id, str) or not TASK_ID.fullmatch(task_id):
            raise SandboxError(f"invalid swarm task id: {task_id!r}")
        if task_id in by_id:
            raise SandboxError(f"duplicate swarm task id: {task_id}")
        by_id[task_id] = task
        if not isinstance(task.get("title"), str) or not task["title"].strip():
            raise SandboxError(f"task {task_id} has no title")
        if not isinstance(task.get("priority"), int) or task["priority"] < 1:
            raise SandboxError(f"task {task_id} has invalid priority")
        requirements = task.get("requires")
        if not isinstance(requirements, dict) or set(requirements) != REQUIREMENTS:
            raise SandboxError(f"task {task_id} must declare exactly {sorted(REQUIREMENTS)}")
        if not all(isinstance(value, bool) for value in requirements.values()):
            raise SandboxError(f"task {task_id} requirements must be booleans")
        if task.get("acceptance_ceiling") not in CEILINGS:
            raise SandboxError(f"task {task_id} has invalid acceptance ceiling")
        allowed = task.get("allowed_paths")
        if not isinstance(allowed, list) or not allowed:
            raise SandboxError(f"task {task_id} must declare allowed paths")
        for pattern in allowed:
            if not isinstance(pattern, str) or pattern.startswith(("/", "../")) or "/../" in pattern:
                raise SandboxError(f"task {task_id} has unsafe allowed path: {pattern!r}")
        required_checks = task.get("checks")
        if not isinstance(required_checks, list) or not required_checks:
            raise SandboxError(f"task {task_id} must declare checks")
        unknown = set(required_checks) - set(checks)
        if unknown:
            raise SandboxError(f"task {task_id} uses unknown checks: {sorted(unknown)}")
        if not isinstance(task.get("done_when"), str) or not task["done_when"].strip():
            raise SandboxError(f"task {task_id} has no completion condition")

    for task_id, task in by_id.items():
        dependencies = task.get("depends_on")
        if not isinstance(dependencies, list) or not all(isinstance(item, str) for item in dependencies):
            raise SandboxError(f"task {task_id} has invalid dependencies")
        missing = set(dependencies) - set(by_id)
        if missing:
            raise SandboxError(f"task {task_id} has missing dependencies: {sorted(missing)}")
        if task_id in dependencies:
            raise SandboxError(f"task {task_id} depends on itself")

    visiting: set[str] = set()
    visited: set[str] = set()

    def visit(task_id: str) -> None:
        if task_id in visiting:
            raise SandboxError(f"dependency cycle includes {task_id}")
        if task_id in visited:
            return
        visiting.add(task_id)
        for dependency in by_id[task_id]["depends_on"]:
            visit(dependency)
        visiting.remove(task_id)
        visited.add(task_id)

    for task_id in by_id:
        visit(task_id)


def task_map(manifest: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {task["id"]: task for task in manifest["tasks"]}


def task_is_dump_independent(task: dict[str, Any]) -> bool:
    return not task["requires"]["owner_dump"]


def task_is_locally_actionable(task: dict[str, Any]) -> bool:
    return not any(task["requires"].values())


def find_cargo() -> str | None:
    found = shutil.which("cargo")
    if found:
        return found
    candidates = sorted((Path.home() / ".rustup" / "toolchains").glob("stable-*/bin/cargo"))
    return str(candidates[-1]) if candidates else None


def resolve_command(command: list[str]) -> list[str]:
    if command[0] != "cargo" or shutil.which("cargo"):
        return command
    cargo = find_cargo()
    return [cargo, *command[1:]] if cargo else command


def path_allowed(path: str, patterns: list[str]) -> bool:
    normalized = path.replace("\\", "/")
    while normalized.startswith("./"):
        normalized = normalized[2:]
    return any(fnmatch.fnmatchcase(normalized, pattern) for pattern in patterns)


def render_prompt(task: dict[str, Any], base: str) -> str:
    requirements = ", ".join(name for name, required in task["requires"].items() if required) or "none"
    allowed = "\n".join(f"- `{pattern}`" for pattern in task["allowed_paths"])
    checks = "\n".join(f"- `{name}`" for name in task["checks"])
    dependencies = ", ".join(task["depends_on"]) or "none"
    return f"""# Swarm task: {task['id']}

## Objective

{task['title']}

## Completion condition

{task['done_when']}

## Contract

- Base commit: `{base}`
- Dependencies: {dependencies}
- External requirements: {requirements}
- Maximum defensible claim: `{task['acceptance_ceiling']}`
- Do not push. Do not change the Charter. Do not add proprietary data.

## Allowed paths

{allowed}

## Required checks

{checks}
"""


def changed_paths(workspace: Path, base: str) -> list[str]:
    tracked = run(["git", "diff", "--name-only", base, "--"], cwd=workspace).stdout.splitlines()
    untracked = run(["git", "ls-files", "--others", "--exclude-standard"], cwd=workspace).stdout.splitlines()
    return sorted(set(path for path in tracked + untracked if path not in {TASK_FILE, PROMPT_FILE}))


def safe_workspace(path: Path) -> Path:
    resolved = path.expanduser().resolve()
    forbidden = {Path("/").resolve(), Path.home().resolve(), ROOT.resolve()}
    if resolved in forbidden or resolved.parent == Path("/"):
        raise SandboxError(f"refusing unsafe workspace path: {resolved}")
    return resolved


def command_doctor(args: argparse.Namespace, manifest: dict[str, Any]) -> int:
    tasks = manifest["tasks"]
    actionable = [task for task in tasks if task_is_locally_actionable(task)]
    dump_independent = [task for task in tasks if task_is_dump_independent(task)]
    blocked_on_dump = [task for task in tasks if task["requires"]["owner_dump"]]
    repo_ok = run(["git", "rev-parse", "--is-inside-work-tree"], check=False).returncode == 0
    tools = {
        "git": bool(shutil.which("git")),
        "python3": bool(shutil.which("python3")),
        "cargo": find_cargo() is not None,
    }
    status = "PASS" if repo_ok and all(tools.values()) else "FAIL"
    report = {
        "schema_version": 1,
        "status": status,
        "manifest": str(DEFAULT_MANIFEST.relative_to(ROOT)),
        "tasks_total": len(tasks),
        "locally_actionable": len(actionable),
        "dump_independent": len(dump_independent),
        "blocked_on_owner_dump": len(blocked_on_dump),
        "tools": tools,
        "repository": repo_ok,
    }
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(f"{status} swarm sandbox")
        print(f"  {len(actionable)}/{len(tasks)} tasks are locally actionable now")
        print(f"  {len(dump_independent)}/{len(tasks)} tasks do not require owner dumps")
        print(f"  {len(blocked_on_dump)}/{len(tasks)} tasks are blocked on owner dumps")
        print(f"  tools: {', '.join(f'{name}={value}' for name, value in tools.items())}")
    return 0 if status == "PASS" else 1


def command_list(args: argparse.Namespace, manifest: dict[str, Any]) -> int:
    tasks = sorted(manifest["tasks"], key=lambda task: (task["priority"], task["id"]))
    if args.actionable:
        tasks = [task for task in tasks if task_is_locally_actionable(task)]
    if args.json:
        print(json.dumps(tasks, indent=2))
        return 0
    print("ID                               P  DUMP  CEILING  TITLE")
    for task in tasks:
        dump = "yes" if task["requires"]["owner_dump"] else "no"
        print(f"{task['id']:<32} {task['priority']:<2} {dump:<5} {task['acceptance_ceiling']:<8} {task['title']}")
    return 0


def get_task(manifest: dict[str, Any], task_id: str) -> dict[str, Any]:
    try:
        return task_map(manifest)[task_id]
    except KeyError as error:
        raise SandboxError(f"unknown swarm task: {task_id}") from error


def command_prompt(args: argparse.Namespace, manifest: dict[str, Any]) -> int:
    task = get_task(manifest, args.task)
    base = run(["git", "rev-parse", args.ref]).stdout.strip()
    print(render_prompt(task, base), end="")
    return 0


def command_prepare(args: argparse.Namespace, manifest: dict[str, Any]) -> int:
    task = get_task(manifest, args.task)
    if not task_is_locally_actionable(task) and not args.allow_blocked:
        needed = ", ".join(name for name, value in task["requires"].items() if value)
        raise SandboxError(f"task {task['id']} requires {needed}; pass --allow-blocked only for scaffolding")
    workspace = safe_workspace(Path(args.workspace))
    if workspace.exists():
        raise SandboxError(f"workspace already exists: {workspace}")
    base = run(["git", "rev-parse", args.ref]).stdout.strip()
    run(["git", "worktree", "add", "--detach", str(workspace), base])
    metadata = {"schema_version": 1, "task": task["id"], "base": base}
    (workspace / TASK_FILE).write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    (workspace / PROMPT_FILE).write_text(render_prompt(task, base), encoding="utf-8")
    print(f"prepared {task['id']} at {workspace}")
    print(f"task prompt: {workspace / PROMPT_FILE}")
    return 0


def command_check(args: argparse.Namespace, manifest: dict[str, Any]) -> int:
    workspace = Path(args.workspace).expanduser().resolve()
    task = get_task(manifest, args.task)
    metadata_path = workspace / TASK_FILE
    metadata: dict[str, Any] = {}
    if metadata_path.is_file():
        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        if metadata.get("task") != task["id"]:
            raise SandboxError(f"workspace belongs to task {metadata.get('task')!r}")
    base = args.base or metadata.get("base")
    if not isinstance(base, str) or not base:
        raise SandboxError("check requires --base or prepared workspace metadata")
    paths = changed_paths(workspace, base)
    violations = [path for path in paths if not path_allowed(path, task["allowed_paths"])]
    checks: list[dict[str, Any]] = []
    if violations:
        checks.append({"name": "scope", "status": "FAIL", "detail": violations})
    else:
        checks.append({"name": "scope", "status": "PASS", "detail": paths})
    whitespace = run(["git", "diff", "--check", base, "--"], cwd=workspace, check=False)
    checks.append({
        "name": "diff_check",
        "status": "PASS" if whitespace.returncode == 0 else "FAIL",
        "detail": whitespace.stdout.strip(),
    })
    for check_name in task["checks"]:
        completed = run(resolve_command(manifest["checks"][check_name]), cwd=workspace, check=False)
        checks.append({
            "name": check_name,
            "status": "PASS" if completed.returncode == 0 else "FAIL",
            "detail": completed.stdout[-4000:],
        })
    failed = [check for check in checks if check["status"] != "PASS"]
    report = {
        "schema_version": 1,
        "task": task["id"],
        "base": base,
        "head": run(["git", "rev-parse", "HEAD"], cwd=workspace).stdout.strip(),
        "status": "FAIL" if failed else "PASS",
        "claim_ceiling": task["acceptance_ceiling"],
        "changed_paths": paths,
        "checks": checks,
    }
    output = Path(args.report) if args.report else workspace / "verification" / "out" / f"swarm-{task['id']}.json"
    if not output.is_absolute():
        output = workspace / output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"{report['status']} {task['id']}; report: {output}")
    for check in checks:
        print(f"  {check['status']:<4} {check['name']}")
    return 1 if failed else 0


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    commands = result.add_subparsers(dest="command", required=True)
    doctor = commands.add_parser("doctor", help="validate the manifest and host prerequisites")
    doctor.add_argument("--json", action="store_true")
    listing = commands.add_parser("list", help="list bounded swarm tasks")
    listing.add_argument("--actionable", action="store_true")
    listing.add_argument("--json", action="store_true")
    prompt = commands.add_parser("prompt", help="render the immutable task contract")
    prompt.add_argument("task")
    prompt.add_argument("--ref", default="HEAD")
    prepare = commands.add_parser("prepare", help="create an isolated detached worktree")
    prepare.add_argument("task")
    prepare.add_argument("workspace")
    prepare.add_argument("--ref", default="HEAD")
    prepare.add_argument("--allow-blocked", action="store_true")
    check = commands.add_parser("check", help="enforce scope and run a task's required checks")
    check.add_argument("task")
    check.add_argument("workspace")
    check.add_argument("--base")
    check.add_argument("--report")
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        manifest = load_manifest(args.manifest)
        return {
            "doctor": command_doctor,
            "list": command_list,
            "prompt": command_prompt,
            "prepare": command_prepare,
            "check": command_check,
        }[args.command](args, manifest)
    except (SandboxError, OSError, json.JSONDecodeError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
