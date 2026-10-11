# Charter

The other tabs explore. This one decides. If anything elsewhere in this doc contradicts this tab, this tab is right and the other tab needs editing.

**The governing rule:** when the player is fighting, it should feel like Elden Ring. When the player is doing anything else, it should feel like Skyrim.

## Decided

| Question | Decision |
| --- | --- |
| What transfers from Elden Ring | Timing and geometry, in relative terms |
| What does not | Damage model, HP, armour, stat curves |
| Who owns progression | Skyrim — skills, perks, levelling, entirely |
| Who owns gear quality | The Elden Ring axis — catalysts and armaments as a second lever |
| Poise and stance | Two separate systems, always |
| Critical ripostes | Skyrim's killmoves, retargeted to fire on a stance break instead of at random |
| Shouts | Mechanically untouched; Dragon tree perks may modify their numbers |
| Memory slots | Not implemented — two hands is the limit |
| Potions | Instant, no animation. The clutch mid-fight top-up stays exactly as it is |
| Magicka regeneration | Normal out of combat, frozen in combat |
| Catalysts | Staves and rings; required to cast; one passive each |
| Ring cost | Worn in the ring slot, but casting dedicates that hand |
| Upgrading | Smithing stones replace tempering — every weapon, staff and ring, Skyrim's included |
| Torrent | In. Instantly summoned, double jump, cannot be staggered off |
| Enemy aggression | Raised, through rule symmetry rather than authored AI |
| Saving | Skyrim's. No souls-style death, no recovery run |
| World difficulty | Skyrim's level scaling. No de-levelled regions |
| Statuses and affinities | Deferred together to a later pass, not dropped |
| Target platform | Skyrim SE via CrossOver on the M2 Pro; Windows kept as a build machine |
| Architecture | One extractor, one game-agnostic IR, two backends |

Shouts get an extra line because the decision has two halves. **The system stays exactly as it is:** cooldown-based, free, no catalyst, no magicka, usable regardless of what is held or worn. A Dragonborn stripped naked in a dungeon can still shout. **But the numbers become reachable:** the Dragon tree carries perks that modify shout damage, duration and cooldown, because shouts are the only thing in Skyrim with no progression attached at all, and that is a bigger hole than Block or Alteration. Perks move the numbers; nothing gates the ability.

## Out of scope

Each of these came up in an earlier tab, each is defensible, and each would eat the project. They are listed here so that proposing one later requires arguing against this page rather than just having a good idea at midnight.

**Rewriting Skyrim in Bevy.** Settled earlier. The wall is Papyrus, and an engine that cannot load the mod ecosystem is worse than LoreRim at everything except combat.

**Porting Elden Ring's damage model.** Motion values, attack rating, defence formulas, scaling curves. Importing any of it coherently means importing all of it, and then you have replaced Skyrim rather than integrated with it. This is the most likely way the project quietly becomes unfinishable.

**Extracting enemy behaviour.** The sandbox does not do it and its dummy's attacks are invented. Real enemy telegraphs are a separate project. For now: hand-author a handful of enemies properly rather than approximating all of them.

**Archery.** Sits awkwardly between combat and magic, no decision needed yet, and it will get designed by accident if it is not explicitly parked. Parked.

**Enchanting as a competing progression.** Flagged as an open question in the magic tab and now answered by exclusion: enchanting is not redesigned in this project. If it later needs to host the affinity system it can, but not in any phase below.

**The legendary armaments as real weapons.** The items tab is reference data for geometry and for catalyst design. Turning those nine armaments into implemented weapons with working unique skills needs the weapon-art system finished first, so it is a reward for getting there, not a milestone on the way.

**Nightreign anything.** It exists, moveset packs are appearing, and it is not this project.

### The scope test

Before adding anything, ask: **does this change what happens in the three seconds around a swing?** If yes, it is probably in. If it changes what happens over three hours of play, Skyrim already has an answer and the answer is Skyrim's.

### Deferred, not dropped

Two systems are worth building and are explicitly not being built yet, because they are coupled to each other and neither is coupled to anything in the phases below.

**Status buildup.** Bleed, frostbite, rot, poison, sleep, madness as invisible meters rather than Skyrim's flat weak effects. This is what turns weapon choice into a decision rather than a damage comparison, and it is the thing that would finally give Alchemy a combat job. Until it lands, status effects stay strictly Skyrim's.

