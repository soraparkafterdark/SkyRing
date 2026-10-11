# Elden Ring combat mods for Skyrim — register and comparison

## The stack

There is no single "Elden Ring combat mod" for Skyrim and there never will be, because Skyrim's combat is not one system. It is six or seven separable ones, and the scene has evolved a distinct mod — usually several competing ones — for each. Building a cursed patched Skyrim means picking exactly one from each layer, because within a layer they are almost always mutually exclusive.

This is the shape of it, bottom to top:

| Layer | What it governs | Who implements it | Pick one? |
| --- | --- | --- | --- |
| Behavior engine | Patches Havok behaviour graphs so any of this can exist at all | Nemesis, or Pandora (newer, faster) | Yes |
| Animation replacer | Decides which clip plays for which condition | Open Animation Replacer (OAR); DAR is legacy | Yes |
| Attack behavior framework | Attack commitment, chains, cancel windows, the thing that makes Skyrim stop feeling like Skyrim | SkySA → Attack MCO-DXP → BFCO | Yes |
| Hit detection | Whether a swing connects | Vanilla cone, or Precision | Effectively yes |
| Poise / stagger | Whether a hit interrupts | POISE SKSE, Chocolate Poise, Valhalla stun, Elden Rim toughness, Rim Combat posture | Yes — four-way conflict |
| Block / parry | Timed defence | Elden Parry, Valhalla Combat, Parry for All, Rim Combat | Yes |
| Guard counter | The riposte after a block | Elden Counter | Only one exists |
| Dodge | Rolls, steps, i-frames | TK Dodge RE, DMCO, The Ultimate Dodge Mod, Dodge Framework | Yes |
| Weapon arts | Ashes of War | Elden Rim, Ashes of War via Additional Attack, Rim Combat's art system | Yes |
| Grip / stance | One-hand ↔ two-hand, moveset switching | Dynamic Grip, Stances | Conflicting |
| Movesets (content) | The actual animations | Dozens of ER moveset packs | No — stack freely |
| Camera / UI | Lock-on, boss bars, HUD | True Directional Movement, TrueHUD | No |
| NPC parity | Whether enemies use the same movesets | SCAR (for MCO), native in BFCO | Depends on framework |

The split that matters for your project: everything from "behavior engine" through "grip" is **framework** — systems code, mostly SKSE plugins, mostly solving the same four or five problems in incompatible ways. Everything in "movesets" is **content** — hand-authored animation, and this is where almost all the Elden Ring-branded mods actually live.

That asymmetry is the single most important fact in this document. The Skyrim scene has roughly a dozen serious framework mods and several hundred Elden Ring moveset packs. Nobody has built a data pipeline; everybody has animated by eye.

## Register

Every mod below either carries Elden Ring branding or is load-bearing for an Elden Ring setup. Nexus IDs are given so you can track versions; "active" means it has shipped an update within the last year.

### Explicitly Elden Ring

