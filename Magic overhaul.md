# Magic overhaul

**The governing rule for the whole project:** when the player is fighting, it should feel like Elden Ring. When the player is doing anything else — exploring, looting, talking, levelling, travelling — it should feel like Skyrim.

That rule decides more of the magic design than any amount of taste does, because almost every disagreement between the two systems turns out to sit cleanly on one side of the line.

## The rule, applied

Split every magic subsystem by where the player encounters it. If they meet it mid-fight, Elden Ring wins. If they meet it at a menu, a merchant or a chest, Skyrim wins.

| Subsystem | Elden Ring | Skyrim | Encountered | Verdict |
| --- | --- | --- | --- | --- |
| Resource pool | FP, separate | Magicka, separate | Mid-fight | Magicka becomes FP — same shape already |
| Resource regen | None in combat | Regenerates always | Mid-fight | **Elden Ring** — no regen while hostile |
| Resource refill | Flask, charges, bonfire | Potions, unlimited | Out of fight | **Skyrim** — keep potions |
| Catalyst required | Yes, always | Optional | Mid-fight | **Elden Ring** — no barehand casting |
| Cast commitment | Long, committed, interruptible | Fast, cancellable | Mid-fight | **Elden Ring** — this is most of the feel |
| Spell scaling | One Scaling number per catalyst | Essentially none | Mid-fight | **Elden Ring** — the biggest single win |
| Dual-wield casting | Two catalysts, passives stack | Dual-cast for 2.2× | Mid-fight | **Elden Ring** — catalyst powerstancing |
| Loadout limit | Memory slots, set at grace | Unlimited spellbook | At a menu | **Skyrim** — hands already limit you |
| Spell acquisition | Found and bought as items | Tomes, trainers, levelling | Out of fight | **Skyrim** |
| Stat requirements | Int / Fai gates | Skill level gates | At level-up | **No gates** — skills scale results only |
| Staff behaviour | Catalyst | Charged spell-dispenser | Mid-fight | **Elden Ring** — rebuild vanilla staves |

Two of those deserve the reasoning spelled out, because they are the ones where the obvious Elden Ring answer is wrong.

**Memory slots go.** They are iconic and they do not belong here. A player sets their loadout at a site of grace — a menu, out of combat, in the world layer. By the rule, that is Skyrim's, and Skyrim's answer is an unlimited spellbook. You lose less than you think: the player still has only two hands, and both of them have to hold a catalyst to cast, so the in-combat limit Elden Ring actually enforces survives intact. What you lose is the pre-fight commitment, and what you gain is not fighting Skyrim's entire spell-acquisition design.

**The flask goes, the regeneration stays gone.** This looks contradictory and is not. Attrition between fights is a progression feel and belongs to the world; Skyrim's answer is potions and you should keep them. But magicka ticking back up *while an enemy is swinging at you* is a combat feel, and it quietly deletes the resource pressure that makes Elden Ring casting a decision. So: full Skyrim economy outside combat, hard stop on regeneration the moment you are in it.

That pair is the template for every other call in this document. Where the two games disagree, ask which screen the player is looking at.

## Catalysts: staves and rings

Two families, mirroring Elden Ring's split, with rings standing in for sacred seals.

- **Staves** — the Destruction / Alteration / Illusion side. Elden Ring's glintstone sorceries. Visible, held, ostentatious.
- **Rings** — the Restoration / Conjuration side. Elden Ring's incantations. Worn, quiet, devotional.

Rings are the better choice and not only aesthetically. Skyrim already has a ring slot the player understands, rings already carry enchantments, and "a ring that lets you pray" reads as Tamrielic in a way a sacred seal does not. It also sidesteps the fact that Elden Ring's seals are visually forgettable — most players could not describe one.

### The problem rings create

A seal occupies a hand. That is not decoration; it is the entire cost structure of incantations. Holding a seal means not holding a shield, not two-handing your weapon, and choosing which hand gets the passive when you run two. Every meaningful decision about incantation builds in Elden Ring comes from that one constraint.

A ring occupies a finger. Fingers are free. Port the ring naively and incantations become something you always have, at no cost, alongside a full weapon-and-shield loadout — which is not an Elden Ring experience, it is Skyrim's, where magic is a thing you can always do.

