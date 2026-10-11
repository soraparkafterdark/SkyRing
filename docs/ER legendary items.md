# Elden Ring Legendary Combat Items — Data & Scale Reference

Oct 10, 2026 · @Devon

## End state, and what "Apple Silicon native" actually costs

The reference repo is not a game mod. It is a Rust/Bevy 0.19 sandbox that reimplements the Elden Ring player character — movement, dodging, attack chains, hit windows, i-frames, hit-stop, poise reactions, power stance — driven by timings extracted from the game's own files. Nothing from the game ships in it; Python tools in `tools/` regenerate the data from your copy.

That distinction decides everything about porting.

**The runtime is already portable.** Bevy sits on wgpu, which targets Metal natively. `cargo run` on an M2 Pro is an aarch64-apple-darwin build and should need no porting work beyond whatever the sandbox does with platform paths.

**The toolchain is the only blocker.** The README's Windows requirement comes from one thing: the extractors load the game's own Oodle decompression DLL to read `.dcx` containers. Three ways out, in increasing order of effort:

1. **Bake on Windows, run on Mac.** Generate `src/sim/extracted.rs`, `assets/player_anims.bin` and `assets/player_sounds.bin` on a Windows box, copy the three artifacts over, build natively. Zero code changes. This is the fast path and it gets you a native M2 Pro binary today.
2. **Replace the Oodle dependency.** Swap the DLL call for an open Kraken/Mermaid decoder compiled for arm64. This makes `python tools/setup.py` run on macOS and removes the Windows machine from the loop permanently.
3. **Port the whole extraction chain.** Only worth it if you are going to re-derive data often, or extend the extractors to cover what the sandbox currently leaves out.

**Skyrim is a different problem, not a continuation of this one.** Skyrim SE has no native Apple Silicon build and no Bevy sandbox to extend. If the goal is "Elden Ring combat in Skyrim," that is Creation Engine behaviour work (Behavior Graph, Papyrus/SKSE) running under CrossOver or Whisky — a translation layer, so "native Apple Silicon" is not available for it at all. If the goal is instead "a sandbox like this one, for Skyrim's data," that is a second extractor against `.bsa`/`.hkx` and reuses the Bevy side.

**A concrete end state, worth writing down before anything else:**

- A native aarch64-apple-darwin binary of the sandbox runs on the M2 Pro at 60 Hz with sound.
- `python tools/setup.py` runs on macOS without a Windows machine.
- The stand-in primitive weapons are replaced by correctly scaled proxies per weapon class, with hit capsules that match real reach rather than the sandbox's placeholder models.
- A named roster of legendary armaments, catalysts and spells exists as data, with stats and dimensions — which is what the rest of this document is.

That last item matters more than it looks. The README lists hit shapes under **still estimated**: the capsule along the striking part follows the sandbox's stand-in models, not the game's. Accurate per-weapon dimensions are the input that turns an estimate into extracted data.

## The nine Legendary Armaments

These are the nine that count toward the base-game achievement. All nine share three traits worth encoding as flags: unique skill (no Ash of War swap), no affinity change, and Somber Smithing Stone upgrade path (+0 to +10). None can be spell-buffed or greased.

### Stats

Attack power shown as base (+0) → max (+10). Scaling shown the same way where it changes.

