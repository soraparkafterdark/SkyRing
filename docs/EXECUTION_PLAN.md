# SkyRing zero-to-hero execution plan

This is the operational expansion of `Charter.md` and `architecture.md`.
Those documents remain authoritative. The machine-readable form of this plan
is `swarm/tasks.json`; generated swarm prompts must come from that manifest,
not from an agent's memory of this page.

No task is considered complete because matching code happens to exist. Existing
code is a head start that must still satisfy the task's `done_when`, subtasks,
checks, evidence requirement, and acceptance ceiling.

## Non-negotiable execution rules

1. Merge dependencies before preparing a dependent task.
2. Give one bounded manifest task to one isolated worktree.
3. Never let a worker change `Charter.md`, push, or commit owner-generated
   data, animations, audio, logs, credentials, or runtime evidence.
4. Run the task's declared checks in its worktree, review the diff, then merge.
5. Rebase or recreate sibling work after an ABI/schema change; do not resolve
   semantic conflicts by taking both sides.
6. `local`, `windows`, `source`, `runtime`, and `release` are claim ceilings,
   not progress percentages.
7. A missing owner dump, Skyrim runtime, or CrossOver machine is `BLOCKED`.
8. Ship the combat platform before megafauna. Megafauna may extend a shipped
   platform; it may never delay the combat release.

## Critical path

```text
foundation
  -> owner-data import + source action validation + animation conversion
  -> dual-perspective colossal-sword CrossOver slice
  -> action/armament balance
  -> shared consequences
  -> poise / stance / ripostes
  -> projectile adapter -> bow slice
  -> gear axis -> catalysts -> magic slice
  -> enemy symmetry
  -> breadth / saves / Gate to Sovngarde compatibility
  -> Torrent
  -> priority authored encounter
  -> COMBAT PLATFORM RELEASE
  -> megafauna (last, optional to the combat release)
  -> complete zero-to-hero release
```

The first CrossOver melee gate is intentionally expensive. Nothing downstream
may reinterpret a native build, synthetic trace, or converted animation as a
substitute for that proof.

## Target code contracts

These blocks name the intended boundaries. Field spelling may change during an
ABI revision, but ownership and information flow may not.

### Combat IR

```rust
struct ActionRecord {
    id: StableActionId,
    provenance: Provenance,       // extracted | authored | estimated
    duration_s: f32,
    stamina_cost: f32,
    input_windows: Vec<Window>,
    cancel_windows: Vec<Window>,
    hit_windows: Vec<HitWindow>,
    iframe_windows: Vec<Window>,
    hyperarmour_windows: Vec<Window>,
    root_motion_m: Vec<MotionSample>,
    weapon_sweeps_m: Vec<SweepSample>,
    balance_facts: RelativeCombatFacts,
    presentation: PresentationRefs,
}

struct RangedActionExtension {
    draw_windows: Vec<Window>,
    release_windows: Vec<Window>,
    recovery_windows: Vec<Window>,
    projectile_profile: StableProjectileProfileId,
}

// Forbidden in gameplay IR:
// Elden Ring HP damage, attack rating, defence, stat scaling curves.
```

Every record uses seconds and metres internally. Generated source records carry
the source build and bundle fingerprint. Unsupported IR versions fail rather
than silently defaulting fields.

### Core/bridge ABI

```c
typedef struct ErActionView {
    uint32_t action_serial;
    float action_frame;
    ErAction action_kind;
    uint32_t action_direction;
    ErVec3 desired_position;
    float desired_heading;
    uint8_t invincible;
    uint8_t interruptible;
} ErActionView;

typedef struct ErContactIntent {
    uint32_t action_serial;
    uint32_t hit_id;
    ErSweep sweep;
    float bounded_damage_multiplier;
    float poise_pressure;
    float stance_pressure;
    float hit_stop_seconds;
} ErContactIntent;

typedef struct ErDeliveryIdentity {
    uint64_t delivery_id;
    uint64_t release_id;          // zero for non-projectile delivery
    uint32_t source_form_id;
    uint32_t target_form_id;
    uint32_t action_serial;
    uint32_t hit_id;
} ErDeliveryIdentity;
```

ABI revisions require matching Rust/C size, offset, enum, and invalid-value
tests. A perspective flag does not belong in core combat input or state.

### Narrow Skyrim adapters

