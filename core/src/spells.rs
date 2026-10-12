//! The first magic slice: three catalysts and seven spells.
//!
//! Everything here is data composed on top of [`crate::magic`] — no new
//! mechanics. Slice D of `docs/architecture.md` names the seven cases: direct
//! damage, a projectile, a persistent area, a buff, a debuff, a status, and a
//! charged cast. Each spell below is one case, and each carries its own
//! [`SpellFamily`] so passives have something unambiguous to key on.
//!
//! The numbers are fractions and ratios, never magnitudes: a `power_ratio` is
//! a fraction of the casting catalyst's scaling, a `cost_fraction` is a
//! fraction of the caster's maximum magicka. Skyrim owns absolute damage,
//! health and the magicka pool, so no Elden Ring absolute number appears in
//! this file — Elden Ring supplies only the ordering and relative gaps
//! between catalysts.
//!
//! Nothing here carries a skill, level, perk or attribute requirement.
//! `castable_from` is a catalyst *family*, acquisition is the only access
//! gate, and skill 0 casts everything at full eligibility (Charter).

use crate::consequences::{DamageElement, ElementalProfile};
use crate::effects::{ModifiedStat, ModifierOperation, StackRule, STATUS_COUNT};
use crate::magic::{
    Catalyst, CatalystKind, Charge, ModifierTarget, ModifierTemplate, Passive, School, SpellDefinition,
    SpellDelivery, SpellFamily, SpellPayload, FROM_RING, FROM_STAFF,
};

// ---------------------------------------------------------------------------
// Spell families
// ---------------------------------------------------------------------------

/// One family per slice spell, so a passive boosts exactly one spell and the
/// roster shows the build structure families exist to create.
pub const FAMILY_EMBER: SpellFamily = SpellFamily(1);
pub const FAMILY_CINDER: SpellFamily = SpellFamily(2);
pub const FAMILY_PYRE: SpellFamily = SpellFamily(3);
pub const FAMILY_HASTE: SpellFamily = SpellFamily(4);
pub const FAMILY_TERROR: SpellFamily = SpellFamily(5);
pub const FAMILY_RIME: SpellFamily = SpellFamily(6);
pub const FAMILY_SUNLIGHT: SpellFamily = SpellFamily(7);

/// The quality of the high-scaling staff, expressed the only way the Charter
/// allows: as a ratio. Elden Ring's strongest destruction staff scales at 413
/// where the academy baseline scales at 345 — a relative gap of about 1.2.
/// The two absolute values are used for nothing else and appear nowhere as
/// damage, health or FP.
pub const HIGH_STAFF_QUALITY: f32 = 413.0 / 345.0;

// ---------------------------------------------------------------------------
// Catalysts
// ---------------------------------------------------------------------------

/// The baseline Destruction staff: the reference catalyst. Quality 1.0, no
/// passive, no cost trade — every other catalyst is expressed relative to it.
pub fn apprentice_staff() -> Catalyst {
    Catalyst {
        id: 1,
        kind: CatalystKind::Staff,
        school: School::Destruction,
        quality: 1.0,
        upgrade_level: 0,
        passive: None,
        cost_multiplier: 1.0,
    }
}

/// The high-scaling Destruction staff: the same family at about 1.2 times the
/// baseline quality, paid for with a 1.5× magicka cost trade. Best scaling on
/// the axis, and it costs you for every spell, exactly as a Lusat-shaped
/// catalyst should.
pub fn sages_staff() -> Catalyst {
    Catalyst {
        id: 2,
        kind: CatalystKind::Staff,
        school: School::Destruction,
        quality: HIGH_STAFF_QUALITY,
        upgrade_level: 0,
        passive: None,
        cost_multiplier: 1.5,
    }
}

/// The Restoration ring: worn, not held, and casting with it dedicates a hand
/// exactly as a staff does. One passive, on its own family: sunlight spells
/// cast from it (or with it in the off hand) are 25% stronger, and it does
/// nothing else.
pub fn sun_ring() -> Catalyst {
    Catalyst {
        id: 3,
        kind: CatalystKind::Ring,
        school: School::Restoration,
        quality: 1.0,
        upgrade_level: 0,
        passive: Some(Passive { family: FAMILY_SUNLIGHT, bonus: 0.25 }),
        cost_multiplier: 1.0,
    }
}

