# Integrating the combat systems — a reasoning plan

The question is not how to move Elden Ring's combat into Skyrim. It is which quantities mean the same thing in both games, which mean different things, and which have no counterpart at all. Nearly every decision below falls out of that.

## The invariant

Before deciding what to port, decide what Elden Ring combat *is*. Get this wrong and you will spend a year porting numbers that do not matter.

It is not the damage values. It is not the stat scaling. It is a **reading loop**: the enemy telegraphs, you commit to a response, and the window you get is determined by what both of you committed to. Everything that makes the game feel like itself lives in the *relative timing* between actions — how long a swing takes versus how long a roll takes versus how long the recovery is.

That gives a sharp test for every candidate quantity. Ask: **if I got this wrong by 20%, would a player notice it as "this doesn't feel like Elden Ring," or just as "this is tuned differently"?**

- Roll i-frames 20% short → immediately, viscerally wrong. The loop breaks.
- Greatsword recovery 20% long → immediately wrong. Punish windows stop lining up.
- Greatsword damage 20% low → nobody notices. It is a difficulty slider.
- Scaling curve 20% off → nobody notices. It is a progression knob.

So the invariant is **timing and geometry, preserved in relative terms**. Everything else is tuning, and tuning should be done against Skyrim's own economy rather than imported.

This has an immediate consequence that is worth stating plainly, because it cuts the scope roughly in half: **do not port the damage model.** Not motion values, not attack rating, not the defence formula, not scaling. Skyrim already has a damage model, several mods already rebalance it, and importing Elden Ring's would require importing its HP curve, its armour model and its stat system to be coherent — at which point you have not integrated two games, you have replaced one with the other and kept the textures.

The second consequence is subtler. Elden Ring has **two** interruption systems and almost every Skyrim mod collapses them into one:

- **Poise** is defensive hyperarmour. It decides whether *you* get interrupted while swinging.
- **Stance** (posture) is an offensive meter on the target. It decides whether *they* break open for a critical.

These are separate numbers, separately tracked, and they create opposite incentives — poise lets you trade, stance rewards aggression. Every poise mod in the register models one of them and calls it poise. Keeping them separate is probably the single highest-value conceptual thing this project can contribute, and it costs nothing at extraction time because the data distinguishes them.

## The conversion ledger

This is the document the whole project hangs on. Every quantity the extractor produces gets one of three verdicts, and the verdict determines who owns the number.

- **Transfers** — same physical meaning at both ends, convert with a scalar or not at all.
- **Transfers as ratio** — the absolute value is meaningless across games, the proportion is not.
- **Refuse** — no counterpart; Skyrim's own system owns it and the port must not touch it.

### The two scalars you need first

**Time** needs no conversion. Elden Ring's action data is in seconds and Skyrim's behaviour annotations are in seconds. The sandbox runs a 60 Hz state machine over it; Skyrim plays animations at whatever rate the clip was authored at. Windows expressed as absolute times transfer directly. This is the single luckiest fact in the whole project.

**Space** needs one scalar, and it is clean. A Bethesda unit is exactly 9/16 of an inch — 1.42875 cm — which is why a 128-unit Nord is exactly six feet. So:

```latex
1\ \text{metre} = \frac{100}{1.42875} \approx 69.99\ \text{Skyrim units}
```

Call it **70 units per metre** and you are within rounding. Character height differs slightly — Skyrim's humanoid is 128 units (1.83 m), Elden Ring's player is roughly 1.75 m — about a 4.5% mismatch. Decide early whether reach scales by absolute metres or by body height; absolute is right for weapons, body-relative is right for root motion, and conflating them will make rolls overshoot.

### The ledger

