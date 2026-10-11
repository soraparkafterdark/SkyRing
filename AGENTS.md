# SkyRing agent contract

This file is the short operational contract for automated coding harnesses.
Read `Charter.md` and `architecture.md` before changing gameplay. If documents
disagree, the Charter wins, then the architecture. Do not infer completion
from a successful native build.

## End goal

Ship a Skyrim SE/AE combat integration that feels like Elden Ring because it
preserves action commitment, timing, movement, hit geometry, i-frames, poise,
and stance, while Skyrim continues to own actors, progression, equipment base
damage, armour, resistances, perks, difficulty, quests, and saves. The final
acceptance machine is Skyrim running through CrossOver on Apple Silicon.

The first proof is one complete colossal-sword vertical slice against one test
actor. It is complete only when input, collision-preserving movement,
animation, outgoing contact, incoming dodge/block resolution, Skyrim-native
damage, stamina, hit-stop, deterministic logs, and one-frame timing agreement
all work in first- and third-person in the CrossOver runtime. Switching
perspective during an action must preserve its identity, phase, movement, hit
geometry, stamina, i-frames, buffered input, and resolved contacts.

## Mandatory order and product decisions

1. Keep builds and verification reproducible.
2. Finish the melee vertical slice.
3. Tune action and armament balance.
4. Integrate elements, statuses, buffs, and debuffs.
5. Integrate poise and stance as separate systems.
6. Prove shared ranged delivery with one bow and physical, elemental, and
   buildup arrows; Skyrim remains authoritative for projectile flight and hit.
7. Only then implement magic delivery on that ranged/effect foundation.
8. Add breadth and compatibility after the slices are proven.
9. Megafauna is last and must never block combat work.

- Consumables remain instant and animation-less. Revisit committed item-use
  animation only when throwables are implemented.
- Imported armaments may be fun-biased but never exceed the documented 1.15x
  cap without a Charter change and regression evidence.
- Never add attribute, skill, level, perk, grip, or moveset requirements to
  weapons. Acquisition is the only access gate; progression may improve
  results but never eligibility.
- Do not port Elden Ring's absolute damage, HP, defence, or scaling model.
- Keep perspective out of combat state. First- and third-person animation and
  cameras are presentations of one authoritative action clock and geometry.
- Do not commit generated Elden Ring data, animations, sound, or other derived
  proprietary assets.

## Verification discipline

Run `python3 tools/verify.py --profile local` before committing ordinary code.
Use `--profile source` only when owner-generated `extracted.rs` is present.
Use `--profile release` only with a matching local runtime evidence manifest.

Never describe a lower profile as proof of a higher one:

- `local` proves synthetic plumbing, deterministic core tests, ABI layout,
  bridge geometry, and balance tooling.
- `source` additionally proves the core was built and tested against locally
  extracted owner data and records its fingerprint.
- `release` additionally requires Windows DLL artifacts and a CrossOver
  vertical-slice evidence bundle tied to the exact Git commit and source-data
  fingerprint.

A missing platform, game dump, or runtime is `BLOCKED`, not `PASS`. A skipped
required gate is a failure. Record estimates as estimates; agreement between
two consumers of the same estimate is not extraction validation.

## Working rules

- Keep changes small enough that a failed gate identifies one cause.
- Add a regression test for every fixed defect that can be reproduced outside
  Skyrim.
- Preserve the C/Rust ABI with explicit size and offset checks.
- Route all delivery types through the shared consequence contract.
- Keep engine calls behind narrow bridge adapters; pure math belongs in code
  that runs natively on macOS.
- Do not push generated evidence or secrets. Runtime reports and proprietary
  generated data are intentionally ignored.