| Mod | Author | Nexus ID | What it implements | Status |
| --- | --- | --- | --- | --- |
| [Elden Rim - Weapon Arts](https://www.nexusmods.com/skyrimspecialedition/mods/65625) | black364 | 65625 | Ashes of War, toughness/stagger system, combo key input, magic frame, skill talent progression. Modular: base, weapon arts, two War Ash packs, special arts, NPC arts, MCO scripts | Active — v3.7.5, Oct 2026 |
| [Elden Parry](https://www.nexusmods.com/skyrimspecialedition/mods/70240) | dTry | 70240 | Rebinds light bash to a timing-window parry; power bash untouched | Stable |
| [Elden Counter](https://www.nexusmods.com/skyrimspecialedition/mods/65579) | dTry | 65579 | Guard counter after a successful block, with configurable i-frames. Script-free via Payload Interpreter | Stable |
| [Rim Combat](https://www.nexusmods.com/skyrimspecialedition/mods/180357) | Acook1e | 180357 | Seven-module overhaul: posture bar on TrueHUD, Elden Rim-style weapon arts with a Prisma UI menu, full stamina takeover, per-type values across 20+ weapon types and 50+ creature races via OCF | Active — 2026, open source |
| [Elden Rim Moveset Collection (SCAR)](https://www.nexusmods.com/skyrimspecialedition/mods/72220) | black364 | 72220 | 130+ ER animations across every weapon class, with distinct dual-wield sets | Stable, 9k+ endorsements |
| [BFCO I MCO Elden Ring — Maces (SCAR)](https://www.nexusmods.com/skyrimspecialedition/mods/183082) | — | 183082 | Per-class ER moveset with Precision collision annotations, NPC support, optional Dynamic Grip two-handing | Active — 2026 |
| ER Colossal Swords (MCO) | black364 | — | Colossal sword moveset for all 2H weapons | Stale, 2022 |
| Elden Rim Dodge | black364 | 76872 | ER/DS roll animations for TK Dodge and DMCO | Active |
| Elden Ring Step Dodge | Skypia | — | Quickstep-style dodge replacer for TK Dodge | Stale |
| Elden Sprint | Naxmaardur | — | No stamina drain outside combat, as in ER | Stable |
| Ashes of War Weapon Art via Additional Attack | — | 100174 | 68+ special attacks bound to weapons and Ash items; alternative to Elden Rim's system | Stale |
| Elden Ring Weapon Sound Effect | — | — | Swing and impact SFX replacer | Cosmetic |

ER moveset packs for individual weapon classes run into the hundreds and are not worth enumerating — they are interchangeable content on top of whichever framework you pick. Worth noting that Nightreign movesets have started appearing (a Duchess dagger set turned up in the Tuxborn list this year), so the content side is still growing four years on.

### Load-bearing frameworks

| Mod | Author | Role | Notes |
| --- | --- | --- | --- |
| Attack MCO-DXP | Distar | Attack behavior framework | The long-standing default; Maxsu's fork is what most modlists actually ship |
| BFCO - Attack Behavior Framework | — | Attack behavior framework | Newer MCO alternative; NPC moveset support works without SCAR |
| SCAR - Skyrim Combos AI Revolution | — | NPC parity | Lets enemies use player movesets under MCO |
| [Precision](https://www.nexusmods.com/skyrimspecialedition/mods/72347) | Ershin | Hit detection | True Havok collisions, weapon recoil off geometry, per-animation custom collision authoring, pre-hit callback API |
| [Valhalla Combat](https://www.nexusmods.com/skyrimspecialedition/mods/64741) | dTry | Stamina, block, stun | Script-free SKSE; stamina on attacks, timed block, projectile parry, stun meter and executions |
| [POISE - Stagger Overhaul SKSE](https://www.nexusmods.com/skyrimspecialedition/mods/72653) | Loki | Poise | Strips vanilla RNG stagger entirely and replaces the behaviour |
| Chocolate Poise | — | Poise | The fork most modlists use instead |
| [Parry for All](https://www.nexusmods.com/skyrimspecialedition/mods/167873) | — | Parry | 2026 entrant; weapon-to-weapon stagger, perk-driven NPC parry rules, damage reflection |
| True Directional Movement | Ershin | Camera | Lock-on and 360 movement — the thing that makes any of this readable |
| TrueHUD | Ershin | UI | Boss bars and the special meter that poise/stun systems render into |
| Dynamic Grip | — | Grip | One-hand ↔ two-hand swapping with separate movesets |
| Stances - Dynamic Weapon Movesets | — | Stance | Competing approach; conflicts with Dynamic Grip |
| Payload Interpreter · Animation Motion Revolution · Behavior Data Injector · dTry's Key Utils · IFrame Generator RE · Open Animation Replacer | various | Plumbing | Hard requirements scattered across the above |

### Prebuilt modlists worth reading as reference implementations

- **LoreRim** (Biggie Boss) — Maxsu's MCO fork, Chocolate Poise, Stances, Precision, SCAR 2, Combat Pathing Revolution, plus Requiem. The most deliberate souls-like integration on Wabbajack, and it ships a Dynamic Grip patch.
- **Tuxborn** (Omni-guides) — actively tracks BFCO and ER moveset churn; its changelog is the best public record of which ER mods are current.
- **Nolvus** — broader list, lighter on souls mechanics, but its mod index is a useful cross-check.

## Compare and contrast

The interesting part is not the mod list, it is that four or five teams independently solved the same problems with incompatible architectures. Each layer below is a genuine fork in the road.

### Attack commitment: SkySA → MCO → BFCO

All three do the same job — stop Skyrim letting you cancel a swing into a sprint — by rewriting the attack behaviour graph. The lineage matters because moveset packs are authored against a specific one.

- **SkySA** came first and is effectively dead; anything requiring it is legacy.
- **MCO-DXP** became the standard and has the deepest content library, but NPC parity needs SCAR bolted on, and the community now runs Maxsu's fork rather than Distar's original. Version sensitivity is real: Elden Rim wants MCO 1.5.x or later and misbehaves otherwise.
- **BFCO** is the newer design and handles NPC movesets natively, which removes a whole class of SCAR patching. The 2026 moveset packs increasingly ship "BFCO I MCO" dual support, which tells you the scene is mid-migration rather than settled.

For a from-scratch build in 2026, BFCO is the better foundation; for maximum existing content, MCO. You cannot run both.

### Hit detection: cone versus capsule

This is the cleanest comparison in the whole scene because there is only one real answer. Vanilla Skyrim resolves a melee hit with an angle-and-distance check from the attacker — the weapon mesh is decoration. **Precision** replaces that with actual Havok collision along the weapon, which means reach becomes a property of the weapon model, you bounce off walls, and attacks can genuinely whiff past someone standing to the side.

Precision is also the only mod in this document that exposes the thing you care about: per-animation custom collision authoring, plus a pre-hit callback that other plugins hook to cancel or modify a hit. Elden Parry works by registering against that callback. **Precision is the attachment point for anything data-driven you want to build.**

The catch is that it breaks mods assuming the old model — Elden Parry needs a compatibility toml because light bash collision has to be stripped while power bash is preserved.

### Poise: four systems, one job, no interoperability

This is the ugliest conflict in the stack.

| System | Model | Where it renders | Conflicts |
| --- | --- | --- | --- |
| POISE - Stagger Overhaul (Loki) | Removes humanoid stagger behaviour outright and substitutes its own; poise damage per weapon | — | Anything using vanilla stagger does nothing |
| Chocolate Poise | Fork of the above, better balanced; what modlists actually ship | — | Same |
| Valhalla Combat stun | AC/God of War model — a stun meter that does not regenerate in combat, ending in an execution | TrueHUD special meter | Competes with the above for the meter |
| Elden Rim toughness | ER-style, with distinct knockdown/throw stagger animations and an NPC patch for non-humanoids | — | Needs tuning against the others |
| Rim Combat posture | Closest to ER's actual posture: per-weapon-type and per-creature values via OCF | TrueHUD special bar | Hard-incompatible with Valhalla Combat and Elden Parry |

The fault line: **Valhalla's stun is a resource you deplete to earn an execution; ER's poise is a threshold you break to earn a stagger.** They look similar on a meter and behave completely differently. Rim Combat exists specifically because its author thought Valhalla got this wrong — it is a declared refactor of dTry's work, and it takes over the stamina system wholesale to do it.

If ER fidelity is the goal, Rim Combat is the closest on paper and the most disruptive to install. It knocks out Valhalla Combat, Elden Parry and Shield of Stamina in one go, and wants Elden Rim tuned down.

### Block, parry and counter: three mods that each own a third of the loop

Elden Ring's defensive loop is block → guard counter, or parry → riposte. Skyrim's scene has split this across authors who mostly did not coordinate, though dTry wrote three of the pieces:

- **Elden Parry** owns the parry window by repurposing light bash. Elegant, narrow, compatible with nearly everything.
- **Elden Counter** owns the guard counter. It is the only implementation and it is the most faithful single mod in the entire scene — correct trigger window, configurable i-frames, no script lag.
- **Valhalla Combat** owns timed blocking and projectile parry, and overlaps Elden Parry enough that you tune one against the other.
- **Parry for All** (2026) is the first attempt to unify this: weapon-to-weapon stagger, perk-driven rules so different NPCs parry differently, damage reflection scaled by block skill. It is built on newer plumbing (State Behavior Framework, Dynamic Forms Generator) and is the one to watch if you want parry to be a system rather than a feature.

### Weapon arts: the hardest thing, done twice, both times by hand

Elden Rim is the only mature Ashes of War implementation and it is genuinely ambitious — a talent system, a scroll economy for binding arts to Ash items, a magic frame, NPC arts. It also ships a companion mod whose entire purpose is displaying which arts you have learned, which tells you how much bookkeeping it carries.

Rim Combat's art system with the Prisma UI menu is the modern competitor, and "Ashes of War via Additional Attack" is the lightweight third option. None of them derive anything from Elden Ring's own data. Every art is an animator's interpretation of what the move looked like.

### Where everyone converges

True Directional Movement, TrueHUD, Precision and Open Animation Replacer appear in essentially every souls-like Skyrim build. There is no competing lock-on camera worth naming. These four are settled infrastructure — treat them as given and spend your effort above them.

## The gap

Four years of Elden Ring mods for Skyrim and not one of them reads Elden Ring's files.

Every moveset in the register above was authored by watching footage and animating to match. Every poise value was picked by feel. Every i-frame count was tuned until it felt right. The mods are impressive and the fidelity ceiling is set by eyeballing, which is precisely the ceiling the Bevy sandbox was built to break through — its whole premise is that timings, root motion, stamina costs, hit windows and cancel windows come out of the game's own data rather than a animator's judgement.

That is the cursed patched Skyrim that does not exist yet: not another moveset pack, but the extracted numbers driving the existing frameworks.

**What is already extractable and nobody is using:**

| Extracted datum | Skyrim target | Why it is currently guessed |
| --- | --- | --- |
| Action length, hit windows, cancel windows | OAR/BFCO annotation timings | Animators match footage frame counts by eye |
| I-frame spans per roll weight | IFrame Generator RE / dodge mod config | Set to whatever felt fair |
| Stamina cost per hit, per roll, per backstep | Valhalla or Rim Combat stamina module | Balanced by playtesting |
| Motion values and weapon base attack | Damage formulas | Skyrim uses its own scaling entirely |
| Hit-stop duration per attack | zxlice hitStop or equivalent | One global value for everything |
| Root motion per action | Animation Motion Revolution | Baked into the hand-made clip |
| Poise/stance damage per attack | POISE / Chocolate Poise / Rim Combat | Per-weapon-type guesses |
| Weapon dimensions | Precision collision annotations | Authored against the Skyrim mesh |

The last row is the bridge to the other tab in this doc. Precision supports per-animation custom collision capsules authored by the animator — which is exactly the shape of the geometry tables I built for the legendary armaments. Real reach numbers going into Precision annotations is a concrete, small, nobody-has-done-it piece of work.

**Four things nobody has built, in rough order of effort:**

1. **An extracted-timing annotation generator.** Take the repo's `extract.py` output and emit OAR/BFCO annotation files. The sandbox already produces frame-accurate action tables; Skyrim already consumes annotation files. Nothing in between exists.
2. **ER-accurate Precision collision sets.** Per weapon class, from real model dimensions rather than Skyrim's meshes. Smallest standalone win here.
3. **A poise system fed by real stance-damage numbers.** Every existing poise mod has the architecture and invented values. Rim Combat's per-weapon-type OCF structure is the natural host.
4. **The legendary armaments as actual weapons** — real stats, real scaling behaviour, real unique skills — rather than retextured Skyrim weapons with an Elden Rim art bound to them. This needs the scaling work the sandbox also lacks, so it comes last.

**The honest caveat.** None of this is a mod you install; it is tooling that makes other people's mods accurate. That is a less visible deliverable than a moveset pack, and it is also the only part of this space that is not already crowded.

## Sources and tracking

Mod pages opened for this register:

- [Elden Rim - Weapon Arts](https://www.nexusmods.com/skyrimspecialedition/mods/65625) · [Elden Parry](https://www.nexusmods.com/skyrimspecialedition/mods/70240) · [Elden Counter](https://www.nexusmods.com/skyrimspecialedition/mods/65579)
- [Rim Combat](https://www.nexusmods.com/skyrimspecialedition/mods/180357) — source at [Acook1e/RimCombat](https://github.com/Acook1e/RimCombat)
- [Elden Rim Moveset Collection (SCAR)](https://www.nexusmods.com/skyrimspecialedition/mods/72220) · [BFCO I MCO Elden Ring — Maces](https://www.nexusmods.com/skyrimspecialedition/mods/183082) · [Elden Rim Dodge](https://www.nexusmods.com/skyrimspecialedition/mods/76872)
- [Valhalla Combat](https://www.nexusmods.com/skyrimspecialedition/mods/64741) · [Precision](https://www.nexusmods.com/skyrimspecialedition/mods/72347) · [POISE - Stagger Overhaul SKSE](https://www.nexusmods.com/skyrimspecialedition/mods/72653) · [Parry for All](https://www.nexusmods.com/skyrimspecialedition/mods/167873)
- [Dynamic Dodge Animation](https://www.nexusmods.com/skyrimspecialedition/mods/79598) · [Elden Parry - Precision Compatibility](https://www.nexusmods.com/skyrimspecialedition/mods/115144)
- [LoreRim](https://www.lorerim.com/) · [Tuxborn changelog](https://github.com/Omni-guides/Tuxborn/blob/main/Changelog.md) · [Nolvus mod index](https://www.nolvus.net/)

### How to keep this current

This scene moves fast enough that a snapshot rots in months — BFCO displaced MCO as the default inside about two years, and Rim Combat arrived in 2026 explicitly to replace work from 2022. Three places tell you what changed without trawling Nexus:

1. **The Tuxborn changelog** on GitHub. It is a dated add/remove log of a maintained list, so it shows adoption and abandonment rather than just releases. BFCO being added and later removed in that file is the kind of signal no mod page gives you.
2. **Nexus "recently updated" filtered to the Combat and Animation categories.** Version bumps on Elden Rim and Rim Combat are the two that matter.
3. **Rim Combat's GitHub.** It is the only major mod in this register that is open source, which makes it the one you can actually read rather than reverse-engineer.

Unverified and worth checking before you rely on them: the exact current default framework in LoreRim (it shipped MCO, may have moved to BFCO), and whether Elden Rim's October 2026 release changed its MCO version requirements.
