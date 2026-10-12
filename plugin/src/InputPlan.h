#pragma once

#include "er_combat.h"

#include <cstdint>

// Device-independent input rules. Everything here is plain data and logic so
// the same edge semantics can be tested without a Skyrim process: the
// CommonLib adapter in Input.cpp only translates engine events into calls.
namespace InputPlan
{
    enum class Device
    {
        Keyboard,
        Mouse,
        Gamepad,
    };

    // One number per physical control, whatever device it is on. Keyboard keys
    // are DirectInput scan codes; mouse buttons are 256 + n (SkyUI's scheme);
    // gamepad buttons are 1024 + the mask Skyrim reports. Zero means unbound.
    constexpr std::uint32_t kUnbound = 0;
    constexpr std::uint32_t kMouseBase = 256;
    constexpr std::uint32_t kGamepadBase = 1024;
    std::uint32_t Code(Device device, std::uint32_t rawCode);

    enum class Action
    {
        None,
        Dodge,
        Jump,
        Light,
        Heavy,
        Guard,
        HeavyModifier,
        Walk,
    };

    // Each action may be bound once on keyboard/mouse and once on a gamepad.
    struct Slot
    {
        std::uint32_t primary = kUnbound;
        std::uint32_t pad = kUnbound;
    };

    struct Bindings
    {
        Slot dodge, jump, light, heavy, guard, heavyModifier, walk;
    };

    // Which action a control triggers. When two actions share a control the
    // first in this order wins (dodge, jump, heavy modifier, walk, heavy,
    // light, guard), so a conflicting remap is deterministic, not racy.
    Action Resolve(const Bindings& bindings, std::uint32_t code);

    enum class Direction
    {
        Forward,
        Back,
        Left,
        Right,
    };

    // Turns a stream of control events into one ErInput per frame.
    class Controls
    {
    public:
        void SetBindings(const Bindings& value) { bindings = value; }

        // `down` is true while the control is held, including auto-repeat;
        // `firstDown` is true only on the event that put it down. Only
        // transitions in `down` create pressed/released edges, so a repeating
        // key cannot fire an action twice.
        void Button(std::uint32_t code, bool down, bool firstDown);
        void Move(Direction direction, bool down);
        void Stick(float x, float y);

        // The controls since the last call. Clears the one-frame edges and
        // keeps held state.
        ErInput Take(float cameraHeading);

        bool Walking() const { return walk; }

    private:
        struct Edge
        {
            bool held = false;
            bool pressed = false;
            bool released = false;

            void Set(bool down);
            ErButton Take();
        };

        Bindings bindings{};
        Edge dodge, jump, light, heavy, guard;
        bool heavyModifierHeld = false;
        bool heavyFromLight = false;
        bool walk = false;
        bool forward = false, back = false, left = false, right = false;
        float stickX = 0.0f, stickY = 0.0f;
    };
}
