#include "Settings.h"

#include <Windows.h>

namespace
{
    constexpr auto kPath = L"Data/SKSE/Plugins/ERCombat.ini";

    std::uint32_t ReadKey(const wchar_t* key, std::uint32_t fallback)
    {
        return static_cast<std::uint32_t>(GetPrivateProfileIntW(L"Keys", key, static_cast<int>(fallback), kPath));
    }

    std::string ReadEvent(const wchar_t* key, const char* fallback)
    {
        wchar_t buffer[128]{};
        std::wstring wideFallback(fallback, fallback + std::strlen(fallback));
        GetPrivateProfileStringW(L"Events", key, wideFallback.c_str(), buffer, 128, kPath);
        std::string out;
        for (const wchar_t* c = buffer; *c; ++c) {
            out.push_back(static_cast<char>(*c));
        }
        return out;
    }
}

Settings& Settings::Get()
{
    static Settings settings;
    return settings;
}

void Settings::Load()
{
    dodgeKey = ReadKey(L"Dodge", dodgeKey);
    jumpKey = ReadKey(L"Jump", jumpKey);
    lightKey = ReadKey(L"Light", lightKey);
    guardKey = ReadKey(L"Guard", guardKey);
    heavyKey = ReadKey(L"Heavy", heavyKey);
    heavyModifier = ReadKey(L"HeavyModifier", heavyModifier);
    walkKey = ReadKey(L"Walk", walkKey);

    dodgePad = ReadKey(L"GamepadDodge", dodgePad);
    jumpPad = ReadKey(L"GamepadJump", jumpPad);
    lightPad = ReadKey(L"GamepadLight", lightPad);
    heavyPad = ReadKey(L"GamepadHeavy", heavyPad);
    guardPad = ReadKey(L"GamepadGuard", guardPad);
    heavyModifierPad = ReadKey(L"GamepadHeavyModifier", heavyModifierPad);
    walkPad = ReadKey(L"GamepadWalk", walkPad);

    // Defaults drive the animations Gate to Sovngarde already has: TK Dodge
    // for rolls, and the vanilla attack events that MCO/ADXP and BFCO answer.
    events[ER_ACTION_ROLL] = ReadEvent(L"Roll", "TKDodge");
    events[ER_ACTION_CROUCH_ROLL] = ReadEvent(L"CrouchRoll", "TKDodge");
    events[ER_ACTION_BACKSTEP] = ReadEvent(L"Backstep", "TKDodgeBack");
    events[ER_ACTION_JUMP] = ReadEvent(L"Jump", "JumpStandingStart");
    events[ER_ACTION_LIGHT_ATTACK] = ReadEvent(L"LightAttack", "attackStart");
    events[ER_ACTION_HEAVY_CHARGE] = ReadEvent(L"HeavyCharge", "attackPowerStartInPlace");
    events[ER_ACTION_RUN_ATTACK] = ReadEvent(L"RunAttack", "attackStartSprint");
    events[ER_ACTION_ROLL_ATTACK] = ReadEvent(L"RollAttack", "attackStart");
    events[ER_ACTION_BACKSTEP_ATTACK] = ReadEvent(L"BackstepAttack", "attackStart");
    events[ER_ACTION_CROUCH_ATTACK] = ReadEvent(L"CrouchAttack", "attackStart");
    events[ER_ACTION_GUARD_COUNTER] = ReadEvent(L"GuardCounter", "attackPowerStartInPlace");
    events[ER_ACTION_LEFT_ATTACK] = ReadEvent(L"LeftAttack", "attackStartLeftHand");
    events[ER_ACTION_HURT] = ReadEvent(L"Hurt", "");

    coreHitDetection = GetPrivateProfileIntW(L"General", L"CoreHitDetection", 0, kPath) != 0;
}

InputPlan::Bindings Settings::InputBindings() const
{
    using InputPlan::Code;
    using InputPlan::Device;
    // Keyboard and mouse share one numbering (mouse = 256 + n), so the ini
    // value is already the device-tagged code.
    const auto pad = [](std::uint32_t raw) { return Code(Device::Gamepad, raw); };
    InputPlan::Bindings bindings;
    bindings.dodge = { dodgeKey, pad(dodgePad) };
    bindings.jump = { jumpKey, pad(jumpPad) };
    bindings.light = { lightKey, pad(lightPad) };
    bindings.heavy = { heavyKey, pad(heavyPad) };
    bindings.guard = { guardKey, pad(guardPad) };
    bindings.heavyModifier = { heavyModifier, pad(heavyModifierPad) };
    bindings.walk = { walkKey, pad(walkPad) };
    return bindings;
}