### The fix: worn, but cast with a hand

The ring is equipped in the ring slot and is visible on the hand. Casting from it requires **dedicating that hand** — raising it, which means that hand is not blocking, not attacking, and not two-handing for the duration of the cast and its recovery.

Mechanically the ring behaves as a hand-slot catalyst that happens to render as worn jewellery rather than a held object. You keep Elden Ring's cost structure exactly, and you get an animation that is better than Elden Ring's: an open palm raised, rather than a small object held awkwardly.

This also preserves powerstancing, which is the mechanic most worth saving. Two rings — one per hand — means the hand you cast from supplies the Scaling and the other supplies its passive. That is Elden Ring's off-hand seal trick intact, and it needs a second ring slot, which several existing Skyrim mods already provide. Confirm one works with your framework before committing to this design; if only one ring slot is available, the whole off-hand passive layer collapses.

### Staves have the opposite problem

Vanilla Skyrim staves are not catalysts. They are charged consumables that cast one fixed spell until the charge runs out, recharged with soul gems. That concept has to be deleted and replaced, which means touching every staff record, every loot table that drops one, and every vendor that sells them.

This is the single largest piece of record-editing work in the overhaul and it is unavoidable, because leaving both systems in place gives you two unrelated things called staves. Budget for it as its own task rather than discovering it halfway through.

### The catalyst record

Every catalyst, staff or ring, carries exactly two gameplay properties. The roster in the legendary items tab is still the design template — twenty staves and twelve seals — but its source-game requirements are evidence only and are discarded by the Skyrim translator:

1. **A Scaling value** — derived, drives all spell power cast from it.
2. **At most one passive** — a named spell family boosted by a stated percentage, or nothing.

A level-one character may equip every catalyst and use its complete spell
family immediately. Skyrim skills and perks improve magnitude, efficiency, or
other results; they never suppress scaling, disable a spell, or create a
minimum-use threshold.

That passive constraint matters. Elden Ring's catalysts are memorable because each does one thing: Meteorite Staff boosts gravity spells by 30% and nothing else; Godslayer's Seal boosts black flame by 10% and nothing else. A catalyst with three passives is a Skyrim enchanted item, not an Elden Ring catalyst, and it will feel like one.

## Scaling

This is the part of Elden Ring's magic worth importing above everything else, and it happens to fix Skyrim's longest-standing balance failure.

Skyrim's Destruction does not scale. A Firebolt cast at level 5 and the same Firebolt cast at level 50 do identical damage; skill and perks reduce its cost and add utility, but the number never grows. Every melee build gets stronger with better gear forever, and the mage hits a wall somewhere around level 30. This is the complaint, and it has been the complaint since 2011.

Elden Ring solved the same problem with one number. **Sorcery Scaling** and **Incantation Scaling** are a single derived value on the catalyst, and every spell's damage is a percentage of it. A spell description says what it does; the catalyst says how hard it hits. Upgrade the catalyst or raise the stat and every spell you own gets better at once.

Import the shape, not the progression. Skyrim's skills and perks keep owning how strong the character is; the catalyst becomes the second axis, exactly as a sword already is for a warrior. See the Charter's scaling spine — this section originally overstated the import, and the Charter is the authority.

### How the number gets built

```latex
\text{Scaling} = \text{Base}_{\text{catalyst}} \times f(\text{skill}) \times (1 + \text{upgrade level})
```

- **Base** is the catalyst's own quality — the thing that separates a Glintstone Staff from the Carian Regal Scepter.
- **f(skill)** is the scaling curve against the governing Skyrim skill, with the same soft-cap shape Elden Ring uses rather than a straight line.
- **Upgrade level** maps onto Skyrim's smithing or enchanting, whichever you want owning catalyst progression.

And then every spell carries a **power ratio** — a percentage of Scaling — exactly as Elden Ring's spells do. Not a damage number. That is the whole trick: a spell's identity becomes its shape, cost and cast time, while its magnitude lives entirely in the catalyst.

### Why this is worth the disruption