**Affinities.** Elden Ring's infusion system — Heavy, Keen, Sacred, Cold, Bleed and the rest — which is the elegant fix for enchanting being Skyrim's most broken system. It depends on statuses existing, so it waits for them.

Both get tuned in once the combat core is real and there is something to tune against. Filing them here rather than under out-of-scope because the distinction matters: these are postponed, not rejected.

**Rejected outright:** a drinking animation. Elden Ring's committed flask creates real tension and it would cost the thing that makes Skyrim combat recoverable — the instant top-up in a pinch is good UX and it stays.

## The scaling spine

Two tabs in this doc disagreed about scaling and you were right to catch it. The integration plan says refuse Elden Ring's scaling; the magic tab says import Elden Ring's Scaling number. Both were reaching for something true and neither said it cleanly. This is the resolution, and it is the rule everywhere.

**There are two axes, and each game already owns one.**

| Axis | Owner | What it is | What moves it |
| --- | --- | --- | --- |
| Progression | **Skyrim** | How strong the character is | Skill levels, perks, attribute points |
| Gear quality | **Elden Ring shape** | How strong the implement is | Which catalyst or armament, and its upgrade level |

Skyrim already has both axes for melee: a sword's damage comes from the weapon plus One-Handed plus perks, and nobody finds that confusing. Skyrim's magic is broken precisely because it has only the first axis — Destruction has perks but the spell's number never moves, so the mage stops growing while the warrior keeps finding better swords.

So the fix is not to import Elden Ring's scaling system. It is to **give magic the gear axis that melee already has**, and let Skyrim's perks keep doing the progression job they already do. That is a smaller change than the magic tab implied, it does not touch the damage model, and it is consistent with the integration plan's refusal.

### What this means concretely

- A catalyst has a quality value, the way a sword has base damage. Elden Ring's roster supplies the **ordering and the relative gaps** — Lusat's above the Academy Staff above the Demi-Human Queen's — and Skyrim's health economy supplies the absolute numbers.
- A spell's power is a percentage of that value rather than a fixed number. This is the one genuinely imported mechanic and it is worth it: it means twenty catalysts tune the entire spell library.
- **Perks still do everything they do now.** Cost reduction, duration, utility, the dual-cast replacements, every branch of every tree. Nothing in the perk trees is deleted to make room.
- No new stats. No Intelligence, no Faith. The governing skill for a catalyst is the Skyrim school it belongs to.

### Where the Elden Ring systems get their numbers

Poise, stance, i-frames and stamina costs have no Skyrim counterpart to defer to, so they need a home. They go on the gear axis too — armour weight drives poise, equip load drives roll tier, weapon class drives stance damage — and the perk trees modify them, which is the subject of the next section.

That keeps the shape consistent: **gear sets the baseline, perks move it, and the player's build is the interaction between the two.** Same sentence for a greatsword, a staff and a set of heavy armour.

## Perk tree map

Every Elden Ring system needs somewhere for the player to invest in it, and Skyrim already has eighteen trees. Three of them are nearly vestigial in vanilla — **Block, Heavy Armor and Alteration** are the perennial "why would you" trees — and they turn out to be exactly the right hosts for the three systems with no Skyrim home. That is a good sign rather than a coincidence: they are empty because vanilla Skyrim has no mechanics that need them, and these mechanics are precisely what was missing.

| Tree | Hosts | Vanilla state | Notes |
| --- | --- | --- | --- |
| **Block** | Guard counter, parry window width, stance damage dealt on block | Vestigial | The biggest revival. Blocking becomes an offensive option |
| **Heavy Armor** | Poise thresholds, hyperarmour duration during attacks and casts | Vestigial | Makes the heavy-armour caster a real build, as in Elden Ring |
| **Light Armor** | Equip-load roll tier, i-frame span, roll stamina cost | Weak | The dodge build's home |
| **Alteration** | Gravity sorceries, Carian blade spells, the weight/load manipulation fantasy | Weakest school | Gravity is a perfect identity for a school that has never had one |
| **Conjuration** | Spirit ashes, Dragon Communion, bound armaments | Healthy | Direct one-to-one fit; needs the least design work |
| **Restoration** | Incantations, holy damage, undead interaction | Healthy | Ring catalysts' primary tree |
| **Destruction** | Elemental spells, split across both catalyst families | Healthy but non-scaling | Fixed by the gear axis, not by perks |
| **Illusion** | Invisibility and night sorceries | Healthy | Gains a catalyst identity it has never had |
| **One-Handed** | Stance damage, powerstancing, weapon art access | Healthy |  |
| **Two-Handed** | Charged heavy attacks, colossal commitment, poise during swings | Healthy |  |
| **Smithing** | Smithing stones for every weapon, staff and ring | Healthy | Replaces tempering; stone tier sets the ceiling, not the skill |
| **Alchemy** | Greases and combat consumables | Healthy | Stays thin until statuses land — that is what gives it a real job |
| **Sneak** | Backstab criticals, riposte damage on stance break | Healthy |  |
| Archery · Enchanting · Lockpicking · Pickpocket · Speech | — | — | Untouched this project |

