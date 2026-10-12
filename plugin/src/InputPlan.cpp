#include "InputPlan.h"

#include <cmath>

namespace InputPlan
{
    std::uint32_t Code(Device device, std::uint32_t rawCode)
    {
        switch (device) {
        case Device::Keyboard:
            return rawCode;  // scan code 0 is not a key: it stays unbound
        case Device::Mouse:
            return kMouseBase + rawCode;
        case Device::Gamepad:
            return rawCode == 0 ? kUnbound : kGamepadBase + rawCode;
        }
        return kUnbound;
    }

    Action Resolve(const Bindings& bindings, std::uint32_t code)
    {
        if (code == kUnbound) {
            return Action::None;
        }
        const auto matches = [code](const Slot& slot) { return slot.primary == code || slot.pad == code; };
        if (matches(bindings.dodge)) {
            return Action::Dodge;
        }
        if (matches(bindings.jump)) {
            return Action::Jump;
        }
        if (matches(bindings.heavyModifier)) {
            return Action::HeavyModifier;
        }
        if (matches(bindings.walk)) {
            return Action::Walk;
        }
        if (matches(bindings.heavy)) {
            return Action::Heavy;
        }
        if (matches(bindings.light)) {
            return Action::Light;
        }
        if (matches(bindings.guard)) {
            return Action::Guard;
        }
        return Action::None;
    }

    void Controls::Edge::Set(bool down)
    {
        if (down && !held) {
            pressed = true;
        }
        if (!down && held) {
            released = true;
        }
        held = down;
    }

    ErButton Controls::Edge::Take()
    {
        const ErButton button{
            static_cast<std::uint8_t>(held),
            static_cast<std::uint8_t>(pressed),
            static_cast<std::uint8_t>(released),
        };
        pressed = released = false;
        return button;
    }

    void Controls::Button(std::uint32_t code, bool down, bool firstDown)
    {
        switch (Resolve(bindings, code)) {
        case Action::Dodge:
            dodge.Set(down);
            break;
        case Action::Jump:
            jump.Set(down);
            break;
        case Action::HeavyModifier:
            heavyModifierHeld = down;
            break;
        case Action::Walk:
            if (firstDown) {
                walk = !walk;
            }
            break;
        case Action::Heavy:
            heavy.Set(down);
            break;
        case Action::Light:
            // Modifier + light is the heavy attack, as in Elden Ring on a
            // keyboard. The choice is made when the press starts and holds
            // until that same press is released, whatever the modifier does.
            if (down && !light.held && !heavyFromLight && heavyModifierHeld) {
                heavyFromLight = true;
            }
            if (heavyFromLight) {
                heavy.Set(down);
                if (!down) {
                    heavyFromLight = false;
                }
            } else {
                light.Set(down);
            }
            break;
        case Action::Guard:
            guard.Set(down);
            break;
        case Action::None:
            break;
        }
    }

    void Controls::Move(Direction direction, bool down)
    {
        switch (direction) {
        case Direction::Forward:
            forward = down;
            break;
        case Direction::Back:
            back = down;
            break;
        case Direction::Left:
            left = down;
            break;
        case Direction::Right:
            right = down;
            break;
        }
    }

    void Controls::Stick(float x, float y)
    {
        // A non-finite axis would poison the core's heading and position.
        stickX = std::isfinite(x) ? x : 0.0f;
        stickY = std::isfinite(y) ? y : 0.0f;
    }

    ErInput Controls::Take(float cameraHeading)
    {
        ErInput out{};
        // Keyboard movement wins over a resting stick.
        float x = static_cast<float>(right) - static_cast<float>(left);
        float y = static_cast<float>(forward) - static_cast<float>(back);
        if (x == 0.0f && y == 0.0f) {
            x = stickX;
            y = stickY;
        }
        const float length = std::sqrt(x * x + y * y);
        if (length > 1.0f) {
            x /= length;
            y /= length;
        }
        // Walking: a light tilt.
        if (walk && length > 0.0f) {
            x *= 0.4f;
            y *= 0.4f;
        }
        out.move_x = x;
        out.move_y = y;
        out.camera_heading = cameraHeading;
        out.dodge = dodge.Take();
        out.jump = jump.Take();
        out.light = light.Take();
        out.heavy = heavy.Take();
        out.guard = guard.Take();
        out.walk = walk;
        return out;
    }
}
