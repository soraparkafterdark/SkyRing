# Hazard homologation across the Two Lands

Status work for the consequences phase (swarm tasks `effects-runtime` and
`crossover-consequence-slice`) has so far covered discrete contact: melee,
spells, arrows, creature hits. This document and `core/src/hazards.rs`
homologate the rest of the "grossness" — the things you breathe, wade
through, or step in.

## The principle

Elden Ring authors hazards as spatial volumes (poison swamps, scarlet-rot
pools, gas clouds). Skyrim authors them as traps and placed effects (gas
vents, poison runes, spore bursts, dragon breath). Both reduce to the same
physical facts, so the core models the facts and lets bridge adapters carry
the game-specific naming.

A hazard is a **continuous, spatial** source of consequences. It re-applies
the same delivery-neutral payload as every other delivery, on the same six
status meters (`core/src/effects.rs`), so decay, lockout, source
attribution, and Skyrim's authority over damage and resistances stay
uniform. Hazards never subtract health directly.

## Canonical media

| Medium | Elden Ring manifestation | Skyrim manifestation | Status | Element |
| --- | --- | --- | --- | --- |
| PoisonGas | poison swamps, poison mist, poison breath | gas vents, poison runes | Poison | — |
| RotFume | scarlet rot pools, rot breath | none native (reserved) | Rot | — |
| FrostVapor | frost breath, freezing mist | frost traps, frost-breath dragons | Frostbite | — |
| Flame | flame jets, burning ground, fire breath | flame jets, fire runes | — | Fire |
| ShockField | lightning fields | shock runes, lightning vents | — | Lightning |
| Lava | lava pools | absent natively; mods only | — | Fire |
| ExhaustFume | sleep gas | vaporizer/sleep traps | Sleep | — |
| SporeBurst | spore-burst flora | spore traps | Poison | — |
| DeepWater | shock-conducting water | conductive water | — | — |
| Mire | mud, tar, deepwater slow | mud and tar zones | Sleep | — |

This table is encoded as `MediumCrosswalk` and the `CROSSWALK` constant in
`core/src/hazards.rs`; the unit test `crosswalk_covers_every_medium_exactly_once`
guarantees code and document cannot drift apart.

## The one-channel rule

Each medium delivers at most one semantic consequence channel:

- a status-bearing medium (gas, rot, spores, fumes, mire) delivers **no**
  elemental damage share; the proc is the payload, and Skyrim's native
  resistance path stays untouched;
- an elemental medium (flame, shock, lava) delivers **no** status buildup;
  Skyrim applies its own burn/resist logic through the existing elemental
  shares in `core/src/consequences.rs`.

This is asserted by
`every_medium_maps_to_at_most_one_channel_of_each_kind`.

## Exposure geometry

`ExposureState` grades how much of an actor sits inside a volume:

- `Immersed` — full per-second rate;
- `Wading` — `wading_scale` (e.g. shallow swamp edge);
- `Brushed` — `brushed_scale` (boundary contact, brief cloud overlap);
- `Clear` — no evaluation.

Scales are Skyrim-facing balance inputs on `HazardDefinition`, never
engine constants, honoring the IR rule that forbids absolute source-game
values.

## Resistance and immunity

`HazardVulnerability::expose` takes the same resistance convention as
`StatusSet::apply`: 1 neutral, 0 immune, above 1 vulnerable. Temporary
immunity (`grant_immunity`) covers wading boots, resist potions, and
talisman-like protections; `tick` decrements it. `clear` resets on respawn
or save load.

## Deliberate scope limits

- No environmental slow mechanics yet — `Mire` maps to Sleep buildup but
  movement-speed modifiers belong to the modifier stack in `effects.rs`,
  which the bridge composes at the same evaluation.
- No airborne-vs-submerged distinction yet; one `ExposureState` per actor
  per instant. Add a second axis only when a slice needs it.
- `RotFume` and `Lava` are reserved crosswalk entries: Skyrim has no native
  twin, so modded content decides later whether to populate them.
- Damage-over-time from hazards flows as a per-second semantic share, not
  a new damage pipeline; the bridge maps it onto Skyrim's existing
  resistance path exactly as elemental weapon shares are handled.

## What this unblocks

- `effects-runtime.subtasks.statuses`: hazard exposure now feeds the same
  six meters, so CrossOver evidence for continuous exposure can be gathered
  alongside discrete-contact evidence.
- The Charter's Alchemy promise: grease and consumable resistance items
  have a concrete job (`grant_immunity` and resistance multipliers) the
  moment hazards exist.