**The one new tree: Dragon.** Two branches — Dragon Communion (breath and transformation, bought with dragon souls) and Dragon Cult (lightning, taught by Dragon Priests) — plus a third branch of perks that modify shouts without gating them. Designed in full on the Magic overhaul tab.

### The three revivals, in more detail

**Block becomes the aggressive tree.** Guard counter is the only fully faithful Elden Ring mechanic already shipping in Skyrim, and it currently hangs off no progression at all. Put the counter's damage, the parry window's width, and stance damage dealt while blocking into Block perks, and the tree stops being a tax on shield users and becomes a build.

**Heavy Armor becomes poise.** This is the cleanest mapping in the entire project. Elden Ring's poise comes from armour weight; Skyrim's Heavy Armor tree does nothing interesting. Poise thresholds and hyperarmour duration go here, and the tree immediately justifies itself for both melee and casters. It also creates the Elden Ring build Skyrim has never been able to express: the slow, armoured mage who trades hits to finish a cast.

**Alteration becomes gravity.** Alteration has no fantasy — it is a grab bag of armour spells and detection. Elden Ring's gravity sorceries are a complete, distinctive, mechanically interesting school that Skyrim has no version of, and they are thematically Alteration in every respect. Pull, slam, weightlessness, meteor. This is where the "integrate Elden Ring where it belongs" instruction pays off best, because it fills a hole rather than competing with something.

### The discipline

No new stats, no replaced trees, and **exactly one new tree**: Dragon. It earns the exception on three grounds — it houses thirty-one imported spells with no coherent home elsewhere, it is the Dragonborn fantasy, and it gives shouts the progression Skyrim never gave them. Everything else lands inside an existing tree or it does not land. If something has no plausible host, that is evidence it does not belong in this project rather than evidence that Skyrim needs another tree.

## Imported features

Four additions beyond poise, stance and the magic work. What unites them is that **none of them adds a system** — each takes a Skyrim feature that already exists and is inert, and gives it a reason to matter. That is far cheaper than importing, and it is why Block and Alteration felt clean.

### Ripostes from stance break

Skyrim's killmove animations already exist and are currently random cosmetic noise — common enough that people install mods to suppress them. Elden Ring's critical is the same animation with one difference: you earned it.

So the killmoves are not replaced, they are **retargeted**. They stop firing on a damage roll and start firing on a stance break. The animations are already in the game, the stance system is already being built in Phase 2, and the wiring between them is the entire change. A dead feature becomes the payoff for the whole poise-and-stance loop, and Sneak gets somewhere worth investing.

### Smithing stones for everything

Tempering is replaced wholesale. Every weapon in the game goes on Elden Ring's two-track stone system — standard stones for ordinary arms, somber stones for unique and named ones — and **staves and rings go on it too**.

That last part is not a flourish. The gear axis in the scaling spine has an upgrade-level term in it, and this is what fills it. A mage upgrading a catalyst is now doing exactly what a warrior upgrading a sword is doing, with the same materials, on the same track. The two halves of the progression design meet here.

It also fixes Skyrim's most broken skill by accident. Smithing currently multiplies temper quality without limit, which is how a generic Daedric sword ends up outclassing every artifact in the game. Under stones, **the stone tier sets the ceiling and the skill cannot raise it** — Smithing perks go back to governing what you can forge and work with, not how far you can push a number. Named weapons stay relevant because somber stones are their own scarcity.

### Torrent

In, fully, and it is worth being clear that this is the most expensive item on the list and the only one that is pure addition rather than revival.

- **Instantly summoned.** A whistle, not a stable. Skyrim's horse ownership and stabling economy goes.
- **Double jump**, which is Torrent's actual signature and the reason the Lands Between is traversable the way it is.
- **Cannot be staggered off.** Hits do not dismount you. This is the super-armour you asked for and it is what makes mounted combat usable rather than a liability.
- **Has its own health, and can die.** Resummon on a cooldown. Without this there is no tension at all and Torrent becomes a movement cheat.
- **Locked out of interiors**, which Skyrim enforces for free and which matches Elden Ring's legacy dungeons exactly.

