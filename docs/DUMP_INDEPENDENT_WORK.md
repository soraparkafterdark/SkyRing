# Work available before owner-data extraction

The owner dump is a fidelity dependency, not a prerequisite for most platform
engineering. Synthetic fixtures are sufficient to build and regression-test
the boundaries below, provided every result remains labelled `local` or
`windows` rather than `source` or `release`.

## Available now

- Versioned combat IR schema, validation, provenance, and golden fixtures.
- Deterministic input/world trace recording and replay.
- Skyrim-authoritative stamina accounting.
- Collision-preserving character-controller movement requests.
- Incoming hit arbitration for dodge, jump, block, guard break, and hurt.
- Target selection, outgoing contact reconciliation, and native-hit identity.
- Hit-stop intent and overlap policy without globally freezing Skyrim.
- C/Rust consequence payloads for elements, statuses, modifiers, poise, and
  stance.
- Separate poise and stance actor adapters.
- Keyboard, mouse, and gamepad edge semantics.
- Deterministic event logs and exact-commit evidence-capture tooling.
- Windows SKSE compilation and synthetic artifact production.

Some of those features ultimately need Skyrim to prove their engine behavior,
but none needs Elden Ring files to reach a reviewable, Windows-compiled state.

## Blocked specifically on owner dumps

- Replacing estimated action timings, root motion, sweeps, stamina costs, and
  motion/guard/stance values with owner-extracted facts.
- Fingerprinting and passing the `source` verification profile.
- Converting and validating Elden Ring animations and sounds.
- Proving animation and IR timing agreement to within one frame.
- Producing a `release` evidence bundle, because release evidence must name the
  exact owner-data fingerprint used by the runtime artifacts.

## Honest estimate

The zero-to-hero swarm manifest contains thirty-three bounded implementation
or acceptance tasks across seventeen ordered phases. Eighteen tasks do not
directly consume owner dumps; six currently require neither owner data nor a
game runtime. Dependencies still govern preparation, so these counts are an
inventory, not permission to run all matching tasks at once.

A useful planning range is:

- **The foundation and several bridge boundaries** can proceed before
  extraction; source validation, animation work, and every honest end-to-end
  fidelity claim still wait for the dump.
- **About half of ship risk** can be retired without the dump, because exact
  animation fidelity and real-runtime integration remain the hardest proof.
- **0% of source or release acceptance** can honestly pass until owner data is
  present. Synthetic success is valuable, but it is not extracted fidelity.

`swarm/tasks.json` is the machine-readable backlog and
`tools/swarm_sandbox.py` creates isolated worktrees, renders task prompts,
enforces path scopes, runs required checks, and records a claim ceiling for
each result. `docs/EXECUTION_PLAN.md` is the human-readable code and merge
contract for the same graph.