| Weapon | Class | Dmg type | Attack +0 | Attack +10 | Requires | Scaling +0 → +10 | Wgt | Skill (FP) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Grafted Blade Greatsword | Colossal Sword | Standard | 162 phy | 396 phy | Str 40 / Dex 14 | C/E → B/D | 21.0 | Oath of Vengeance (20) |
| Ruins Greatsword | Colossal Sword | Standard | 124 phy / 37 mag | 303 phy / 90 mag | Str 50 / Int 16 | Str B, Int E → Str S, Int D | 23.0 | Wave of Destruction (25) |
| Dark Moon Greatsword | Greatsword | Standard / Pierce | 82 phy / 98 mag | 200 phy / 240 mag | Str 16 / Dex 11 / Int 38 | D/D/C → D/D/B | 10.0 | Moonlight Greatsword (32) |
| Golden Order Greatsword | Greatsword | Standard / Pierce | 86 phy / 103 holy | 210 phy / 252 holy | Str 16 / Dex 21 / Fai 28 | E/D/C → E/D/B | 10.0 | Establish Order (20, +20 follow-up) |
| Marais Executioner's Sword | Greatsword | Standard / Pierce | 94 phy / 61 mag | 230 phy / 149 mag | Str 24 / Dex 14 / Arc 23 | C/E/D → B/E/D | 11.5 | Eochaid's Dancing Blade (15) |
| Devourer's Scepter | Great Hammer | Strike | 112 phy / 69 fire | 274 phy / 169 fire | Str 24 / Dex 20 / Fai 25 | D/D/D → C/C/C | 11.5 | Devourer of Worlds (35) |
| Bolt of Gransax | Spear | Pierce | 98 phy / 63 lgt | 240 phy / 154 lgt | Str 20 / Dex 40 | D/D → D/C | 8.5 | Ancient Lightning Spear (25) |
| Sword of Night and Flame | Straight Sword | Standard / Pierce | 87 phy / 56 mag / 56 fire | 213 / 137 / 137 | Str 12 / Dex 12 / Int 24 / Fai 24 | E/E/D/D → D/D/B/B | 4.0 | Night-and-Flame Stance (26 beam / 32 sweep) |
| Eclipse Shotel | Curved Sword | Slash | 77 phy / 77 holy | 188 phy / 188 holy | Str 10 / Dex 25 / Fai 30 | E/D/D → D/C/C | 3.0 | Death Flare (16) |

All nine have a 100 critical modifier. Passives worth flagging: Dark Moon Greatsword carries 55 frostbite buildup; Eclipse Shotel applies death blight only while its skill buff is active, and only against players; Golden Order Greatsword does +20% to undead and blocks skeleton revival; Devourer of Worlds restores 15% max HP per enemy caught.

### Geometry and hit capsules

These are derived, not extracted — see the methodology section. Origin is the grip's base (pommel end). The striking segment is the span that should carry a hit capsule; radius is the capsule's thickness at its widest.

| Weapon | Overall length | Grip span | Striking segment | Capsule radius | Blade/head cross-section |
| --- | --- | --- | --- | --- | --- |
| Bolt of Gransax | 2.70 m | 0.55–0.95 m | 1.95–2.70 m | 0.07 m head, 0.035 m shaft | tapering splinter, barbed |
| Devourer's Scepter | 2.15 m | 0.00–1.55 m | 1.55–2.15 m | 0.19 m head, 0.04 m shaft | sphere ≈0.34 m across |
| Grafted Blade Greatsword | 2.05 m | 0.00–0.42 m | 0.45–2.05 m | 0.15 m | 0.34 m × 0.18 m at widest |
| Ruins Greatsword | 1.80 m | 0.00–0.40 m | 0.45–1.80 m | 0.18 m | 0.38 m wide × 0.14 m thick |
| Golden Order Greatsword | 1.62 m | 0.00–0.32 m | 0.40–1.62 m | 0.07 m | 0.13 m × 0.02 m |
| Marais Executioner's Sword | 1.60 m | 0.00–0.33 m | 0.40–1.60 m | 0.075 m | 0.145 m × 0.018 m |
| Dark Moon Greatsword | 1.55 m | 0.00–0.30 m | 0.38–1.55 m | 0.06 m | 0.11 m × 0.02 m |
| Sword of Night and Flame | 1.15 m | 0.00–0.22 m | 0.26–1.15 m | 0.035 m | 0.062 m × 0.012 m |
| Eclipse Shotel | 0.95 m chord | 0.00–0.20 m | 0.22–0.95 m | 0.035 m | 0.055 m deep × 0.01 m |

Two cases need special handling. The **Eclipse Shotel** is an arc, not a line: chord 0.95 m, arc length ≈1.10 m, sagitta ≈0.22 m. Fit three short capsules along the curve rather than one. The **Dark Moon Greatsword** changes length when its skill is active — add roughly 0.25 m of blade-shaped light and a projectile emitter at the tip; patch 1.07 gave that light its own damage detection, so the extended segment is a real hitbox, not decoration.