The cost is mounted combat animation. Skyrim's mounted moveset is four attacks and a shrug; Elden Ring has committed mounted attacks on both sides with their own timings. That work is real and it is also **completely independent of everything else** — Torrent touches no combat framework, no catalyst, no perk tree. It can be built at any point without blocking or being blocked.

### Enemy aggression, through symmetry

The NPC problem has no cheap answer, but it has a cheap *partial* answer, and it is not smarter AI.

The single largest gain is making enemies **play by the same rules the player now plays by**: committed attacks they cannot cancel out of, stamina they actually spend, poise that can be broken, hit windows that can be read. Most of what reads as "Elden Ring enemies are smart" is really "Elden Ring enemies are legible," and legibility comes from symmetry, not intelligence.

So, in order of value per unit of work:

1. **Rule symmetry** — BFCO's native NPC moveset support puts enemies on the same committed attack system. Nearly free, biggest effect.
2. **Movement IQ** — Combat Pathing Revolution for advancing, circling, backing off. Off the shelf.
3. **Aggression and confidence flags** — vanilla AI values, turned up. Free, and easy to overdo.
4. **Hand-authored telegraphs** — five enemy types done properly, not fifty approximated.

Stop at three unless the slice proves it needs four. The failure mode is building an enemy-behaviour extractor, which is the separate project the scope page already rules out.

## Phases

Each phase has a stop condition. If a phase cannot meet it, that is information about the thesis, not a reason to start the next one.

**Phase 0 — Native sandbox.** Swap the Oodle dependency or bake the data on Windows; get an aarch64 build of the Bevy sandbox running on the M2 Pro. *Done when:* `cargo run` works natively and the demo plays through.

**Phase 1 — The colossal sword slice.** Geometry, timings, root motion, i-frames, hit-stop. One weapon class, end to end, through the IR and into Skyrim. *Done when:* a swing in Skyrim and the same swing in the sandbox agree on every window to within one frame, with no hand-tuning.

**Phase 2 — Poise, stance and ripostes.** The two interruption systems kept separate, with Heavy Armor and Block perks behind them, and Skyrim's killmoves retargeted to fire on a stance break. *Done when:* a heavy-armour build can trade through a hit to finish a swing and a light build cannot, and a killmove only ever plays because the player broke something.

**Phase 3 — The gear axis.** Smithing stones replacing tempering across every weapon, staff and ring; catalyst quality values; spells converted to power ratios. *Done when:* a mage played from level 1 to 30 does not hit the wall, and a named weapon is still worth carrying at level 40. Cheapest test of the largest unproven assumption in the project, and it needs no toolchain.

**Phase 4 — Catalysts proper.** Rings as hand-dedicating catalysts, staves rebuilt from charged items, powerstancing, passives. *Done when:* two rings with different passives produce a meaningfully different build from one ring.

**Phase 5 — New spell content.** Gravity into Alteration, summoning into Conjuration, the Elden Ring archetypes Skyrim's vocabulary lacks. *Done when:* Alteration is a school someone would choose on purpose.

**Phase 6 — Enemy symmetry.** Rule symmetry first, then pathing, then aggression values. Hand-authored telegraphs only if the slice proves they are needed. *Done when:* an enemy attack can be read and punished the way a player's can.

**Phase 7 — Torrent.** Summon, double jump, stagger immunity, its own health, mounted movesets. *Done when:* mounted combat is something you choose rather than something you survive.

Torrent is numbered last but blocks nothing and is blocked by nothing — it touches no framework the other phases touch. If the animation work is the part that sounds fun on a given weekend, do it then. Statuses and affinities come after all of this, together.

### What to do this week

Phase 0, and nothing else. It is the only phase with no design questions left in it, and until the sandbox runs natively every other phase is theory.

Phases 1 and 3 are independent of each other and both independent of the ring design, so if Phase 0 stalls on the Oodle question, Phase 3 is the one to start instead — it is pure record editing, needs no toolchain, and tests the largest unproven assumption in the project.

## Encounters

**Phase 8 — Authored encounters.** Elden Ring bosses placed into Skyrim, built on top of the symmetry work in Phase 6. The principle from the enemy section still holds: an imported boss that does not play by the new rules is a reskinned Draugr Deathlord, so none of this starts before Phase 6 is real.

