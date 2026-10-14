By hand, this is a four-to-six person modding team working for about two years, or one person working for five. That's not discouragement — it's the actual scale, and knowing it up front changes how you'd sequence the work. Here's the full pipeline as a solo human would run it.

A caveat on tool names: the Souls and Skyrim toolchains churn fast, and some of what I name below may have been superseded. Treat the names as "the tool that does X" and verify current versions before you install anything.

---

## Phase 0 — Workspace setup

1. Buy and install both games. Elden Ring (Steam) and Skyrim Special Edition specifically — not Legendary Edition, not VR.
2. Get access to a Windows x64 machine. Nearly every tool in this pipeline is Windows-only .NET or C++. A Windows VM or a cheap used desktop. The Mac is the *target*, not the workbench.
3. Install Visual Studio with the C++ desktop workload, CMake, and Python.
4. Create a project folder with subfolders: `extract_er/`, `extract_skyrim/`, `blender/`, `hkx_out/`, `plugin_src/`, `docs/`, `recordings/`.
5. Initialize a git repo covering only your own source and notes — never the extracted game assets.
6. Set up a backup routine. You will corrupt a behavior file at some point and not notice for three days.
7. Start a single spreadsheet workbook, `skyring_data.xlsx`. It becomes the backbone of the entire project. One sheet per param table you extract.

## Phase 1 — Pull the Elden Ring data

8. Run UXM Selective Unpacker against your ER install to unpack the game's archives into loose files. Note your exact game version and never patch it mid-project.
9. Orient yourself in the unpacked tree: `/chr` (characters), `/param` (all the numbers), `/action`, `/msg` (text), `/map`, `/sfx`.
10. Locate `c0000.anibnd.dcx` — the player character's animation bundle. Unpack it with WitchyBND (successor to Yabber). You now have hundreds of `.hkx` clips and the `.tae` files.
11. Install DS Anim Studio and open the TAE. Each animation ID has an event timeline attached.
12. **The transcription grind.** For every animation you care about — roll, backstep, light attack R1 chains per weapon class, heavy R2, jump attack, guard, parry, riposte, each spell cast — record into the spreadsheet: animation ID, total length in frames, invincibility event start/end frame, hitbox-on/hitbox-off frames, which hit ID (0–15) is live, stamina cost event, input-cancel window start. ER's TAE runs at 30 fps; divide by 30 for seconds.
13. Budget realistically: 40–80 animations fully transcribed, 15–30 minutes each once you're fluent. Two to three weeks of evenings.
14. Unpack `regulation.bin` with WitchyBND to get the `.param` files.
15. Open those in Smithbox (or DSMapStudio) with the correct paramdef for your game version.
16. Export each of these to CSV: `EquipParamWeapon`, `AtkParam_Pc`, `BehaviorParam_PC`, `CalcCorrectGraph`, `ReinforceParamWeapon`, `AttackElementCorrectParam`, `EquipParamProtector`, `SpEffectParam`, `Magic`, `Bullet`, `SwordArtsParam`, `PlayerCommonParam`, `LockCamParam`.
17. **The join.** This is the step nobody warns you about. An animation doesn't point at its damage values directly. Animation → Behavior ID → `BehaviorParam_PC` → `atkParamId` → `AtkParam_Pc` row, which holds the motion values, stamina cost, poise damage, and hit capsule radii. You do this join by hand in the spreadsheet with lookup formulas, per attack, and you check a sample of them manually because you will get the indirection wrong at least once.
18. `CalcCorrectGraph` is the one to understand deeply — it's the piecewise curve that produces every soft cap in the game. Plot each curve in the spreadsheet so you can see its shape.
19. **Measure what the params don't say.** Record ER gameplay at 60 fps with OBS. Step through frame by frame in a video editor and count: medium-roll i-frame count, stamina regen delay after an attack versus after a roll, roll distance in body-lengths, time from button press to hitbox active. Log these in a `docs/observed.md`.
20. Cross-check your measurements against your TAE transcription. Where they disagree, you misread the TAE — go back.

## Phase 2 — Pull the Skyrim data