```cpp
struct MotionAdapter {
    MovementResult ApplyThroughCharacterController(
        const MovementRequest& request);
};

struct NativeHitGateway {
    SubmissionResult SubmitMelee(
        const ErDeliveryIdentity&, RE::Actor&, RE::TESObjectWEAP&);
    SubmissionResult ObserveProjectileImpact(
        const ErDeliveryIdentity&, const RE::TESHitEvent&);
};

struct PresentationAdapter {
    void StartBothGraphs(const ActionPresentation&);
    void SeekInactiveGraph(uint32_t actionSerial, float actionFrame);
    void SetPerspective(Perspective); // presentation only
};

struct ActorFactsAdapter {
    GroundFact ReadGround(const RE::PlayerCharacter&);
    TargetFact ReadLockTarget(const RE::PlayerCharacter&);
    TargetRules ReadConsequenceRules(const RE::Actor&);
};
```

CommonLib/Skyrim calls stay behind these adapters. Deterministic selection,
identity checks, geometry, clamping, and stacking rules remain in host-testable
plain C++ or Rust.

### Deterministic event stream

```rust
enum CombatEvent {
    InputAccepted { tick, input_id },
    ActionStarted { tick, action_serial, action_id },
    MotionRequested { tick, action_serial, delta },
    MotionResolved { tick, action_serial, actual_position },
    ContactObserved { tick, delivery, target },
    NativeHitSubmitted { tick, delivery },
    ContactConfirmed { tick, delivery },
    IncomingResolved { tick, source, result },
    StaminaChanged { tick, source, amount },
    ConsequenceResolved { tick, delivery, outcome },
    HitStopChanged { tick, actors, remaining_ticks },
    PerspectiveObserved { tick, perspective },
}
```

`PerspectiveObserved` is permitted in logs and presentation tests only. Adding
or removing it from a replay must not change any other event bytes.

### Shared consequence request

```rust
struct ConsequenceRequest {
    delivery: DeliveryIdentity,
    bounded_action_multiplier: f32,
    armament_multiplier: f32,        // clamped again to <= 1.15
    elements: ElementalShares,       // total <= 1.0
    poise_pressure: f32,
    stance_pressure: f32,
    status_buildup: [f32; 6],
    modifiers: Vec<Modifier>,
}

struct ConsequenceOutcome {
    skyrim_damage_multiplier: f32,
    poise: PoiseOutcome,
    stance: StanceOutcome,
    status_procs: [Option<StatusProc>; 6],
    modifiers_applied: u32,
}
```

This layer never subtracts health. Skyrim's native melee/spell/projectile path
owns base damage, armour, resistance, perks, difficulty, kill attribution, and
gore.

### Projectile correlation

```text
committed action reaches release window
  -> snapshot bow/catalyst + ammo/spell payload
  -> allocate delivery_id and release_id
  -> ask Skyrim to launch the projectile
  -> Skyrim simulates flight and collision
  -> bridge observes matching native impact
  -> reject stale / unowned / duplicate / mismatched impact
  -> submit one shared consequence request
```

Equipment changes after release cannot mutate the snapshot. Misses and world
impacts create diagnostic events but no actor consequences.

### Dual-perspective presentation

```text
                 one action_serial + action_frame
                              |
                 +------------+------------+
                 |                         |
       third-person full body       first-person arms/weapon
                 |                         |
                 +------------+------------+
                              |
                same root motion / sweep / i-frames
```

Both graphs start together when possible. If Skyrim does not advance an
inactive graph, the presentation adapter seeks it to the authoritative action
frame on view change. Restarting an action, replaying an event, moving a hit
window, or changing a target during a toggle is a correctness failure.

### Save envelope

```rust
struct SkyRingSaveV1 {
    schema_version: u32,
    actor_effect_state: Vec<SavedActorEffects>,
    upgrade_state: Vec<SavedUpgrade>,
    torrent_state: Option<SavedTorrent>,
    // Never Skyrim health, stamina, magicka, inventory, perks, quests, or XP.
}
```

Every saved record uses stable Skyrim identities and explicit migration.
Unsupported downgrade/upgrade paths fail safely and preserve the original
save.

## Phase and task ledger

The manifest is the exact source of task definitions. This table is the merge
order and review checkpoint.