| Quantity | Elden Ring unit | Skyrim unit | Verdict | Who owns it |
| --- | --- | --- | --- | --- |
| Action length | seconds | annotation time | Transfers | Extractor |
| Hit window open/close | seconds | annotation time | Transfers | Extractor |
| Cancel window | seconds | framework-specific | Transfers | Extractor |
| Input window | seconds | buffer config | Transfers | Extractor |
| I-frame span | seconds | IFrame Generator / dodge config | Transfers | Extractor |
| Hit-stop duration | seconds, per attack | one global value today | Transfers | Extractor |
| Weapon length and striking segment | metres | units (×70) | Transfers | Extractor |
| Capsule radius | metres | units (×70) | Transfers | Extractor |
| Root motion | metres, per action | units (×70) | Transfers | Extractor |
| Walk / run / sprint speed | m/s | units/s (×70) | Transfers | Extractor |
| Stamina cost per action | absolute, pool ≈ 80–170 | absolute, pool scales with level | Ratio | Shared |
| Stamina regen and delay | absolute rate | absolute rate | Ratio | Shared |
| Poise (hyperarmour threshold) | absolute | no vanilla counterpart | Ratio | Shared |
| Stance damage per attack | absolute | no vanilla counterpart | Ratio | Shared |
| Equip-load roll tier | % of max load | % of carry weight | Ratio | Shared |
| Motion values | multiplier on AR | — | **Refuse** | Skyrim |
| Attack rating and scaling curves | ER stat curves | skill + perks | **Refuse** | Skyrim |
| Defence and damage negation | per-type absorption | armour rating | **Refuse** | Skyrim |
| HP | 300–2000+ | \~100 base | **Refuse** | Skyrim |
| FP and spell costs | separate pool | magicka | **Refuse** | Skyrim |
| Status buildup thresholds | absolute | no counterpart | **Refuse** | Skyrim |

### Why the ratio column is the hard one

The transfers are mechanical and the refusals are free. The middle column is where judgement lives, because a ratio needs a denominator and the denominator is a design decision.

Stamina is the clear case. A light roll costs a fixed amount in Elden Ring, and because the stamina pool barely grows over a playthrough, that cost stays meaningful from start to finish. Skyrim's stamina pool grows with every level you spend on it, so the same proportional cost means something completely different at level 5 and level 50. Port the ratio naively and rolling becomes free by mid-game, which quietly deletes the reading loop.

The fix is to pick the denominator deliberately: express costs as a fraction of a *fixed reference pool* rather than the player's current maximum, and let stamina investment buy regeneration rather than roll count. That is a design choice, not a conversion, and it is exactly the sort of decision that should be written down here rather than discovered in tuning.

## Architecture: one IR, two backends

The open question across this whole project has been whether to target Skyrim or the Bevy sandbox. The answer is that it should not be a question, because the expensive work is identical for both and only the last step differs.

```
  Elden Ring files
         │
    [extractor]            ← exists today (tools/extract.py)
         │
  combat IR (versioned, game-agnostic, seconds + metres)
         │
    ┌────┴────┐
[bevy emitter]  [skyrim emitter]
    │                │
extracted.rs   OAR/BFCO annotations,
               Precision collisions,
               plugin config
```

The repo already has the left half. It reads the game's files and writes `src/sim/extracted.rs` — a generated action table. The whole proposal is: **stop emitting Rust directly, emit a neutral intermediate representation, and make the Rust table one backend among two.**

### What the IR has to be

Four properties, and they are all forced by the ledger:

1. **Units are explicit and canonical.** Seconds and metres, never frames or game units. Backends do their own scaling. A frame count in the IR is a bug, because it bakes in a tick rate that neither target shares.
2. **Every value is tagged with its provenance.** The repo already distinguishes extracted data from estimates and marks the latter `ESTIMATE` in `data.rs`. That distinction must survive into the IR, because a Skyrim backend consuming a guess should be able to say so. An IR where you cannot tell a measured i-frame window from an invented one is worthless for the thing that makes this project novel.
3. **Verdicts are encoded, not implied.** Each field carries whether it transfers, transfers-as-ratio, or is refused. The Skyrim emitter then physically cannot emit a damage number, because the schema does not carry one.
4. **It is versioned against a game patch.** Elden Ring rebalances; the register tab already shows wikis disagreeing about spell costs across patches. An IR that does not record which game version produced it will silently mix data from two balance states.

### Why this is worth the extra layer

The tempting shortcut is to write a Skyrim exporter directly against the existing extractor and skip the IR. It is faster for about two months and then it is worse forever, for three reasons.