21. Install SSE, SKSE64, and Address Library. Pin an exact game version and disable Steam auto-update.
22. Install Mod Organizer 2 and set up a profile. Never install anything directly into the game folder.
23. Extract the BSA archives with BAE or Cathedral Assets Optimizer.
24. Install the Creation Kit and SSEEdit (xEdit).
25. Pull the skeleton: `skeleton.nif` and `skeleton_female.nif`. Also install XPMSSE, since it's the de facto standard rig and adds bones you'll need.
26. Open `skeleton.nif` in NifSkope and write out the complete bone hierarchy and names into `docs/skyrim_rig.md`. Roughly 100+ bones. You need this list verbatim.
27. Extract the behavior graphs — `0_master.hkx`, `defaultmale.hkx`, `mt_behavior.hkx` — and convert them to readable XML with hkxconv.
28. Read the XML and map the combat state machine by hand into `docs/behavior_map.md`: every state, every transition, every event name, every animation variable. This is dense and boring and absolutely load-bearing.
29. In xEdit, dump record sets to CSV: `WEAP`, `ARMO`, `NPC_`, `SPEL`, `MGEF`, `PERK`, `RACE`, `SHOU`, `WOOP`, `GMST`.
30. Isolate every combat-relevant GMST into its own sheet — movement speeds, stagger magnitudes, carry weight formula, timescale. You'll be overriding a subset.
31. Install and study the reference mods, not to copy but to understand what's achievable: MCO/ADXP (attack chaining), Precision (capsule hit detection), Animation Motion Revolution (root motion), Valhalla Combat, True Directional Movement, Open Animation Replacer. Read their source where it's public.

## Phase 3 — Author the bridge documents

32. Pin down unit conversion. Skyrim's world unit is tiny — on the order of 70 units per meter. Verify empirically by measuring a known object in CK rather than trusting a wiki number. Write the constant into `docs/constants.md`.
33. Pin down time conversion: ER TAE at 30 fps, Skyrim animations exported at 30 fps but the engine ticks on variable framerate. Decide whether your windows are frame-based or time-based. Choose time-based.
34. Write the weapon mapping table by hand: every Skyrim `WEAP` record → an ER weapon class → an ER moveset. Hundreds of rows.
35. Write the armor mapping: every `ARMO` record → weight class and poise value.
36. Write the enemy mapping: every `NPC_` → behavior profile, stance threshold, stagger recovery time.
37. Write the hybrid damage formula on paper. Skyrim's scaling stays, ER motion values and `CalcCorrectGraph` curves come in. Decide exactly where they meet and what the resulting number ranges look like at level 1, 20, and 50. Sanity-check that a bandit doesn't one-shot you.
38. Design the rings-as-seals system: which spells, which ring slots, how scaling reads off the ring.
39. Design the Dragon Communion tree: node layout, prerequisites, which incantations sit where.
40. Write down explicitly what shouts do *not* interact with, since they're staying vanilla and independent of wielded items. This boundary will be tempting to violate later.

## Phase 4 — Animation retargeting