### Silhouettes

- **Grafted Blade Greatsword** — a conical bouquet of about twenty mismatched blades lashed around a core, tips fanning outward. Pitted iron, rust in the seams. Reads unmistakably as a pinecone.
- **Ruins Greatsword** — a squared wedge of pitted grey masonry with no edge and no point. Faint blue gravity-light in the cracks, brightest just before a charged heavy detonates.
- **Dark Moon Greatsword** — slim straight double-edged blade, pale blue-white, frost crusting along the fuller. Simple flared crossguard about 0.22 m across. Glows even unbuffed.
- **Golden Order Greatsword** — blade of translucent amber light banded with glyph rings, no metal. Crossguard is a broken ring arc about 0.30 m across, modelled on the Elden Ring itself.
- **Marais Executioner's Sword** — parallel-sided, square-ended, no point. Dark steel with a crimson-and-gold pommel cage and a hanging tassel ring.
- **Devourer's Scepter** — long haft topped by a serpent coiled around a sphere. One of the longest weapons in its class; the reach is the point.
- **Bolt of Gransax** — a splinter broken off a far larger dragon's spear, so it is deliberately asymmetric and jagged. Gold lightning crawls the length of it.
- **Sword of Night and Flame** — short, light, ornate. Gold scrollwork crossguard about 0.20 m wide. The blade reads cold blue along one half and ember orange along the other.
- **Eclipse Shotel** — bone-pale crescent, inward-curving, with a black eclipsed-sun disc set at the ricasso. Thinnest and lightest of the nine at 3.0 weight.

## Legendary-rarity armaments outside the nine

"Legendary" means two different things in Elden Ring and conflating them will give you a wrong roster. There is the **achievement set** — a hard-coded list of nine — and there is the **rarity tag**, which paints the pickup banner gold and plays the legendary acquisition sound. More weapons carry the tag than count toward the achievement, which is why players perennially argue that Godslayer's Greatsword and the Black Knife "should" be on the list: they get the gold banner, they just are not on the nine-item check.

Shadow of the Erdtree added no achievements at all, so nothing in it affects the nine. It did add weapons with the Legendary rarity tag:

| Weapon | Class | Notes |
| --- | --- | --- |
| [Euporia](https://eldenring.wiki.gg/wiki/Euporia) | Twinblade | Luster mechanic: hits bank points toward +95 holy AP at tier 1, +120 at tier 2; the skill consumes it. Str/Dex/Fai. |
| Ancient Meteoric Ore Greatsword | Colossal Sword | Str/Dex/Arc, physical + magic. Arcane raises both. Skill: White Light Charge. Comes with an Ancient Dragon Smithing Stone. |
| Sword of Light | Straight Sword | From the Stone-Sheathed Sword used at the Ruins of Unte altar in daytime. |
| Sword of Darkness | Straight Sword | The night counterpart from the same base weapon. |

The Stone-Sheathed Sword those last two come from is itself not legendary-tagged. Shadow of the Erdtree also added two legendary Spirit Ashes (Taylew the Golem Smith, Divine Bird Warrior Ornis) and legendary armour, which are outside the combat-item scope here.

If you are building a roster by rarity tag rather than by achievement, pull the flag out of `regulation.bin` directly rather than from any wiki list — the achievement pages and the rarity pages disagree with each other constantly, and the param is authoritative.

## Glintstone staves — complete roster

**There is no Legendary rarity tier for staves or seals.** No catalyst in the base game or Shadow of the Erdtree carries the tag, and no achievement covers them. So the useful answer to "every legendary staff or seal" is the complete roster, which is what follows — twenty staves, base game plus the two DLC additions.

Sorcery Scaling is the number that actually drives spell damage. Max values are at 80 Intelligence, maximum upgrade (+25, or +10 for Somber staves); hybrids assume 80 in the second stat too. The Meteorite Staff cannot be upgraded at all, which is why its ceiling sits low despite S scaling.

| Staff | Requires | Scaling (base) | Wgt | Sorc. scaling (max) | Passive |
| --- | --- | --- | --- | --- | --- |
| Lusat's Glintstone Staff | Str 10 / Int 52 | D / B | 4.0 | 413 | All sorceries cost +50% FP |
| Prince of Death's Staff | Str 6 / Int 18 / Fai 18 | E / D / D | 3.0 | 430 | Death sorceries +10% |
| Maternal Staff (DLC) | Str 7 / Int 21 / Arc 21 | E / D / C | 2.5 | 413 | Finger sorceries +15%; hemorrhage buildup on sorceries |
| Albinauric Staff | Str 6 / Int 10 / Arc 12 | E / D / C | 2.5 | 374 | Hemorrhage buildup on sorceries |
| Carian Regal Scepter | Str 8 / Dex 10 / Int 60 | E / E / B | 3.0 | 373 | Moon sorceries +10%; unique Spinning Weapon variant (FP 12) |
| Crystal Staff | Str 8 / Int 48 | D / B | 4.5 | 367 | Crystalian sorceries +10% |
| Rotten Crystal Staff | Str 8 / Int 48 | D / B | 4.5 | 365 | Crystalian +10%; scarlet rot buildup 50 on melee |
| Azur's Glintstone Staff | Str 10 / Int 52 | D / B | 4.0 | 362 | Cuts cast time (+40 virtual Dex); +20% FP cost |
| Staff of the Great Beyond (DLC) | Str 7 / Int 27 / Fai 27 | E / D / D | 2.5 | 352 | Casts both sorceries and incantations |
| Academy Glintstone Staff | Str 6 / Int 28 | E / B | 3.0 | 345 | — |
| Astrologer's Staff | Str 7 / Int 16 | E / C | 3.0 | 340 | — |
| Staff of Loss | Str 6 / Dex 12 / Int 14 | E / E / C | 2.5 | 339 | Invisibility sorceries +30% |
| Carian Glintblade Staff | Str 6 / Dex 12 / Int 22 | E / E / C | 2.5 | 337 | Glintblade sorceries +15% |
| Carian Glintstone Staff | Str 6 / Dex 8 / Int 24 | E / — / C | 3.0 | 329 | Carian sword sorceries +15% |
| Glintstone Staff | Str 6 / Int 10 | E / C | 3.0 | 329 | — |
| Gelmir Glintstone Staff | Str 6 / Int 14 / Fai 14 | E / D / D | 2.5 | 319 | Magma sorceries +15% |
| Digger's Staff | Str 8 / Int 12 | C / D | 4.5 | 305 | Stonedigger sorceries +20% |
| Staff of the Guilty | Str 8 / Fai 12 | D / C | 4.5 | 305 | Thorn sorceries +20%; blood loss buildup 45 on melee |
| Demi-Human Queen's Staff | Str 6 / Int 10 | D / C | 3.0 | 291 | — |
| Meteorite Staff | Str 6 / Int 18 | D / S | 4.5 | 272 (cannot be upgraded) | Gravity sorceries +30% |

**Breakpoints for a pure-Intelligence build**, which is the only ordering that matters in play: Demi-Human Queen's Staff from 10–40 Int, Academy Glintstone Staff from 42–67, Carian Regal Scepter from 68 up. Lusat's beats all of them from 52 Int onward if you can absorb the +50% FP cost.

Two catalysts sit outside the staff class but cast sorceries: the **Carian Sorcery Sword** (DLC, Thrusting Sword — best pure-Int option at exactly 41 Int, needs 19 Dex) and the **Staff of the Great Beyond**, which crosses the other way into incantations.

**Geometry.** Staves are uniform enough to treat as one proxy with variants: shaft 1.35–1.70 m, radius 0.03–0.05 m, head mass 0.10–0.25 m. The melee attack is a bonk with negligible scaling, so a single capsule over the head is sufficient. Distinctive heads: Meteorite Staff carries a raw unworked rock ≈0.22 m across; Carian Regal Scepter ends in a crescent ≈0.28 m wide; Crystal and Rotten Crystal Staffs end in a jagged cluster ≈0.20 m.

## Sacred seals — complete roster

Twelve seals: nine base game, three added by Shadow of the Erdtree. Same caveat as staves — none carries a Legendary rarity tag. Seals take no Ashes of War, no greases and no armament buffs.

Incant Scaling maxima are at 80 Faith, maximum upgrade; dual-scaling seals assume 80 in the second stat. Note that Strength and Dexterity scaling on a seal only feeds its melee bonk — it does not raise Incant Scaling. The two exceptions are Clawmark Seal and Frenzied Flame Seal, where the secondary stats genuinely do feed casting.

| Seal | Requires | Scaling (base) | Wgt | Incant scaling (max) | Passive |
| --- | --- | --- | --- | --- | --- |
| Golden Order Seal | Int 17 / Fai 17 | D / D | 0 | 391 | Golden Order incantations +10% |
| Dragon Communion Seal | Fai 10 / Arc 10 | D / C | 0 | 388 | Dragon Communion incantations +15% |
| Frenzied Flame Seal | none | E/E/E/D (Str/Dex/Int/Fai) | 0 | 374 | Frenzied Flame +20%; slight madness buildup on the wielder |
| Erdtree Seal | Fai 40 | C | 0 | 353 | — |
| Fire Knight's Seal (DLC) | Str 8 / Fai 23 | E / C | 1.5 | 333 | Messmer fire incantations +15% |
| Godslayer's Seal | Str 4 / Fai 27 | E / C | 1.5 | 329 | Black Flame incantations +10% |
| Finger Seal | Str 4 / Fai 10 | E / C | 1.5 | 324 | — |
| Clawmark Seal | Str 4 / Fai 10 | D / D | 1.5 | 319 | Bestial incantations +10% |
| Gravel Stone Seal | Str 4 / Fai 18 | E / C | 1.5 | 318 | Dragon Cult incantations +15% |
| Giant's Seal | Str 4 / Fai 14 | E / D | 1.5 | 307 | Giants' Flame incantations +20% |
| Spiraltree Seal (DLC) | Str 8 / Fai 17 | E / C | 1.5 | 305 | Spiral Tower incantations +20% |
| Dryleaf Seal (DLC) | Str 8 / Fai 27 | E / C | 1.5 | 304 | Golden rings of light incantations +15% |

**Breakpoints for pure Faith:** Finger Seal from 10–26, Dryleaf Seal from 27–50, Godslayer's Seal from 51–68, Erdtree Seal from 69 up. Three seals weigh literally nothing — Erdtree, Golden Order, Dragon Communion, Frenzied Flame — which is worth two or three levels of Endurance at a medium equip load.

**Off-hand stacking is a real mechanic and worth modelling.** Powerstanced catalysts resolve the spell against the Incant Scaling of the hand that casts, but the *other* hand's percentage passive still applies. Erdtree Seal in the casting hand plus Dryleaf Seal in the off-hand beats Dryleaf alone for rings-of-light spells. The same holds for staves.

**Geometry.** Seals are hand props, 0.18–0.35 m, held in the palm with no reach of their own; a single capsule on the hand covers the melee attack. The distinctive shapes: Erdtree Seal is a small golden branch-and-ring; Dragon Communion Seal is a clawed talon gripping a stone; Frenzied Flame Seal is a fused mass of yellow flame around the fingers; Clawmark Seal is a flat beast-paw brand.

## The seven Legendary Sorceries and Incantations

Four sorceries and three incantations, covered by one base-game achievement. Shadow of the Erdtree added no legendary spells.

| Spell | Type | Requires | FP | Stamina | Slots | Damage | Effect |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Comet Azur | Sorcery | Int 60 | 40 (+10 per tick held) | — | 3 | Magic | Sustained beam; hold to keep channelling. Azur's Glintstone Crown adds 15%. |
| Ranni's Dark Moon | Sorcery | Int 68 | 57 | — | 2 | Magic + frost | Slow travelling moon; lowers the target's magic resistance on hit. Long cast. |
| Founding Rain of Stars | Sorcery | Int 52 | 27 | 31 | 2 | Magic | Summons a cloud, then a delayed deluge of star rain over a wide area. |
| Stars of Ruin | Sorcery | Int 43 | 38 | 33 | 1 | Magic | Twelve homing stars in sequence; chargeable. Strong poise damage, hard to fully roll. |
| Elden Stars | Incantation | Fai 50 | 41 | 48 | 2 | Holy | Slow golden orb that sprays small projectiles; 37 stance damage. Orb detonates on contact. |
| Flame of the Fell God | Incantation | Fai 41 | 34 | 42 | 2 | Fire | Hurls a large slow fireball that lingers and burns. |
| Placidusax's Ruin | Incantation | Fai 36 | 62 | 75 | 3 | Lightning burst, then fire beam | \~8 s cast, castable while jumping, 360° steerable, 70 total stance damage. |

A few implementation notes that matter more than the raw numbers:

- **Comet Azur's FP figure is a floor, not a cost.** 40 to start, then roughly 10 per tick for as long as the beam is held. Any sim needs it as a drain, not a one-shot deduction.
- **Placidusax's Ruin is two damage types in one cast.** The opening explosion is lightning with almost no windup — effectively unreactable — and the beams that follow are fire. It is not tagged as a Dragon Communion or Dragon Cult incantation, so the Gravel Stone and Dragon Communion seal passives do nothing for it.
- **Stars of Ruin fires eight damage instances, not twelve**, despite the twelve visible projectiles: 52/52/49/49/45/45/43/43% of Sorcery Scaling, rising to 65/65/60/60/57/57/54/54% when charged.
- **Elden Stars' small projectiles are near-worthless on damage** and exist for unceasing stance pressure; the single large orb carries essentially all of the damage.

**Patch drift warning.** Several of these have been rebalanced repeatedly and the wikis have not converged. Founding Rain of Stars went from 32 FP to 27 and 37 stamina to 31 in patch 1.07; Elden Stars went from 47 FP to 41 in the same patch. Sources currently disagree on slot counts for Founding Rain of Stars (1 vs 2) and Stars of Ruin (1 vs 2 vs 3). If these numbers are going into a sim, read them out of `regulation.bin` for your installed version rather than trusting this table.

## Legendary talismans and ashen remains

Included to close the set — there are four Legendary achievement categories in the base game, and the two above are only half of them. Neither of these groups is an armament, but both are combat items and the talismans in particular change damage maths, so a combat sim that models them needs the list.

**Legendary Talismans (8)**

| Talisman | Effect |
| --- | --- |
| Godfrey Icon | Raises attack power of charged spells and skills |
| Radagon Icon | Shortens spell cast time |
| Moon of Nokstella | Adds memory slots |
| Dragoncrest Greatshield Talisman | Greatly raises physical damage negation |
| Marika's Soreseal | Raises Mind, Intelligence, Faith, Arcane; raises damage taken |
| Radagon's Soreseal | Raises Vigor, Endurance, Strength, Dexterity; raises damage taken |
| Old Lord's Talisman | Extends spell and skill duration |
| Erdtree's Favor +2 | Raises HP, stamina and equip load |

Godfrey Icon and Radagon Icon are the two that matter most for a combat sim: the first multiplies charged skill and spell damage, the second behaves as virtual Dexterity against cast speed, which interacts with the sandbox's action-length tables rather than its damage tables.

**Legendary Ashen Remains (6)**

Black Knife Tiche · Mimic Tear · Lhutel the Headless · Redmane Knight Ogha · Ancient Dragon Knight Kristoff · Cleanrot Knight Finlay.

Spirit summons are out of scope for a player-character sandbox, but Shadow of the Erdtree added two more legendary ones (Taylew the Golem Smith, Divine Bird Warrior Ornis) if you ever extend past the player.

## Methodology, caveats and weapon-class mapping

**Where the stats came from.** Attack power, requirements, scaling, weight, skills and FP costs are read from the Fextralife and Game8 stat tables, cross-checked against GameWith and RankedBoost where they disagreed. They reflect post-1.10 balance. Treat them as good enough to design against and wrong enough that you should re-derive them from `regulation.bin` before they go into a sim — which the extraction toolchain in the repo already does.

**Where the dimensions came from, and how much to trust them.** They are derived, not extracted. The method: the player character stands roughly 1.75 m, the Elden Ring/Havok unit is one metre, and weapon models were measured proportionally against the character rig in screenshots and against known class reach. Expect them to be right to within about 10% on length and rather looser on cross-section. They are a starting point for building proxies, not a substitute for reading the actual model bounds out of the game's `.flver` files — which is work the repo's extractors do not currently do.

The reason this matters: the README lists hit shapes under *still estimated*, noting that where the striking part sits on each weapon and how thick the capsule is follow the sandbox's stand-in models rather than the game's. Everything in the geometry tables above is aimed at that one gap. If you only use one thing from this document, use the striking-segment and capsule-radius columns.

**Weapon-class mapping is not one-to-one.** The sandbox implements 24 classes; Elden Ring has 32-plus in the base game and eight more in the DLC. The nine legendary armaments do not all have a home in the current list:

| Legendary armament | Elden Ring class | Nearest sandbox class | Note |
| --- | --- | --- | --- |
| Dark Moon Greatsword | Greatsword | Claymore or Greatsword | The repo's "Claymore" and "Greatsword" naming needs checking — one of them is probably ER's Colossal Sword |
| Golden Order Greatsword | Greatsword | same as above | same ambiguity |
| Marais Executioner's Sword | Greatsword | same as above | same ambiguity |
| Grafted Blade Greatsword | Colossal Sword | Colossal Weapon | ER separates Colossal Sword from Colossal Weapon; the repo has one class |
| Ruins Greatsword | Colossal Sword | Colossal Weapon | as above |
| Devourer's Scepter | Great Hammer | Great Hammer | direct match |
| Bolt of Gransax | Spear | Short Spear or Great Spear | ER's plain Spear class sits between the two the repo has |
| Sword of Night and Flame | Straight Sword | Longsword | direct match |
| Eclipse Shotel | Curved Sword | Curved Sword | direct match, but the arc geometry is not a straight capsule |

Resolve the Claymore/Greatsword/Colossal naming against `src/sim/extracted.rs` before building proxies — three of the nine land in that ambiguity.

**Out of scope in the sandbox today.** Weapon skills, parrying, stat scaling and the two-handing damage bonus are all listed as not included. Every legendary armament in this document is defined largely by its unique skill, and all of them have split damage that depends on scaling. Encoding this roster properly is therefore blocked on those three features, in that order: scaling first, then skills, then the two-handed bonus.

### Sources

- [Legendary Armaments](https://eldenring.wiki.fextralife.com/Legendary+Armaments) and the individual weapon pages on Fextralife
- [Glintstone Staffs](https://eldenring.wiki.fextralife.com/Glintstone+Staffs) and [Sacred Seals](https://eldenring.wiki.fextralife.com/Sacred+Seals), Fextralife
- [Elden Stars](https://eldenring.wiki.fextralife.com/Elden+Stars), [Placidusax's Ruin](https://eldenring.wiki.fextralife.com/Placidusax%27s+Ruin), [Stars of Ruin](https://eldenring.wiki.fextralife.com/Stars+of+Ruin), Fextralife
- [Golden Order Greatsword](https://gamewith.net/elden-ring/article/show/31808), GameWith
- [All Legendary Items in Shadow of the Erdtree](https://vulkk.com/2024/08/20/all-legendary-items-in-shadow-of-the-erdtree/), Vulkk
- [Euporia](https://eldenring.wiki.gg/wiki/Euporia), Eldenpedia
- [ELDEN-RING-Combat-Rewrite](https://github.com/Funny-Bones/ELDEN-RING-Combat-Rewrite), Funny-Bones