/// Every catalyst of the slice, in roster order.
pub fn all_catalysts() -> [Catalyst; 3] {
    [apprentice_staff(), sages_staff(), sun_ring()]
}

// ---------------------------------------------------------------------------
// The seven spells
// ---------------------------------------------------------------------------

fn full(element: DamageElement) -> ElementalProfile {
    ElementalProfile::from_channels(&[(element, 1.0)])
        .expect("a single element at full share is a valid profile")
}

fn timing(release_at: f32, cancel_until: f32, recovery: f32) -> crate::magic::CastTiming {
    crate::magic::CastTiming {
        release_at,
        cancel_until,
        recovery,
        // Hyperarmour covers the later windup through the release frame: the
        // cast is punishable early, armoured once committed.
        hyperarmour_from: release_at * 0.4,
        hyperarmour_to: release_at,
    }
}

#[allow(clippy::too_many_arguments)]
fn definition(
    id: u32,
    school: School,
    family: SpellFamily,
    delivery: SpellDelivery,
    castable_from: u8,
    power_ratio: f32,
    cost_fraction: f32,
    timing: crate::magic::CastTiming,
    payload: SpellPayload,
) -> SpellDefinition {
    SpellDefinition {
        id,
        school,
        family,
        delivery,
        castable_from,
        power_ratio,
        cost_fraction,
        timing,
        charge: None,
        area: None,
        payload,
    }
}

/// Case 1 — direct damage. A short committed bolt that resolves at release
/// against the target Skyrim selects. The cheapest hit in the slice.
pub fn ember_bolt() -> SpellDefinition {
    definition(
        101,
        School::Destruction,
        FAMILY_EMBER,
        SpellDelivery::Instant,
        FROM_STAFF,
        0.7,
        0.06,
        timing(0.45, 0.25, 0.35),
        SpellPayload {
            elements: full(DamageElement::Fire),
            poise_pressure: 15.0,
            stance_pressure: 20.0,
            status_buildup: [0.0; STATUS_COUNT],
            modifiers: Vec::new(),
        },
    )
}

/// Case 2 — projectile. Skyrim launches and flies it; a confirmed native
/// impact is the only thing that resolves it. Staff-only: this is the
/// Destruction shape staves own.
pub fn cinder_lance() -> SpellDefinition {
    definition(
        102,
        School::Destruction,
        FAMILY_CINDER,
        SpellDelivery::Projectile,
        FROM_STAFF,
        0.85,
        0.12,
        timing(0.7, 0.4, 0.5),
        SpellPayload {
            elements: full(DamageElement::Fire),
            poise_pressure: 25.0,
            stance_pressure: 35.0,
            status_buildup: [0.0; STATUS_COUNT],
            modifiers: Vec::new(),
        },
    )
}

/// Case 3 — persistent area. Four ticks over six seconds inside a three-metre
/// pillar of flame. Destruction straddles both catalyst families (Charter):
/// this is the Destruction spell a ring can carry, alongside the staff-only
/// bolt and lance above.
pub fn pyre_pillar() -> SpellDefinition {
    let mut spell = definition(
        103,
        School::Destruction,
        FAMILY_PYRE,
        SpellDelivery::Area,
        FROM_STAFF | FROM_RING,
        0.35,
        0.15,
        timing(0.8, 0.45, 0.7),
        SpellPayload {
            elements: full(DamageElement::Fire),
            poise_pressure: 20.0,
            stance_pressure: 30.0,
            status_buildup: [0.0; STATUS_COUNT],
            modifiers: Vec::new(),
        },
    );
    spell.area = Some(crate::magic::Area { radius: 3.0, duration: 6.0, tick_interval: 1.5 });
    spell
}