It makes catalysts the mage's weapon progression, which Skyrim has never had. A mage finding a better staff is now the same event as a warrior finding a better sword, and the roster in the legendary items tab gives you twenty of them already differentiated and ranked — the Scaling numbers there are real values from a shipped, balanced game, and the ordering between them is known good.

It also means the two catalyst families can be balanced against each other with one lever instead of auditing every spell. Rings coming out ahead of staves at high skill is a single curve adjustment.

The cost is that every spell record in the game needs its damage replaced by a ratio. That is mechanical, scriptable, and tedious, and it is where an afternoon of careful work buys you a system that tunes itself for the rest of the project.

### One warning

Do not reuse Elden Ring's absolute Scaling values. Those are calibrated against Elden Ring's HP pools, which the integration plan explicitly refuses to import. Take the **ratios between catalysts** — Lusat's at 413 against the Academy Staff's 345 — and rebuild the absolute magnitudes against Skyrim's health economy. Same ordering, same relative gaps, different units. This is the ledger's ratio rule applied to magic, and the failure mode if you ignore it is a level-10 mage one-shotting dragons.

## Resource model and commitment

### Magicka is already FP

The luckiest correspondence in the whole overhaul: Skyrim's magicka is a separate pool from stamina, exactly as Elden Ring's FP is. No new resource needed, no HUD work, no plugin. Keep the name — the world layer is Skyrim's and the player should read "magicka" on their own character sheet.

Three changes to its behaviour, all combat-layer:

1. **No regeneration while in combat.** Out of combat, Skyrim's normal regen. The moment a hostile is aware of you, the bar freezes. This single change converts magicka from a cooldown into a resource.
2. **Potions restore it, and potions are finite in the moment.** Consumables remain instant and animation-less, as the Charter requires. Commitment animations are deferred until throwables; ordinary potions do not quietly acquire Elden Ring's flask animation.
3. **Casting costs come from the catalyst's spell, not the spell alone.** Elden Ring's Lusat's Glintstone Staff is the design proof: best-in-class Scaling at +50% FP cost on everything. That trade should exist on at least two or three catalysts, because it is the most interesting decision in Elden Ring's casting economy.

### Cast commitment is most of the feel

Skyrim casting is a held button with an instant release and no recovery. Elden Ring casting is an animation you are locked into, with a startup you can be punished during and a recovery you can be punished after. If you change nothing else about magic, change this.

It also means casting goes through the same machinery as attacks, which is why this section is short: **spells are actions**, and the integration plan already describes how actions get their timings. A spell gets a length, a release frame, a cancel window, a recovery, hyperarmour during part of it, and a stamina cost — the same fields as a greatsword swing, emitted by the same backend.

That is a real architectural saving. There is no separate magic timing system to build; there is a catalyst weapon class and its spells are its movesets.

Consequences worth naming:

- **Poise applies to casting.** Getting hit mid-cast should interrupt unless your poise carries it. Elden Ring's heavy armour caster is a real build for exactly this reason, and Skyrim has no equivalent today.
- **Cast speed is a stat.** Elden Ring scales it with Dexterity. Skyrim has no Dex, so pick a governing skill or a derived value, and make it visible — a mage who cannot tell whether their casts got faster will not believe they did.
- **Spells cannot be cancelled into a roll** past the cancel window. This is the thing that makes Elden Ring casting feel dangerous, and it is the thing Skyrim players will complain about first.

### What this does to Skyrim's dual-cast

Skyrim's dual-cast — same spell in both hands for 2.2× damage at 2.8× cost — conflicts directly with catalyst powerstancing, and powerstancing is the better mechanic because it creates build decisions rather than a damage multiplier.

Replace it. Two catalysts means the casting hand supplies Scaling and the off-hand supplies its passive. A player running Lusat's for raw power with a Meteorite Staff off-hand for the gravity bonus is making an actual choice; a player holding the same spell twice is pressing a button harder.

## The spell roster

Skyrim has five schools; Elden Ring has two catalyst families and a dozen or so named spell groups. They map better than you would expect, because Elden Ring's groups are mostly thematic while Skyrim's schools are mostly mechanical.