41. Install Blender with PyNifly (Skyrim NIF) and a Souls import addon such as Soulstruct (FLVER + HKX).
42. Import the ER `c0000` skeleton into Blender.
43. Import the Skyrim XPMSSE skeleton into Blender.
44. Build the bone correspondence map manually, bone by bone. Account for differing bone orientations, differing rest poses, and bones that exist on one rig and not the other.
45. Fix the rest-pose mismatch first — align both rigs to the same reference pose before any retargeting, or every clip will be subtly wrong in the same way.
46. Retarget one clip (use a roll; it's unambiguous) with constraints or a retarget addon. Fix foot sliding and hip drift by hand on the curves.
47. Extract root motion properly. ER bakes movement differently than Skyrim expects; you'll feed Skyrim's root motion through Animation Motion Revolution annotations.
48. Repeat for every clip. Expect 1–3 hours per clip for the first ten, then 20–40 minutes each once your process stabilizes.
49. Export each retargeted clip to FBX.
50. Convert FBX → Skyrim-compatible HKX using ck-cmd (which wraps the Havok Content Tools). SE needs 64-bit HKX.
51. Annotate each converted clip with hkanno64: motion data for AMR, hit-frame markers matching your TAE transcription, and Payload Interpreter calls where you need code to fire mid-animation.
52. Place clips into Open Animation Replacer folder structures with condition JSON — weapon type, stance, equip load bracket.
53. Load in game. The first one will be inside-out or scaled wrong. Debug, fix the conversion step, re-run the batch.

## Phase 5 — Behavior graph surgery

54. Decide between patching via Nemesis/Pandora versus hand-editing behavior XML. Patching is survivable; hand-editing gives you new states but breaks compatibility with everything.
55. Add new animation variables for your systems: stance value, equip load bracket, i-frame active, FP available.
56. Add new states and transitions for the attack chains, directional rolls, and casting poses that vanilla has no slot for.
57. Register new event names and wire them to the clips.
58. Re-run the behavior patcher after every change. Keep a known-good backup of the behavior folder.

## Phase 6 — Write the actual code

59. Set up a CommonLibSSE-NG project with CMake. Papyrus is far too slow for any of this; it's for UI and quest glue only.
60. Hook the actor update loop.
61. Implement the stamina model: new pool, ER regen rate, ER post-action delay, per-attack costs from your spreadsheet.
62. Implement poise and stance: accumulate poise damage per hit, threshold check, stagger, recovery timer, reset-on-idle.
63. Implement i-frames: read the annotation window at runtime and set the invulnerability flag. Do not hardcode frame numbers per animation unless you enjoy suffering.
64. Replace hit detection with capsule-based collision driven by annotations, using Precision's approach as the reference model.
65. Implement equip load brackets and select the roll animation accordingly.
66. Implement FP as a resource — either a new actor value or a repurposed one — plus cast speed scaling.
67. Hook the damage calculation and substitute your hybrid formula with ER motion values and the `CalcCorrectGraph` curves.
68. Implement status buildup: bleed, frost, rot, each with buildup rate and decay timer.
69. Rework lock-on: lock range, soft lock, target switching, camera follow speed.
70. Build the ESP in CK or xEdit: new perks for the ER-build skill tree options, new spell records, new magic effects, the Dragon Communion tree, ring-seal keywords.
71. Write Papyrus only for menus, quest hooks, and anything that can tolerate a 100ms delay.

## Phase 7 — Get it onto the M2 Pro

72. Set up CrossOver or Whisky, or Apple's Game Porting Toolkit, with DXVK and MoltenVK in the chain.
73. Verify base SSE runs and is stable before adding anything.
74. Verify SKSE64 loads under Wine — this is the real gate. DLL plugin loading under Wine has historically worked but is fragile.
75. Verify *your* SKSE plugin loads and its hooks fire.
76. Profile performance. Expect shader compilation stutter and fix it with a pre-cache pass.
77. Test save/load stability over long sessions. Wine plus heavy SKSE plugins is where silent save corruption lives.

## Phase 8 — Tune it until it feels right

78. Build a test cell in CK: flat ground, three dummy targets, one aggressive humanoid, one large enemy.
79. Record your build at 60 fps. Record Elden Ring at 60 fps doing the same action.
80. Put them side by side in a video editor and compare frame counts for: input-to-hitbox, i-frame window, recovery length, stagger duration.
81. Adjust, rebuild, re-record. Repeat per animation.
82. Keep a changelog of every tuning number you touch and why. Six weeks in you will not remember why stamina regen delay is 0.7 and not 0.6.

---

If you were doing this with no AI and no shortcuts, the order that keeps you sane is: **Phase 1 step 12, then Phase 4 steps 41–48, then stop and evaluate.** Transcribe exactly one roll and one light attack, retarget those two clips onto the Skyrim skeleton, and get them playing with correct i-frame and hitbox windows. That's maybe three weeks of work and it tells you whether the remaining two years are worth starting.

Two different extraction jobs, because you want opposite things from each: timing and numbers from Elden Ring, the world and its contents from Skyrim.

From Elden Ring — the “feel” data
Animation clips + their event timelines. The clips themselves matter less than the event track attached to each one: frame X the hitbox turns on, frame Y it turns off, frames A–B are invincible during a roll, frame C is the earliest you can cancel into something else. This is 80% of what makes ER combat feel like ER. Everything else is garnish.
Per-attack numbers. Motion value (damage multiplier for that specific swing), stamina cost, poise damage dealt, and which collision shape on the weapon is live during the swing.
Player state constants. Stamina pool and regen rate, the delay before regen restarts after an action, roll distance and speed per equip-load bracket, the equip-load thresholds themselves, poise thresholds and which attacks grant hyperarmor.
Weapon definitions. Weapon class → which moveset it plays, stat requirements, scaling grades plus the actual scaling curves behind the letters, upgrade progression, and weapon-art (Ash of War) definitions.
Damage math tables. Attack-rating curves with their soft caps, defense/negation percentages, and status buildup rates plus decay rate (bleed, frost, rot).
Enemy behavior data. Attack selection and spacing logic, aggro ranges, stance/stagger thresholds, stagger recovery timing, and how posture resets.
Lock-on and camera. Lock range, target switching, camera follow speed. Easy to overlook, and getting it wrong makes correct combat still feel wrong.
Magic data. Spell list with FP cost, cast speed, scaling stat, and effect; sacred seal mechanics specifically (since rings are replacing them); and Dragon Communion / Dragon Cult incantations as their own cluster.
Spirit Ash / summon data — costs, durations, summon AI — if summoning is making the cut.
From Skyrim — the “world” data
Skeleton and rig. Bone names, hierarchy, and proportions. Non-negotiable, because every ER animation has to be retargeted onto this skeleton. If it doesn’t exist here, the animation can’t play.
Cells, worldspaces, navmesh. The actual geography and where NPCs are allowed to walk.
Actor records. NPC stats, races, factions, AI packages, leveled lists — who exists and how they’re placed.
Items. Weapons, armor, ingredients, misc objects, with their records intact. You’ll need each weapon’s record to map it to an ER weapon class later.
Meshes, textures, archives. The look stays Skyrim, so all of this comes over untouched.
Magic records. Spells, magic effects, enchantments, and the shout definitions separately — shouts are staying vanilla, so you want them isolated and left alone.
Perk trees and leveling curves. You said Skyrim scaling largely stays, so these are the backbone the ER bits get grafted onto.
Quests, dialogue, Papyrus scripts. Everything that makes interacting with the world Skyrim.
Game settings (the GMST block). The global tuning constants — movement speeds, carry weight formula, timescale. You’ll be overriding a subset of these and need to know which.
The third pile nobody plans for: bridge data
Unit and scale conversion. The two engines measure distance and time differently (Skyrim’s world unit is tiny — roughly 70 per meter). Every range, hitbox size, and roll distance needs converting, and getting this wrong makes rolls feel either like teleports or like wading.
Mapping tables you author yourself. Skyrim weapon → ER weapon class/moveset. Skyrim armor → weight class and poise value. Skyrim enemy → ER behavior profile and stance threshold. These don’t exist in either game; they’re the actual design work.

One practical note: get Special Edition rather than Legendary. SE uses the 64-bit Havok format, LE uses 32-bit, and that single choice determines which animation toolchain you live in for the rest of the project.

The highest-leverage thing to pull first is the Elden Ring event timelines. Grab those, get one roll and one light attack playing on a Skyrim skeleton with correct i-frame and hitbox windows, and you’ll know within a week whether the whole thing is viable.

How would I have done this by hand (Explain it clearly, be exact and include every possible action and subtask) without any AI

By hand, this is a four-to-six person modding team working for about two years, or one person working for five. That’s not discouragement — it’s the actual scale, and knowing it up front changes how you’d sequence the work. Here’s the full pipeline as a solo human would run it.

A caveat on tool names: the Souls and Skyrim toolchains churn fast, and some of what I name below may have been superseded. Treat the names as “the tool that does X” and verify current versions before you install anything.

Phase 0 — Workspace setup
Buy and install both games. Elden Ring (Steam) and Skyrim Special Edition specifically — not Legendary Edition, not VR.
Get access to a Windows x64 machine. Nearly every tool in this pipeline is Windows-only .NET or C++. A Windows VM or a cheap used desktop. The Mac is the target, not the workbench.
Install Visual Studio with the C++ desktop workload, CMake, and Python.
Create a project folder with subfolders: extract_er/, extract_skyrim/, blender/, hkx_out/, plugin_src/, docs/, recordings/.
Initialize a git repo covering only your own source and notes — never the extracted game assets.
Set up a backup routine. You will corrupt a behavior file at some point and not notice for three days.
Start a single spreadsheet workbook, skyring_data.xlsx. It becomes the backbone of the entire project. One sheet per param table you extract.
Phase 1 — Pull the Elden Ring data
Run UXM Selective Unpacker against your ER install to unpack the game’s archives into loose files. Note your exact game version and never patch it mid-project.
Orient yourself in the unpacked tree: /chr (characters), /param (all the numbers), /action, /msg (text), /map, /sfx.
Locate c0000.anibnd.dcx — the player character’s animation bundle. Unpack it with WitchyBND (successor to Yabber). You now have hundreds of .hkx clips and the .tae files.
Install DS Anim Studio and open the TAE. Each animation ID has an event timeline attached.
The transcription grind. For every animation you care about — roll, backstep, light attack R1 chains per weapon class, heavy R2, jump attack, guard, parry, riposte, each spell cast — record into the spreadsheet: animation ID, total length in frames, invincibility event start/end frame, hitbox-on/hitbox-off frames, which hit ID (0–15) is live, stamina cost event, input-cancel window start. ER’s TAE runs at 30 fps; divide by 30 for seconds.
Budget realistically: 40–80 animations fully transcribed, 15–30 minutes each once you’re fluent. Two to three weeks of evenings.
Unpack regulation.bin with WitchyBND to get the .param files.
Open those in Smithbox (or DSMapStudio) with the correct paramdef for your game version.
Export each of these to CSV: EquipParamWeapon, AtkParam_Pc, BehaviorParam_PC, CalcCorrectGraph, ReinforceParamWeapon, AttackElementCorrectParam, EquipParamProtector, SpEffectParam, Magic, Bullet, SwordArtsParam, PlayerCommonParam, LockCamParam.
The join. This is the step nobody warns you about. An animation doesn’t point at its damage values directly. Animation → Behavior ID → BehaviorParam_PC → atkParamId → AtkParam_Pc row, which holds the motion values, stamina cost, poise damage, and hit capsule radii. You do this join by hand in the spreadsheet with lookup formulas, per attack, and you check a sample of them manually because you will get the indirection wrong at least once.
CalcCorrectGraph is the one to understand deeply — it’s the piecewise curve that produces every soft cap in the game. Plot each curve in the spreadsheet so you can see its shape.
Measure what the params don’t say. Record ER gameplay at 60 fps with OBS. Step through frame by frame in a video editor and count: medium-roll i-frame count, stamina regen delay after an attack versus after a roll, roll distance in body-lengths, time from button press to hitbox active. Log these in a docs/observed.md.
Cross-check your measurements against your TAE transcription. Where they disagree, you misread the TAE — go back.
Phase 2 — Pull the Skyrim data
Install SSE, SKSE64, and Address Library. Pin an exact game version and disable Steam auto-update.
Install Mod Organizer 2 and set up a profile. Never install anything directly into the game folder.
Extract the BSA archives with BAE or Cathedral Assets Optimizer.
Install the Creation Kit and SSEEdit (xEdit).
Pull the skeleton: skeleton.nif and skeleton_female.nif. Also install XPMSSE, since it’s the de facto standard rig and adds bones you’ll need.
Open skeleton.nif in NifSkope and write out the complete bone hierarchy and names into docs/skyrim_rig.md. Roughly 100+ bones. You need this list verbatim.
Extract the behavior graphs — 0_master.hkx, defaultmale.hkx, mt_behavior.hkx — and convert them to readable XML with hkxconv.
Read the XML and map the combat state machine by hand into docs/behavior_map.md: every state, every transition, every event name, every animation variable. This is dense and boring and absolutely load-bearing.
In xEdit, dump record sets to CSV: WEAP, ARMO, NPC_, SPEL, MGEF, PERK, RACE, SHOU, WOOP, GMST.
Isolate every combat-relevant GMST into its own sheet — movement speeds, stagger magnitudes, carry weight formula, timescale. You’ll be overriding a subset.
Install and study the reference mods, not to copy but to understand what’s achievable: MCO/ADXP (attack chaining), Precision (capsule hit detection), Animation Motion Revolution (root motion), Valhalla Combat, True Directional Movement, Open Animation Replacer. Read their source where it’s public.
Phase 3 — Author the bridge documents
Pin down unit conversion. Skyrim’s world unit is tiny — on the order of 70 units per meter. Verify empirically by measuring a known object in CK rather than trusting a wiki number. Write the constant into docs/constants.md.
Pin down time conversion: ER TAE at 30 fps, Skyrim animations exported at 30 fps but the engine ticks on variable framerate. Decide whether your windows are frame-based or time-based. Choose time-based.
Write the weapon mapping table by hand: every Skyrim WEAP record → an ER weapon class → an ER moveset. Hundreds of rows.
Write the armor mapping: every ARMO record → weight class and poise value.
Write the enemy mapping: every NPC_ → behavior profile, stance threshold, stagger recovery time.
Write the hybrid damage formula on paper. Skyrim’s scaling stays, ER motion values and CalcCorrectGraph curves come in. Decide exactly where they meet and what the resulting number ranges look like at level 1, 20, and 50. Sanity-check that a bandit doesn’t one-shot you.
Design the rings-as-seals system: which spells, which ring slots, how scaling reads off the ring.
Design the Dragon Communion tree: node layout, prerequisites, which incantations sit where.
Write down explicitly what shouts do not interact with, since they’re staying vanilla and independent of wielded items. This boundary will be tempting to violate later.
Phase 4 — Animation retargeting
Install Blender with PyNifly (Skyrim NIF) and a Souls import addon such as Soulstruct (FLVER + HKX).
Import the ER c0000 skeleton into Blender.
Import the Skyrim XPMSSE skeleton into Blender.
Build the bone correspondence map manually, bone by bone. Account for differing bone orientations, differing rest poses, and bones that exist on one rig and not the other.
Fix the rest-pose mismatch first — align both rigs to the same reference pose before any retargeting, or every clip will be subtly wrong in the same way.
Retarget one clip (use a roll; it’s unambiguous) with constraints or a retarget addon. Fix foot sliding and hip drift by hand on the curves.
Extract root motion properly. ER bakes movement differently than Skyrim expects; you’ll feed Skyrim’s root motion through Animation Motion Revolution annotations.
Repeat for every clip. Expect 1–3 hours per clip for the first ten, then 20–40 minutes each once your process stabilizes.
Export each retargeted clip to FBX.
Convert FBX → Skyrim-compatible HKX using ck-cmd (which wraps the Havok Content Tools). SE needs 64-bit HKX.
Annotate each converted clip with hkanno64: motion data for AMR, hit-frame markers matching your TAE transcription, and Payload Interpreter calls where you need code to fire mid-animation.
Place clips into Open Animation Replacer folder structures with condition JSON — weapon type, stance, equip load bracket.
Load in game. The first one will be inside-out or scaled wrong. Debug, fix the conversion step, re-run the batch.
Phase 5 — Behavior graph surgery
Decide between patching via Nemesis/Pandora versus hand-editing behavior XML. Patching is survivable; hand-editing gives you new states but breaks compatibility with everything.
Add new animation variables for your systems: stance value, equip load bracket, i-frame active, FP available.
Add new states and transitions for the attack chains, directional rolls, and casting poses that vanilla has no slot for.
Register new event names and wire them to the clips.
Re-run the behavior patcher after every change. Keep a known-good backup of the behavior folder.
Phase 6 — Write the actual code
Set up a CommonLibSSE-NG project with CMake. Papyrus is far too slow for any of this; it’s for UI and quest glue only.
Hook the actor update loop.
Implement the stamina model: new pool, ER regen rate, ER post-action delay, per-attack costs from your spreadsheet.
Implement poise and stance: accumulate poise damage per hit, threshold check, stagger, recovery timer, reset-on-idle.
Implement i-frames: read the annotation window at runtime and set the invulnerability flag. Do not hardcode frame numbers per animation unless you enjoy suffering.
Replace hit detection with capsule-based collision driven by annotations, using Precision’s approach as the reference model.
Implement equip load brackets and select the roll animation accordingly.
Implement FP as a resource — either a new actor value or a repurposed one — plus cast speed scaling.
Hook the damage calculation and substitute your hybrid formula with ER motion values and the CalcCorrectGraph curves.
Implement status buildup: bleed, frost, rot, each with buildup rate and decay timer.
Rework lock-on: lock range, soft lock, target switching, camera follow speed.
Build the ESP in CK or xEdit: new perks for the ER-build skill tree options, new spell records, new magic effects, the Dragon Communion tree, ring-seal keywords.
Write Papyrus only for menus, quest hooks, and anything that can tolerate a 100ms delay.
Phase 7 — Get it onto the M2 Pro
Set up CrossOver or Whisky, or Apple’s Game Porting Toolkit, with DXVK and MoltenVK in the chain.
Verify base SSE runs and is stable before adding anything.
Verify SKSE64 loads under Wine — this is the real gate. DLL plugin loading under Wine has historically worked but is fragile.
Verify your SKSE plugin loads and its hooks fire.
Profile performance. Expect shader compilation stutter and fix it with a pre-cache pass.
Test save/load stability over long sessions. Wine plus heavy SKSE plugins is where silent save corruption lives.
Phase 8 — Tune it until it feels right
Build a test cell in CK: flat ground, three dummy targets, one aggressive humanoid, one large enemy.
Record your build at 60 fps. Record Elden Ring at 60 fps doing the same action.
Put them side by side in a video editor and compare frame counts for: input-to-hitbox, i-frame window, recovery length, stagger duration.
Adjust, rebuild, re-record. Repeat per animation.
Keep a changelog of every tuning number you touch and why. Six weeks in you will not remember why stamina regen delay is 0.7 and not 0.6.

If you were doing this with no AI and no shortcuts, the order that keeps you sane is: Phase 1 step 12, then Phase 4 steps 41–48, then stop and evaluate. Transcribe exactly one roll and one light attack, retarget those two clips onto the Skyrim skeleton, and get them playing with correct i-frame and hitbox windows. That’s maybe three weeks of work and it tells you whether the remaining two years are worth starting.

Want me to turn this into a checklist document you can actually tick through?

how would you deploy the reverse engineer anything MCP server to solve this specific problem, theoretically? Sadly I can't run the Windows device until later on this week so for now we're just workshopping

Short version: REA covers maybe 15% of SkyRing — but it’s the 15% that’s genuinely hard to do by hand, and it’s the part that runs on your Mac with no Windows box involved. So it’s actually the right thing to workshop this week.

What REA actually is, and why the Mac constraint doesn’t bite

REA is a CLI plus MCP server that connects an agent to analysis tools for native binaries, JavaScript/Electron apps, .NET assemblies and websites, with results that carry the evidence and limitations behind each conclusion. Deep native analysis runs through Hopper, Ghidra or IDA as a provider, and everything is analyzed locally. Native targets come back as pseudocode, assembly, strings, symbols, calls and references. 
github
github

For your targets — SkyrimSE.exe and eldenring.exe, both Windows PE x64 — use Ghidra as the provider, not Hopper. Ghidra is Java, handles PE natively, and runs fine on Apple Silicon. Hopper’s PE support is the weak spot in an otherwise Mac-first tool. That one choice is what lets you do all of this from the M2 Pro while the Windows machine is still offline: static analysis of a PE doesn’t require a machine that can execute the PE.

Requirements are Node 22.x, 24.x or 26+ plus npm, and setup registers the MCP server with your agent and installs matching workflow instructions. 
github

Deployment on the Mac Mini
brew install --cask ghidra
npx rea-agents setup          # pick your agent(s), review, approve
npm install --global rea-agents

Point it at Ghidra rather than letting it offer Hopper. For a binary the size of SkyrimSE.exe, raise REA_GHIDRA_STARTUP_TIMEOUT_MS immediately — the initial auto-analysis pass on a ~50MB stripped PE is a long one, and the default deadline will fail out and make you think the setup is broken.

Then register the same stdio server with flint. REA is a plain local MCP server, so anything that speaks MCP over stdio can host it; if flint’s tool layer doesn’t do MCP yet, the CLI path (rea <command> --json) gives you the identical capabilities as subprocess calls, which is probably the faster integration for a swarm daemon you’re already mid-refactor on.

The six things you’d actually point it at

1. Skyrim’s damage pipeline. Ask REA to locate the function that applies weapon damage to an actor, then walk its callers and callees. This is exactly what you need to write the hybrid formula hook from Phase 6 — not “find a hook address from a list,” but understand what the function reads, in what order, and what it writes back. Address Library gives you an offset; REA gives you the semantics.

2. The actor value / stamina tick. Same approach. You’re replacing regen rate and post-action delay, and you need to know whether the game ticks stamina in the actor update or in a separate AV manager, because that determines where your hook goes and what else breaks.

3. HKX parsing code. This is the non-obvious high-value one. Havok’s format is only semi-documented in the modding community, and the authoritative spec is the parser compiled into the exe. Have REA find the HKX loader by string and cross-reference, then decompile it. Same technique against ER’s TAE loader. This turns “I hope hkxconv handles this correctly” into “I know what field that is.”

4. BND4/DCX container handling in eldenring.exe. Understanding the container layout from the code that reads it is cleaner than trusting a third-party unpacker’s assumptions, especially if a version bump changes something.

5. The ER stamina and i-frame mechanism. Params give you numbers; the exe gives you the algorithm that consumes them. How i-frames are actually set and cleared, whether stamina regen delay is a timer or a state flag — that’s code, not data, and it’s the difference between a port that has the right values and one that feels right.

6. SKSE64 under Wine, later. REA’s process-behavior capture records terminal output, interactions, exit and filesystem observations, and run comparisons on macOS. When you hit Phase 7 and SKSE won’t load under CrossOver, run the loader under capture and diff it against a known-good run. 
github

Do all of this statically and offline, never against a running Elden Ring process. Attach anything to live ER and you’re in anti-cheat territory, which is a different problem with worse consequences and no upside for your goals. The exe on disk tells you everything you need.

Structuring the agent loop so it doesn’t wander

Binary RE is where agents hallucinate most confidently, because pseudocode looks like an answer. Three guardrails:

One question per session, written down first. “Where is weapon damage applied to a target actor?” not “understand Skyrim’s combat.” REA keeps a session alive across calls, so the discipline has to come from the prompt.
Demand the evidence trail. REA returns findings with their evidence, and the agent uses those to ask follow-up questions or write an implementation. Make the output format mandatory: claim, address, the disassembly that supports it, and an explicit unknowns list. 
github
Verify by prediction. Before you touch anything, have the analysis predict a value you can check another way — a known offset, a constant that appears in a param, a string. If the prediction misses, the whole chain is suspect.
The flint integration warning

A dedicated re-analyst role in the swarm is the right shape, but Ling 3.0 Flash Fin will not do binary RE well. Decompiler pseudocode is dense, ambiguous, and full of compiler artifacts; a free fast model will produce fluent, wrong function summaries at volume and you’ll spend more time pruning than you saved. Use the swarm for the mechanical half — enumerating strings, batching cross-reference queries, building the function inventory — and route the actual interpretation to the stronger model you already planned to review with.

What REA will not touch

TAE transcription, Blender retargeting, behavior graph surgery, HKX conversion, and all the tuning. That’s still the bulk of the project and it’s still Windows-and-hands work. REA doesn’t shrink Phase 1 step 12 or Phase 4 at all.

This week, before the Windows box

Install Ghidra and REA, copy SkyrimSE.exe over, let auto-analysis finish, and run exactly one investigation: find the damage application function and produce an evidence document for it. If that comes back clean and verifiable, you have a working RE pipeline on the Mac and a reason to trust the rest. If it comes back as plausible-sounding mush, you’ve learned that cheaply, and in a week where you couldn’t have done the Windows work anyway