/// Case 4 — buff. A self-cast that lands on the caster at release: +25%
/// damage dealt for eight seconds. No impact exists; the snapshot's
/// caster modifiers are the whole effect.
pub fn haste() -> SpellDefinition {
    definition(
        104,
        School::Alteration,
        FAMILY_HASTE,
        SpellDelivery::SelfCast,
        FROM_STAFF,
        0.0,
        0.05,
        timing(0.5, 0.3, 0.4),
        SpellPayload {
            elements: ElementalProfile::default(),
            poise_pressure: 0.0,
            stance_pressure: 0.0,
            status_buildup: [0.0; STATUS_COUNT],
            modifiers: vec![ModifierTemplate {
                on: ModifierTarget::Caster,
                group: 1,
                stat: ModifiedStat::DamageDealt,
                operation: ModifierOperation::Multiply,
                magnitude: 1.25,
                duration: 8.0,
                priority: 0,
                stack_rule: StackRule::Replace,
            }],
        },
    )
}

/// Case 5 — debuff. A pure hex: no damage of its own, one target modifier
/// that raises damage taken by 25% for six seconds. Whatever the target
/// resists, the hex stacks by its stated rule, never by accident.
pub fn feeble_hex() -> SpellDefinition {
    definition(
        105,
        School::Illusion,
        FAMILY_TERROR,
        SpellDelivery::Instant,
        FROM_STAFF,
        0.0,
        0.08,
        timing(0.6, 0.35, 0.4),
        SpellPayload {
            elements: ElementalProfile::default(),
            poise_pressure: 0.0,
            stance_pressure: 0.0,
            status_buildup: [0.0; STATUS_COUNT],
            modifiers: vec![ModifierTemplate {
                on: ModifierTarget::Target,
                group: 2,
                stat: ModifiedStat::DamageTaken,
                operation: ModifierOperation::Multiply,
                magnitude: 1.25,
                duration: 6.0,
                priority: 0,
                stack_rule: StackRule::Strongest,
            }],
        },
    )
}

/// Case 6 — status. A frost sigil that carries only Frostbite buildup: no
/// damage multiplier at all, so the status channel is provably independent of
/// the damage channel.
pub fn rime_sigil() -> SpellDefinition {
    definition(
        106,
        School::Destruction,
        FAMILY_RIME,
        SpellDelivery::Projectile,
        FROM_STAFF,
        0.0,
        0.1,
        timing(0.65, 0.4, 0.45),
        SpellPayload {
            elements: ElementalProfile::default(),
            poise_pressure: 5.0,
            stance_pressure: 10.0,
            status_buildup: {
                let mut buildup = [0.0; STATUS_COUNT];
                buildup[crate::effects::StatusKind::Frostbite.index()] = 45.0;
                buildup
            },
            modifiers: Vec::new(),
        },
    )
}

/// Case 7 — charged cast. A sunlight lance held past its release frame up to
/// one second, reaching 1.5× power at full charge. Ring-cast: this is the
/// Restoration shape, and it is the spell the sun ring's passive boosts.
pub fn solar_lance() -> SpellDefinition {
    let mut spell = definition(
        107,
        School::Restoration,
        FAMILY_SUNLIGHT,
        SpellDelivery::Projectile,
        FROM_RING,
        1.0,
        0.18,
        timing(0.9, 0.5, 0.8),
        SpellPayload {
            elements: full(DamageElement::Holy),
            poise_pressure: 35.0,
            stance_pressure: 50.0,
            status_buildup: [0.0; STATUS_COUNT],
            modifiers: Vec::new(),
        },
    );
    spell.charge = Some(Charge { max_seconds: 1.0, power_at_full: 1.5 });
    spell
}

/// Every spell of the slice, in the slice's own order: direct damage,
/// projectile, persistent area, buff, debuff, status, charged cast.
pub fn all_spells() -> [SpellDefinition; 7] {
    [
        ember_bolt(),
        cinder_lance(),
        pyre_pillar(),
        haste(),
        feeble_hex(),
        rime_sigil(),
        solar_lance(),
    ]
}
