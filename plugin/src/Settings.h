#pragma once

#include "InputPlan.h"
#include "er_combat.h"

// Data/SKSE/Plugins/ERCombat.ini. Read once at startup.
struct Settings
{
    // DirectInput scan codes for keyboard keys; mouse buttons are 256 + n
    // (256 left, 257 right, 258 middle), the same scheme SkyUI's MCM uses.
    // 0 leaves a control unbound.
    std::uint32_t dodgeKey = 0x38;      // Left Alt: tap to roll, hold to sprint
    std::uint32_t jumpKey = 0x39;       // Space
    std::uint32_t lightKey = 256;       // Left mouse
    std::uint32_t guardKey = 257;       // Right mouse: guard, or left-hand attack
    std::uint32_t heavyKey = 0;         // Optional dedicated heavy attack key
    std::uint32_t heavyModifier = 0x2A; // Left Shift + light = heavy
    std::uint32_t walkKey = 0x3A;       // Caps Lock

    // Gamepad buttons as the raw mask Skyrim reports (decimal in the ini):
    // A 4096, B 8192, X 16384, Y 32768, LB 256, RB 512, left stick click 64,
    // left trigger 9, right trigger 10. Defaults follow Elden Ring's layout.
    // They are unverified in-game until the first runtime session.
    std::uint32_t dodgePad = 8192;      // B
    std::uint32_t jumpPad = 4096;       // A
    std::uint32_t lightPad = 512;       // RB
    std::uint32_t heavyPad = 10;        // RT
    std::uint32_t guardPad = 256;       // LB
    std::uint32_t heavyModifierPad = 0;
    std::uint32_t walkPad = 0;

    InputPlan::Bindings InputBindings() const;

    // Skyrim animation event sent when each kind of action starts. Empty sends
    // nothing. Directional actions try "<event><Dir>" first (Forward, Back,
    // Left, Right), e.g. TK Dodge's TKDodgeForward.
    std::array<std::string, ER_ACTION_COUNT> events{};

    // Let Precision, MCO and the rest swing the weapon (phase A), or let the
    // core's own hit detection decide what was hit (phase B).
    bool coreHitDetection = false;

    static Settings& Get();
    void Load();
};