The backends disagree about almost everything *except* the numbers. Bevy wants a flat action table it can match on at 60 Hz; Skyrim wants annotation files keyed to specific animation clips, plus TOML for Precision, plus per-plugin config. Those are different enough that sharing an emitter is not possible, and different enough that a shared IR is clearly where the commonality actually sits.

The IR is also the only place a **differential test** can live. If both backends derive from one source, you can assert that a Skyrim annotation and a Bevy action describe the same window — which is the validation strategy below, and it does not exist without this layer.

And it is the artifact other people can use. A Rust table is useful to one project; a documented IR of Elden Ring's combat timings is useful to every animator in the register tab who is currently counting frames off YouTube. If any part of this gets adopted by the broader scene, it is this.

## Order of integration

The ordering is not a preference, it is forced: each subsystem's tuning is only meaningful once the one below it is real. Tune poise before hit detection is accurate and you are tuning against phantom hits, then retuning from scratch.

1. **Geometry and hit detection.** Precision collision capsules from real weapon dimensions. This is first because every subsequent measurement depends on hits landing where they should, and because it is the only step that is immediately useful to other people as a standalone mod. Blocked by nothing. This is where to start on Monday.
2. **Action timing.** Emit annotation timings — length, hit windows, cancel windows — for one weapon class. Blocked by step 1, because you cannot verify a hit window if the hit itself resolves off a cone.
3. **Root motion and locomotion speeds.** Scaled by the 70 units/metre constant, fed through Animation Motion Revolution. Blocked by step 2 only loosely; can run in parallel if you are careful about the body-height question.
4. **I-frames and dodge.** Roll windows per equip-load tier. Blocked by 2 and 3, because an i-frame span only means something relative to the roll's distance and duration.
5. **Hit-stop.** Per-attack rather than global. Small, high-impact, needs the Precision callback. Blocked by 1.
6. **Poise.** Defensive hyperarmour thresholds. Blocked by everything above, because "did that hit interrupt me" requires the hit, its timing, and the animation state to all be correct first.
7. **Stance.** Offensive break meter, kept separate from poise. Blocked by 6 — build them in order so the separation is enforced by construction rather than by discipline.
8. **Stamina.** Last, because it is the most design-dependent and the ratio denominator question has to be settled by play rather than extraction.

### The first vertical slice

Do not do step 1 across all weapons, then step 2 across all weapons. Do steps 1 through 5 for **one weapon class**, end to end, and get it shipping-quality before widening.

Pick the colossal sword. Reasons: the timings are long, so errors are visible to the naked eye rather than requiring frame analysis; the reach is extreme, so collision mistakes are obvious; the class already has mature Skyrim movesets to compare against; and two of your legendary armaments live there, which connects this work to the item tab.

The slice is done when a colossal sword swing in Skyrim and the same swing in the Bevy sandbox agree on every window to within one frame, and you did not hand-tune anything to get there.

## Validation

Every mod in the register tab validates by feel. That is the whole reason this project has a reason to exist, so validating by feel would be self-defeating. There are three levels available, and the middle one is the interesting one.

### Level 1 — assert the conversion

Unit tests over the IR. The Skyrim emitter's output, converted back to seconds and metres, equals the IR. Catches scalar errors, off-by-one frame conversions, the body-height-versus-absolute-metres confusion. Cheap, mechanical, and it will catch more real bugs than it has any right to.

### Level 2 — differential testing against the sandbox

This is the one nobody else can do, and it falls out of something the repo already built for an unrelated reason.

The sandbox has a **sparring dummy** and a **scripted demo that plays itself through every action using the same inputs a player has**, explicitly built so nothing in it is staged. That is not a demo reel. That is a test harness.

So: rebuild the dummy and the input script in Skyrim. Same arena geometry, same dummy behaviour, same input sequence, same frame. Then diff the two runs on the quantities that are supposed to match — time from input to first hit frame, time to recovery, distance travelled by a roll, whether a given attack connects at a given range.