| Phase | Swarm tasks | Exit proof |
| --- | --- | --- |
| Foundation | `ir-schema-and-fixtures`, `deterministic-traces`, `reproducible-builds`, `runtime-evidence-harness` | Local/Windows plumbing only |
| Owner data | `owner-dump-import`, `source-action-table`, `animation-pipeline` | Source fingerprint + Skyrim animation validation |
| Colossal sword | `input-world-adapters`, `collision-motion-runtime`, `native-melee-delivery`, `incoming-defense-stamina`, `hit-stop-runtime-log`, `dual-perspective-runtime`, `crossover-colossal-sword` | First release-profile vertical slice |
| Balance | `action-armament-balance` | Role envelopes and 1.15x cap |
| Consequences | `consequence-abi-state`, `effects-runtime`, `crossover-consequence-slice` | Independent five-channel CrossOver proof |
| Posture | `posture-runtime`, `crossover-posture-slice` | Trade + earned-riposte proof |
| Bows | `projectile-adapter`, `bow-arrow-runtime`, `crossover-bow-slice` | Longbow/three-arrow proof |
| Gear axis | `gear-axis-runtime` | Stone tracks + catalyst ratios + playtest |
| Catalysts | `catalyst-runtime-slice` | Hand dedication/passive build proof |
| Magic | `magic-runtime-slice` | Seven-case magic proof |
| Enemy symmetry | `enemy-symmetry-slice` | Readable/punishable enemy proof |
| Breadth | `breadth-save-compatibility` | Content, save, and GTS matrix |
| Torrent | `torrent-runtime-slice` | Traversal/mounted combat proof |
| Encounters | `authored-encounter` | Bleak Falls encounter proof |
| Ship | `combat-platform-release` | Shippable combat platform |
| Megafauna, last | `megafauna-last` | Optional bounded creature work |
| Complete | `zero-to-hero-release` | Final full regression and archive |

## Parallel work rules

Parallelism is allowed only across tasks whose dependencies are already merged
and whose `allowed_paths` do not create a semantic ABI conflict.

Safe early lanes after this plan is committed:

```text
Lane A: ir-schema-and-fixtures -> deterministic-traces
Lane B: reproducible-builds
Lane C: runtime-evidence-harness (after trace/build contracts merge)
Lane D: input-world-adapters (may start without owner data)
```

After owner data arrives:

```text
owner-dump-import
  -> source-action-table
      -> animation-pipeline

Meanwhile, dump-independent bridge work may continue, but the CrossOver slice
waits for both the source/animation chain and the bridge chain.
```

Do not parallelize two tasks that both change `ErInput`, `ErWorld`, `ErOutput`,
the generated C header, save schema, or consequence ABI. Land the schema owner
first, regenerate dependents, then prepare the next worktree from the new base.

## Owner dump handoff

On the Windows owner machine:

```powershell
py -3 tools\owner_data.py dump
```

Copy the produced private bundle to the Mac without adding it to the repo. On
the Mac:

```bash
python3 tools/owner_data.py import-bundle /path/to/private-owner-bundle.zip
python3 tools/verify.py --profile source
```

Before any worker receives a source task, the orchestrator records:

- Elden Ring build/version;
- bundle SHA-256 and importer report;
- installed `extracted.rs` SHA-256;
- proof that all generated outputs are ignored and untracked;
- exact base commit used to prepare source-dependent worktrees.

If the importer or source profile fails, stop at `owner-dump-import`. Do not
hand-edit generated Rust or relabel estimates as extracted facts.

## Swarm orchestration commands

These are documented for the later execution turn. Do not prepare tasks until
the user explicitly starts the swarm.

```bash
# Inspect only
python3 tools/swarm_sandbox.py doctor
python3 tools/swarm_sandbox.py list
python3 tools/swarm_sandbox.py prompt ir-schema-and-fixtures

# Later, after authorization and dependency review
python3 tools/swarm_sandbox.py prepare \
  ir-schema-and-fixtures \
  /tmp/skyring-swarm-ir

# After the worker finishes
python3 tools/swarm_sandbox.py check \
  ir-schema-and-fixtures \
  /tmp/skyring-swarm-ir
```

The orchestrator must review behavior and ownership, not merely a green check.
After merging a task, rerun the repository local profile before preparing a
dependent task.

## Definition of done at each claim ceiling

```text
local    = host tests, synthetic data, deterministic logic, schema/ABI checks
windows  = local + actual Windows DLL compilation; no Skyrim behavior claim
source   = local + fingerprinted owner-derived data validation
runtime  = behavior observed in Skyrim on the stated Windows runtime
release  = source + exact DLL/log/commit evidence under CrossOver on Apple Silicon
```

No task may promote its own ceiling. Only the declared dependent runtime or
release task can turn implementation into runtime/release evidence.

## Zero-to-hero finish line

The combat platform can ship when `combat-platform-release` passes. That means
the melee, consequence, posture, bow, gear, catalyst, magic, enemy, breadth,
Torrent, encounter, save, compatibility, and dual-perspective contracts all
have exact-commit CrossOver evidence.

The larger "everything contemplated" finish line is
`zero-to-hero-release`, which follows `megafauna-last`. Keeping these as two
separate releases enforces the Charter: creature rigging can consume time, but
it cannot hold combat hostage.
