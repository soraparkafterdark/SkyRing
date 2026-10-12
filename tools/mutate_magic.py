#!/usr/bin/env python3
"""Mutation check for the magic core: break the library on purpose, see if a test target notices.

Usage: python3 tools/mutate_magic.py <repo-root> <test-target> [mutant-name ...]
       e.g. python3 tools/mutate_magic.py . magic_adversarial

A test that cannot fail proves nothing. This is how the magic tests were checked: on 2026-10-12 a
set of "all passing" property tests left 6 of 8 deliberate bugs undetected until their generators
were fixed. tools/tests/test_mutate_magic.py fails if a mutant stops matching the source.

Copies <repo-root>/core to a scratch directory (the repository is never modified), applies each
mutant in turn, runs `cargo test --test <test-target>`, and prints CAUGHT (the tests failed, good)
or SURVIVED (the tests still passed: the tests cannot see that bug). Mutants marked EQUIVALENT
are controls that no test should be able to see, and are skipped unless named.
"""
import os
import shutil
import subprocess
import sys
import tempfile

MUTANTS = {
 # name: (file, original, replacement)
 "dead-target-allowed": ("src/magic_world.rs",
   "Some(entry) if !entry.alive => return refuse(self, ImpactRefusal::TargetDead),",
   "Some(entry) if false && !entry.alive => return refuse(self, ImpactRefusal::TargetDead),"),
 "dead-can-cast": ("src/magic_world.rs",
   "if !entry.alive {\n            return CastStart::Failed(WorldError::ActorDead);\n        }",
   "if false {\n            return CastStart::Failed(WorldError::ActorDead);\n        }"),
 "duplicate-resolution": ("src/cast.rs",
   "if entry.resolved.contains(&(target, tick)) {", "if false {"),
 "ledger-unbounded": ("src/cast.rs",
   "if self.entries.len() >= MAX_TRACKED_RELEASES {\n            return Registration::Full;\n        }",
   "if false {\n            return Registration::Full;\n        }"),
 "no-damage-bound": ("src/magic.rs",
   ".clamp(0.0, MAX_SPELL_DAMAGE_MULTIPLIER)", ".max(0.0)"),
 "tick-goes-backwards": ("src/magic_world.rs",
   "        self.tick += 1;\n        let tick = self.tick;",
   "        self.tick = self.tick.wrapping_add(1) % 50;\n        let tick = self.tick;"),
 "replay-ignores-kills": ("src/magic_trace.rs",
   "ScriptStep::Kill { actor } => drop(world.kill(*actor)),", "ScriptStep::Kill { .. } => {}"),
 "area-tick-unbounded": ("src/cast.rs", "if tick >= area.ticks() {", "if false {"),
 "refusal-consumes-release": ("src/magic_world.rs",
   "        match self.actors.get(&target) {\n            None => return refuse(self, ImpactRefusal::UnknownTarget),",
   "        let _ = self.ledger.resolve(release_id, u64::from(target), tick);\n        match self.actors.get(&target) {\n            None => return refuse(self, ImpactRefusal::UnknownTarget),"),
 # controls: cannot be observed through the public API, so they should SURVIVE
 "EQUIVALENT-world-sanitizes-dt": ("src/cast.rs",
   "let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };\n        let Some(active)",
   "let dt = if dt.is_finite() { dt.max(0.0) } else { panic!(\"nan\") };\n        let Some(active)"),
 # controls that SHOULD be caught (the existing tests already catch them)
 "CONTROL-spend-doubled": ("src/cast.rs",
   "self.spent_total += f64::from(needed);\n        self.serial", "self.spent_total += f64::from(needed) * 1.5;\n        self.serial"),
}

def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    root, target, *names = sys.argv[1:]
    names = names or [n for n in MUTANTS if not n.startswith("EQUIVALENT")]
    work = tempfile.mkdtemp(prefix="mutate-magic-")
    try:
        core = os.path.join(work, "core"); shutil.copytree(os.path.join(root, "core"), core, ignore=shutil.ignore_patterns("target"))
        env = dict(os.environ); env["PATH"] = os.path.expanduser("~/.rustup/toolchains/stable-aarch64-apple-darwin/bin") + ":" + env["PATH"]
        base = subprocess.run(["cargo", "test", "--test", target, "--quiet"], cwd=core, capture_output=True, text=True, env=env)
        if base.returncode != 0:
            print("The unmodified code does not pass this target:\n", (base.stdout + base.stderr)[-1500:]); sys.exit(2)
        survivors = []
        for name in names:
            path, old, new = MUTANTS[name]; full = os.path.join(core, path)
            original = open(full).read()
            if old not in original: print(f"{name:34s} NOT APPLICABLE (source changed)"); continue
            open(full, "w").write(original.replace(old, new, 1))
            r = subprocess.run(["cargo", "test", "--test", target, "--quiet"], cwd=core, capture_output=True, text=True, env=env)
            out = r.stdout + r.stderr
            if "error[" in out or ("error:" in out and "test result" not in out): status = "COMPILE-ERROR"
            elif r.returncode: status = "CAUGHT"
            else: status = "SURVIVED"; survivors.append(name)
            print(f"{name:34s} {status}")
            open(full, "w").write(original)
        print("\nSURVIVORS:", survivors or "none")
    finally:
        shutil.rmtree(work, ignore_errors=True)

if __name__ == "__main__": main()