What makes this strong is that the sandbox is a known-good reference *derived from the same source data*. A disagreement is not ambiguous. It means either the Skyrim backend mistranslated, or Skyrim's animation system cannot express what the IR describes — and those two are distinguishable by looking at where the error is. A single number off means a conversion bug; a consistent drift across a whole animation means blending.

Build the Skyrim-side harness early, during the vertical slice, not after. Retrofitting it is the kind of job that never gets done.

### Level 3 — the eye

Frame-by-frame video capture of the same action in Elden Ring, the sandbox, and Skyrim, side by side. Slow and manual, so reserve it for the things levels 1 and 2 cannot see: whether the animation *reads* the same, whether the telegraph is legible, whether blending has eaten the anticipation frames.

This is also the only level that catches the failure where every number is right and it still feels wrong.

### The one metric worth tracking over time

Count of values in the IR tagged extracted, versus tagged estimate. The repo currently has a meaningful estimate list — sprint hold time, stamina regen, gravity after a jump arc, acceleration and turn rates, and critically the hit shapes. Every one you convert from estimate to extracted is the project's actual output. If that number is not moving, the project is producing animations rather than data, and the scene already has plenty of animations.

## Failure modes

### The NPC problem, which is the real one

Everything above ports the *player*. The reading loop needs two participants, and the enemy half does not transfer at all.

Elden Ring enemies are authored around the player's timing: attack selection, spacing, the deliberate delays that bait early rolls. Skyrim's combat AI picks attacks on a loop that was designed for a game where you could cancel out of anything. SCAR and BFCO let NPCs *play* the new movesets; they do not give NPCs reasons to choose between them.

So the predictable outcome is a player who moves exactly like Elden Ring fighting enemies who still behave like Skyrim — and because the loop is a conversation, half of it being right may well feel worse than neither half. A perfectly timed roll against an enemy with no telegraph is not satisfying, it is arbitrary.

There is no extraction answer to this, because the sandbox explicitly does not extract enemy behaviour — its dummy's attacks and timings are invented for testing. Three honest options:

- **Accept it.** Port the player, use Combat Pathing Revolution and the best available AI mods, accept that enemies approximate.
- **Hand-author a small set of enemies properly.** Five Skyrim enemy types with deliberately designed telegraphs and punish windows will carry a vertical slice further than fifty approximated ones.
- **Extend the extractor to enemy actions.** Correct, large, and a separate project from everything above.

Decide which before step 6, because poise tuning is meaningless against an enemy whose attack rhythm is arbitrary.

### The others, ranked by how likely they are to bite

**Animation blending eats your windows.** Skyrim blends between clips; the sandbox says its own blending is simpler than the game's. A window that is correct in the IR can land in the wrong place after Skyrim blends into it. Expect a systematic drift rather than random error, which means it is correctable once measured — but only if level 2 validation exists to measure it.

**The behaviour graph cannot express some transitions.** Elden Ring allows cancels at points Skyrim's graph has no state for. You will hit cases where the IR describes something the backend simply cannot emit. The right response is to record the gap in the IR rather than approximate silently, so the failure is visible later.

**The sandbox's own estimates propagate.** Hit shapes, stamina regen, acceleration and turn rates are all guesses today. Porting a guess into Skyrim and validating it against the sandbox will show perfect agreement, because both are wrong in the same way. This is the one failure mode level 2 testing actively hides, which is why the extracted-versus-estimate count matters as a tracked metric.

**Bevy's release cadence.** Pre-1.0, breaking every release. The IR and the extractor should not depend on Bevy at all — keep the engine confined to the rendering and input layer so a Bevy migration never touches the data work.

**Elden Ring patches move the ground.** Version the IR against a game build or you will eventually mix two balance states and spend a week finding it.

### When to stop

Two honest kill criteria, worth writing down now while it is cheap to be objective:

If after the colossal-sword slice the Skyrim side needs hand-tuning to feel right, the thesis is wrong. The whole premise is that extracted data beats eyeballing; if extracted data needs eyeballing on top, you have built a slower path to where the scene already is.

And if you find yourself porting the damage model, stop and reread the invariant. That is the direction that turns this into a five-year total conversion, and it is the most likely way for the project to quietly become unfinishable.
