# Accuracy and balance audit: melee, bows, throwables, consumables

Companion to `hazard-homologation.md`. This records, per the AGENTS.md rule
that estimates are recorded as estimates, how each delivery's numbers were
chosen, what is calibrated play-feel versus extracted fact, and what changes
when owner dumps arrive.

## Standing rule

Skyrim owns absolute damage, HP, armour, resistances, perks, difficulty, and
saves. The core exports only bounded relative multipliers. Nothing below
bypasses that, and no number here is an absolute source-game value.

## Melee envelopes (existing, audited)

The `ActionRole` envelopes in `core/src/balance.rs` were checked against
both games' payoffs:

| Role | Default | ER justification | Skyrim counterpart |
| --- | --- | --- | --- |
| Light | 1.00 | The baseline light hit is the unit of value | One-handed swing |
| Heavy | 1.55 | ER heavies cost ~2x stamina for ~1.5x damage: a commitment tax, not a linear trade | Skyrim power attacks pay 2x stamina for roughly 1.5-1.8x |
| ChargedHeavy | 2.10 | Full charge adds a windup you can be hit through; the payoff is real but bounded | Skyrim has no native charge; this is the ER import, capped well below a 2x jump |
| JumpHeavy | 1.85 | ER jump heavies are strong but leave you fully committed on landing | Skyrim power-landing strikes |
| GuardCounter | 1.75 | ER guard counters reward the parry-block-read loop | Skyrim bash-counter windows |
| Paired | 0.75/contact | Paired strings land multiple contacts; per-contact budget keeps the action total bounded | Power-stance equivalents |

Audit conclusion: the table already encodes both games' economics. The one
deliberate divergence is ChargedHeavy at 2.10 rather than something nearer
3.0: with Skyrim's health pools and no ER poise-break loop in yet, a higher
default would make charge-spam dominant before the stance system lands.

## Bows (`core/src/delivery.rs`)

- **Skyrim owns the curve shape.** Draw damage rises continuously; releasing
  early fires a weaker arrow. This is Skyrim-native behavior and is kept.
- **ER owns the calibration points.** In Elden Ring the quick shot deals
  essentially full arrow damage and full draw buys arrow velocity rather
  than damage. The curve is therefore front-loaded: at `quick_draw_seconds`
  (0.40 s) the shot already delivers `quick_draw_fraction` (0.80) of
  full-draw damage; the remaining 0.70 s of draw adds the last 20 percent.
  A bridge wanting a pure Skyrim linear curve sets
  `quick_draw_seconds == full_draw_seconds`.
- **Commitment is ER's.** Draw cannot stack a second action; dodge cancels a
  draw before release but never a recovery (queued, not honored). Skyrim
  cannot sprint while drawn; the bridge enforces that from the same
  `DeliveryPhase`.
- **Stamina:** ER charges a shot; Skyrim natively does not. The profile
  default is 8 per release with a note that the bridge may zero it to stay
  Skyrim-authentic. Recorded as a decision the owner can flip.
- **Arrows:** seven profiles homologate both games. Skyrim owns arrow base
  damage, Archery skill, perks, and sneak multipliers. Status arrows carry
  buildup as a *fraction of the target threshold*: bleed 0.45, frost 0.40,
  poison 0.35, sleep 0.60, madness 0.50 — recorded as estimates matching
  how ER's arrows proc (sleep arrows are famously one-to-two-shot procs on
  ordinary targets; poison needs three to four). Madness is player-venom
  and beasts ignore it through immunity rules.

## Throwables

Elden Ring-authored, Skyrim-delivered:

- Wind-up is committed and cannot stack (0.65-0.70 s for pots, 0.45 s for
  throwing daggers). The throw fires at wind-up end; ER does not let you
  hold a pot, and the state machine matches that.
- Recovery is committed: 0.50 s for pots, 0.35 s for daggers.
- Damage 0.70-1.10 of one light hit, stance pressure 0.30-0.50: pots in ER
  are value-efficient but rarely poise-breaking. Recorded as estimates.
- The AoE radius in metres is a bridge input; Skyrim's projectile system
  realizes flight, arc, and impact.

## Consumables

The Charter's standing decision is that consumables are instant and
animation-less. The module enforces exactly that: every profile has zero
windup, and a committed-flask profile is rejected by `valid()` while
`COMMITTED_CONSUMABLES_ENABLED` is false. Enabling the ER-style committed
flask (~1 s full commitment, half walk speed) is a Charter change with
regression evidence, per AGENTS.md, not a code edit. Greases apply their
modifier through the shared stack (additive, refresh/strongest rules), which
is what gives Alchemy its combat job once statuses land.

## What the owner dumps will change

Nothing structural. The dump pipeline (`tools/owner_data.py`,
`extracted.rs`) feeds source motion values into `damage_multiplier`, which
clamps into these envelopes; the delivery profiles similarly take
bridge-supplied thresholds and scales. When real ER motion values arrive,
expect envelope *defaults* to move inside the min/max bounds and the arrow
buildup fractions to be re-derived from real buildup stats. The commitment
timings should be replaced with extracted animation lengths at that point;
they are play-feel estimates until then.