### Encounter 1 — Bleak Falls Barrow (priority)

The Castle Morne fight, staged in the word wall chamber of Skyrim's second main-quest dungeon.

**The naming correction first, because it decides the asset.** The boss at Castle Morne that drops the Grafted Blade Greatsword is the **Leonine** Misbegotten — a bespoke boss model, hunched, maned, wielding the sword two-handed. The **Scaly** Misbegotten is a different, smaller, common enemy, and it is deliberately **not** used here.

**The approach stays draugr, and that is the design.** This is the first Elden Ring boss the player ever meets, so nothing should prepare them for it. They clear a barrow full of exactly what a barrow is full of, the coffin bursts open the way it always does, and what comes out is not a draugr. Foreshadowing with scaly misbegotten on the way in would soften the only moment in the game that gets to land cold. It also halves the creature rigging for the phase to a single model.

**The encounter:**

- The barrow is unchanged up to the word wall chamber. Draugr throughout, no hints.
- Leonine Misbegotten replaces the draugr that bursts from the coffin. One boss, no adds.
- **Boss health bar across the bottom of the screen.** TrueHUD is already in the stack and already does this — it is the single cheapest thing in the whole encounter and it does more than any other element to tell the player what kind of fight they just walked into. Nothing else in Skyrim gets one, which is exactly why this should.
- Drops the **Grafted Blade Greatsword** — real stats from the items tab: Str 40 / Dex 14, 21.0 weight, Oath of Vengeance.
- The word wall stays exactly where it is. Shouts are untouched and this encounter does not change that.

**The weapon is unusable and that is the point.** Grafted Blade needs 40 Strength. At Bleak Falls Barrow the player has nothing like it, so they carry a colossal sword around for twenty hours before they can lift it. That is precisely what Elden Ring does at Castle Morne, and it is one of the best feelings the game produces — a trophy that is also a promise.

**Do not break the main quest.** The draugr in that chamber carries the Dragonstone, which Farengar needs. Leonine has to drop it, or it has to be placed in the chamber. This is the kind of detail that is trivial to handle and ruinous to forget.

**This is a wall in the second dungeon of the main quest, deliberately.** Bleak Falls Barrow is a level-5 area and the Leonine Misbegotten is not a level-5 fight. That makes it Margit — the early, unfair, unforgettable one that teaches the player what kind of game they are now in. It should be beatable, barely, by someone who has learned to dodge and has not learned anything else.

### Encounter 2 — The Bell Bearing Hunters

After the misbegotten, despite being cheaper, because the misbegotten lands earlier in the player's run.

- A hunter appears at a Dragon Priest's site **only after that Priest is dead**, and only once the player has left and returned after 24 in-game hours. Same return-to-a-cleared-place ambush Elden Ring uses.
- **The last one is Elemer of the Briar**, which is canon rather than invention — Elemer *is* the Bell Bearing Hunter, and Marais Executioner's Sword with Eochaid's Dancing Blade is his. Trigger him off a kill count rather than a location so he is the eighth hunter wherever that falls.
- **Vary the weapon per hunter.** Elden Ring only has four and they are the same fight, which works because you meet them rarely. Eight identical ones in a set the player is deliberately collecting will wear thin. Either give each a different armament or only place hunters at five of the eight sites.

**Cost note:** this encounter needs no creature work at all. The hunters are human, Skyrim's humanoid skeleton already fits, Elden Ring humanoid movesets are already ported and on Nexus, and Marais Executioner's Sword is a static mesh. It is pure record and script work, which makes it the cheapest authored encounter in the project by a wide margin even though it is second in priority.

### Encounter 3 — Giants

Elden Ring field bosses placed as roaming overworld encounters. Needs new rigged creatures, so it sits last.

### What this costs, honestly

Everything except the hunters needs **creature rigging**, and that is the expensive word. Elden Ring meshes are FLVER, Skyrim's are NIF, and the two use different Havok skeletons — different bone counts, different names, different hierarchies. Per creature that means converting the mesh, rebuilding the rig against a Skyrim-compatible skeleton, and retargeting every animation. Days of Blender work each, not a script.

The mitigation is to **separate the encounter from the creature**. Build the Bleak Falls fight with a placeholder — a reskinned Draugr Deathlord with the right stats, poise, telegraphs, arena and drop — and confirm the encounter is good before anyone rigs anything. If the fight is not fun with a placeholder, the model will not save it, and you will have learned that for free.
