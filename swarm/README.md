# AI swarm sandbox

This directory defines bounded tasks that coding agents can attempt without
sharing a mutable checkout. It is an implementation harness, not a claim that
parallel agents can replace source data or runtime testing.

The complete phase graph and target interfaces are documented in
[`docs/EXECUTION_PLAN.md`](../docs/EXECUTION_PLAN.md). `tasks.json` is schema
version 2 and is the machine-readable source for worker prompts.

## Safety model

- One task gets one detached Git worktree and one immutable base commit.
- The task contract limits which paths may change and which checks must pass.
- Agents do not push, alter the Charter, or commit generated owner data.
- The orchestrator reviews and integrates results; a sandbox passing does not
  merge itself.
- Every task declares its maximum defensible acceptance level. Synthetic work
  never claims source or release completion.
- The manifest orders work into product phases. Every task contains bounded
  subtasks, concrete code touchpoints, expected artifacts, dependencies, and a
  machine-checked completion condition; generated worker prompts include all
  of them.

Inspect the environment and current task pool:

```bash
python3 tools/swarm_sandbox.py doctor
python3 tools/swarm_sandbox.py list --actionable
python3 tools/swarm_sandbox.py prompt skyrim-stamina-authority
```

`--actionable` means no owner dump or game runtime is required to work on the
task; the dependency list in its prompt still controls integration order.

Create an isolated worker checkout outside the repository:

```bash
python3 tools/swarm_sandbox.py prepare \
  skyrim-stamina-authority \
  /tmp/skyring-swarm-stamina
```

The worktree contains an ignored `SWARM_TASK.md` prompt and
`.swarm-task.json` metadata. After the agent finishes, enforce its path scope
and required checks from the main checkout:

```bash
python3 tools/swarm_sandbox.py check \
  skyrim-stamina-authority \
  /tmp/skyring-swarm-stamina
```

The checker writes an ignored report under `verification/out/` in the worker
checkout. It intentionally has no delete command: cleanup remains an explicit
orchestrator operation so the harness cannot erase an unreviewed result.