| Skyrim school | Catalyst | Elden Ring counterpart | Fit |
| --- | --- | --- | --- |
| Destruction | Both | Glintstone and magma sorceries; Giants' Flame, Dragon Cult, Black Flame incantations | Straddles deliberately — fire exists on both sides in Elden Ring too |
| Alteration | Staff | Carian sword sorceries, gravity, crystal | Clean — Alteration is already the manipulation school |
| Illusion | Staff | Invisibility sorceries (Staff of Loss boosts them 30%) | Clean, and gives Illusion a catalyst identity it has never had |
| Restoration | Ring | Erdtree incantations, Golden Order, healing | Clean |
| Conjuration | Ring | Dragon Communion, Bestial, bound weapons | Clean — summoning is devotional in both games |

**Destruction straddling is correct, not a compromise.** Elden Ring itself puts fire on both sides: magma sorceries are cast from a staff, Giants' Flame incantations from a seal, and they feel different because of it. Keeping Destruction castable from either catalyst — with *different spells* on each side — preserves that, and gives you an obvious reason for a player to run one of each.

### How the two spell libraries combine

Three tiers, and the rule for which is which is about what the spell *does*, not where it came from:

1. **Shared shapes — merge them.** A fireball is a fireball. Skyrim's Firebolt and Elden Ring's Catch Flame occupy the same slot; keep the Skyrim name and the Skyrim visual, give it Elden Ring's cast commitment and the new power ratio. Most of Destruction lives here. The player should not notice a seam.
2. **Elden Ring shapes Skyrim lacks — import them.** Delayed-detonation spells, the charged-beam archetype, spells that leave a persistent hazard, spells with a travel-then-rain structure. Skyrim's spell vocabulary is genuinely narrow — projectile, cone, rune, ward, summon — and this is where the overhaul earns its name. The seven legendary spells in the items tab are each a distinct archetype Skyrim has no version of.
3. **Skyrim shapes Elden Ring lacks — keep them untouched.** Wards, Illusion's fear and calm and frenzy, Clairvoyance, soul trap, bound weapons. These are Skyrim's identity and several of them are world-layer tools. Give them catalyst scaling and commitment timings, change nothing else.

The mistake to avoid is importing Elden Ring spells that duplicate something Skyrim already has under a different name. You end up with two Fireballs, one of which is strictly better, and the player learns to ignore half the library.

### Catalyst passives give the roster its structure

Once spells carry a family tag, the catalyst passives from the real roster become the thing that makes builds. The twelve seals and twenty staves in the items tab are already a worked example of how many distinct families you need — roughly a dozen, each with one or two catalysts that favour it.

Map those families onto the combined library and you get the build space for free: a gravity staff wants Alteration spells, a black flame ring wants a specific Destruction subset, a Dragon Communion ring wants Conjuration. The player assembles a build by picking a family and then hunting the catalyst that boosts it, which is exactly the loop Elden Ring runs and exactly the loop Skyrim's magic has never had.

## Remaining decisions

Three real forks remain. The archery and shout boundaries are recorded here
too because they constrain the magic implementation even though they are
settled.

**What governs Scaling, when Skyrim has no Intelligence or Faith?** Three options. Use the existing school skills, which is least disruptive but means a Destruction mage's staff and ring scale off different bars. Add two derived stats behind the scenes, which is cleanest and most Elden Ring but needs UI work. Or scale off Magicka investment, which is simple and makes every mage identical. I lean toward school skills for the first pass because it costs nothing and you can revisit once you know how it plays.

**Archery — settled beside magic, not inside it.** Bows are committed weapon
actions governed by Skyrim's Archery progression; arrows are payload carriers
for the shared elemental and status pipeline. Skyrim remains authoritative for
projectile flight, collision, bow/arrow base damage, inventory, and perks.
SkyRing owns draw/release/recovery timing and correlates the released arrow to
one confirmed impact. The bow slice should prove this ranged adapter before
projectile spells reuse it, but bows never require a catalyst or magicka.

**Shouts — settled, and the answer is nothing.** They stay exactly as they are: vanilla behaviour, vanilla cooldowns, usable regardless of what is held or worn. No catalyst, no magicka, no scaling. The governing rule appeared to break here, but only because shouts are a story reward wearing combat clothes — there is nothing on the Elden Ring side of the comparison to weigh against.

**Enchanting.** If catalysts are the mage's weapon progression, enchanting is a parallel progression that can undercut it. Elden Ring has no crafted-gear economy. Decide whether enchanting feeds catalyst upgrades, or competes with them, before you build either.

**Second ring slot.** The powerstancing design depends on it. Verify a mod providing one works alongside your chosen framework before the ring design hardens, because the fallback — one ring, no off-hand passive — is a meaningfully smaller system and you would want to know early.

### What I would build first

The shared elemental, buff/debuff and status pipeline comes before this tab, because melee and magic must not grow parallel effect systems. Once that prerequisite exists, Scaling is the first specifically magical feature. It is self-contained, it does not depend on the ring slot question or the school mapping, and it is testable on its own: take three existing staves, give them Base values in the right ratio, convert a dozen Destruction spells to power ratios, and play a mage from level 1 to 30. If the wall at level 30 is gone, the thesis of the entire magic overhaul is confirmed before you have touched a single animation or edited a single ring record.

If it is not gone, you have learned that cheaply, which is the whole point of doing it first.

## The Dragon tree

This is the one justified exception to the Charter's no-new-trees rule, and the argument is not that the spells are cool. It is that **shouts have no progression at all** — no perks, no scaling, nothing between learning a word and using it — and that is a weaker gap in Skyrim's design than Block or Alteration. A Dragon tree fixes that while housing thirty-one imported spells that have nowhere else coherent to live.

### The gift nobody planned

Three coincidences line up so well they are worth stating before the design:

**Both games have a Dragon Cult.** Elden Ring's Dragon Cult incantations are lightning spells bought from priests. Skyrim's Dragon Cult is the Dragon Priests of Bromjunaar, Labyrinthian, and eight masked bosses. Same name, same concept, no reconciliation needed — the lightning spells become what the Dragon Priests actually taught.

**Dragon Hearts are already in Skyrim, and they are useless.** Elden Ring buys Dragon Communion incantations with Dragon Hearts rather than runes. Skyrim gives you dragon souls, and by mid-game you have a stack of them with nothing to spend on because you have already unlocked every shout word you want. Make dragon souls the Dragon Communion currency and you have fixed a known dead-end in Skyrim's economy using the exact mechanic Elden Ring already designed for it. This is the cleanest import in the whole project.

**The breath incantations transform your head into a dragon's.** On the Dragonborn. That needs no justification.

### Shouts: perks yes, catalysts no

You floated folding shouts into the catalyst scaling. I would draw the line one step short of that, and the distinction matters:

- **A Dragon tree perk that makes Fire Breath hit harder, last longer or cool down faster** — yes. That is giving shouts the progression they have never had, and it does not change how they are used.
- **Requiring a catalyst to shout, or paying magicka for it** — no. That breaks "independent of what is wielded," which is the thing that makes shouts feel like a birthright rather than equipment. A Dragonborn stripped naked in a dungeon should still be able to Fus Ro Dah.

So the tree has a shout branch that modifies shouts, and shouts themselves stay mechanically untouched: cooldown-based, free, castable with both hands full. Same relationship Heavy Armor has with poise — the tree moves the number, it does not change the system.

### The two families, and what they become

|  | Dragon Communion | Dragon Cult |
| --- | --- | --- |
| Count | 16 | 15 |
| Flavour | Visceral. You ate the heart | Learned. A priest taught you |
| Currency | Dragon souls | Gold, from priests and tomes |
| Signature | Breath and transformation | Lightning |
| Catalyst | A Dragon Communion ring | A Gravel Stone ring |
| Skyrim hook | The dragon soul surplus | The Dragon Priests |

Two branches of one tree, and they are genuinely different playstyles rather than a reskin — one is close-range transformation, the other is ranged lightning. Skyrim's lightning vocabulary is three spells deep, so the Dragon Cult side is close to pure addition.

### The roster

| Spell | Family | What it does |
| --- | --- | --- |
| Dragonclaw | Communion | Transforms the arm; sweeping claw strike |
| Dragonmaw | Communion | Transforms into a dragon's maw; close-range bite |
| Dragonfire | Communion | Fire breath from above |
| Agheel's Flame | Communion | Wider, sweeping fire breath |
| Magma Breath | Communion | Magma spray that pools on the ground |
| Theodorix's Magma | Communion | Heavier magma, larger persistent pool |
| Glintstone Breath | Communion | Magic-damage breath |
| Smarag's Glintstone Breath | Communion | Stronger magic breath |
| Rotten Breath | Communion | Scarlet rot buildup in a cone |
| Ekzykes's Decay | Communion | Heavier rot, wider spread |
| Dragonice | Communion | Frost breath, frostbite buildup |
| Borealis's Mist | Communion | Lingering frost mist |
| Greyoll's Roar | Communion | Roar; damage plus a debuff on everything nearby |
| Ghostflame Breath | Communion (DLC) | Magic damage plus frostbite |
| Bayle's Tyranny | Communion (DLC) | Heatwave blast roar |
| Bayle's Flame Lightning | Communion (DLC) | Flame-lightning talon strike |
| Lightning Spear | Cult | The basic thrown bolt |
| Honed Bolt | Cult | Bolt that strikes from above |
| Lightning Strike | Cult | Bolt into the ground, arcing outward |
| Ancient Dragons' Lightning Spear | Cult | Red ancient-dragon bolt, far heavier |
| Ancient Dragons' Lightning Strike | Cult | Ground-slam version of the above |
| Frozen Lightning Spear | Cult | Lightning with frostbite buildup |
| Death Lightning | Cult | Lightning carrying death blight |
| Fortissax's Lightning Spear | Cult | Multi-strike bolt barrage |
| Vyke's Dragonbolt | Cult | Self-buff; madness-tinged lightning armament |
| Dragonbolt Blessing | Cult | Self-buff; lightning armament and resistance |
| Electrify Armament | Cult | Weapon buff, lightning damage |
| Lightning Fortification | Cult | Lightning damage negation |
| Knight's Lightning Spear | Cult (DLC) | Faster, knightly bolt variant |
| Electrocharge | Cult (DLC) | Charged area discharge |
| Dragonbolt of Florissax | Cult (DLC) | Heavier self-buff variant |
| **Placidusax's Ruin** | Unclassified | Transforms the caster into the two-headed Dragonlord; lightning burst then a steerable fire beam |

Placidusax's Ruin is technically neither family — it is a unique incantation from a remembrance and no seal passive boosts it. Keep it unclassified here too and make it **the tree's capstone**. It is the only spell in Elden Ring that turns the caster into a dragon rather than borrowing a part of one, it takes three memory slots and eight seconds to cast, and "become the Dragonlord" is the correct reward for emptying a Dragonborn's skill tree.

### The redundancy problem

Dragonfire and Fire Breath do the same thing. Dragonice and Frost Breath do the same thing. Greyoll's Roar overlaps several shouts. The magic tab's own rule says do not import spells that duplicate what Skyrim has, so either the rule bends or the best spells in the family get cut.

The rule holds, because they are only duplicates in fiction — their **economies are opposites**, and that is what the player actually experiences:

|  | Shout | Dragon incantation |
| --- | --- | --- |
| Cost | Free | Magicka |
| Gate | Cooldown | Resource |
| Setup | None | Catalyst in hand |
| Commitment | Short | Long, punishable |
| Scales | Only via Dragon tree perks | Catalyst quality and perks |
| Power ceiling | Fixed | High |

A shout is the thing you always have. An incantation is the thing you built for. Having both is not redundancy, it is the difference between a free ability on a timer and a committed cast that hits four times harder — and the Dragon tree is where a player invests to make both better at once. That is a coherent build, and it is the most Dragonborn build the game could offer.

### Scope check

Honestly: the spells and the perks pass the three-second test. **Dragon souls as currency does not** — that is a three-hour change. It stays in because it replaces an existing dead Skyrim system rather than adding a new one, and because it costs almost nothing to implement. Worth noting as a deliberate exception rather than pretending it passes.
