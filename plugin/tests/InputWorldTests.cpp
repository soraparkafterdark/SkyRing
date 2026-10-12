// Host tests for the device-independent input rules and the camera heading,
// ground, and lock-on facts that cross the ABI. No Skyrim process involved.
#include "BridgePlan.h"
#include "InputPlan.h"

#include <array>
#include <cassert>
#include <cmath>
#include <limits>

namespace
{
    constexpr float kPi = 3.14159265358979323846f;
    constexpr float kNaN = std::numeric_limits<float>::quiet_NaN();

    bool Near(float actual, float expected, float tolerance = 1e-5f) { return std::abs(actual - expected) < tolerance; }

    using namespace InputPlan;

    // The same logical controls on three devices. Only the codes differ.
    Bindings KeyboardBindings()
    {
        Bindings b;
        b.dodge.primary = Code(Device::Keyboard, 0x38);
        b.jump.primary = Code(Device::Keyboard, 0x39);
        b.light.primary = Code(Device::Mouse, 0);
        b.guard.primary = Code(Device::Mouse, 1);
        b.heavy.primary = Code(Device::Keyboard, 0x13);
        b.heavyModifier.primary = Code(Device::Keyboard, 0x2A);
        b.walk.primary = Code(Device::Keyboard, 0x3A);
        return b;
    }

    Bindings AllDevices()
    {
        Bindings b = KeyboardBindings();
        b.dodge.pad = Code(Device::Gamepad, 0x2000);
        b.jump.pad = Code(Device::Gamepad, 0x1000);
        b.light.pad = Code(Device::Gamepad, 0x0200);
        b.guard.pad = Code(Device::Gamepad, 0x0100);
        b.heavy.pad = Code(Device::Gamepad, 0x000A);
        b.walk.pad = Code(Device::Gamepad, 0x0040);
        return b;
    }

    struct Pad
    {
        std::uint32_t dodge, jump, light, guard, heavy, walk;
    };
    constexpr Pad kKeyboard{ 0x38, 0x39, 256, 257, 0x13, 0x3A };
    constexpr Pad kGamepad{
        1024 + 0x2000, 1024 + 0x1000, 1024 + 0x0200, 1024 + 0x0100, 1024 + 0x000A, 1024 + 0x0040
    };

    bool SameButton(const ErButton& a, const ErButton& b)
    {
        return a.held == b.held && a.pressed == b.pressed && a.released == b.released;
    }

    bool SameInput(const ErInput& a, const ErInput& b)
    {
        return SameButton(a.dodge, b.dodge) && SameButton(a.jump, b.jump) && SameButton(a.light, b.light) &&
               SameButton(a.heavy, b.heavy) && SameButton(a.guard, b.guard) && a.walk == b.walk &&
               a.move_x == b.move_x && a.move_y == b.move_y && a.camera_heading == b.camera_heading;
    }

    // Replays one scripted session through whichever device's codes it is given.
    std::array<ErInput, 8> Session(const Pad& pad)
    {
        Controls controls;
        controls.SetBindings(AllDevices());
        std::array<ErInput, 8> frames{};
        // frame 0: dodge pressed, held through auto-repeat
        controls.Button(pad.dodge, true, true);
        controls.Button(pad.dodge, true, false);
        frames[0] = controls.Take(0.5f);
        // frame 1: still held, key repeating: no new edge
        controls.Button(pad.dodge, true, false);
        frames[1] = controls.Take(0.5f);
        // frame 2: dodge released; jump tapped inside one frame
        controls.Button(pad.dodge, false, false);
        controls.Button(pad.jump, true, true);
        controls.Button(pad.jump, false, false);
        frames[2] = controls.Take(0.5f);
        // frame 3: light pressed
        controls.Button(pad.light, true, true);
        frames[3] = controls.Take(0.5f);
        // frame 4: light released, guard pressed
        controls.Button(pad.light, false, false);
        controls.Button(pad.guard, true, true);
        frames[4] = controls.Take(0.5f);
        // frame 5: dedicated heavy pressed while guarding
        controls.Button(pad.heavy, true, true);
        frames[5] = controls.Take(0.5f);
        // frame 6: both released
        controls.Button(pad.heavy, false, false);
        controls.Button(pad.guard, false, false);
        frames[6] = controls.Take(0.5f);
        // frame 7: walk toggled once, even though the event repeats
        controls.Button(pad.walk, true, true);
        controls.Button(pad.walk, true, false);
        controls.Button(pad.walk, false, false);
        frames[7] = controls.Take(0.5f);
        return frames;
    }
}

int main()
{
    // --- codes and bindings --------------------------------------------------
    assert(Code(Device::Keyboard, 0x38) == 0x38);
    assert(Code(Device::Keyboard, 0) == kUnbound);
    assert(Code(Device::Mouse, 0) == 256);
    assert(Code(Device::Gamepad, 0x1000) == 1024 + 0x1000);
    assert(Code(Device::Gamepad, 0) == kUnbound);
    // Keyboard, mouse and gamepad codes never collide.
    assert(Code(Device::Mouse, 7) < kGamepadBase);
    assert(Code(Device::Keyboard, 255) < kMouseBase);

    const Bindings all = AllDevices();
    assert(Resolve(all, kKeyboard.dodge) == Action::Dodge);
    assert(Resolve(all, kGamepad.dodge) == Action::Dodge);
    assert(Resolve(all, kKeyboard.heavy) == Action::Heavy);
    assert(Resolve(all, kGamepad.heavy) == Action::Heavy);
    assert(Resolve(all, Code(Device::Keyboard, 0x2A)) == Action::HeavyModifier);
    assert(Resolve(all, kUnbound) == Action::None);
    assert(Resolve(all, Code(Device::Keyboard, 0x01)) == Action::None);
    // An unbound slot must not match an unbound (zero) code from any device.
    Bindings none;
    assert(Resolve(none, kUnbound) == Action::None);
    // Conflicting remap: the documented priority decides, in both orders.
    Bindings conflict;
    conflict.guard.primary = 300;
    conflict.dodge.primary = 300;
    assert(Resolve(conflict, 300) == Action::Dodge);

    // --- identical edge semantics on every device -----------------------------
    const auto keyboardFrames = Session(kKeyboard);
    const auto gamepadFrames = Session(kGamepad);
    for (std::size_t i = 0; i < keyboardFrames.size(); ++i) {
        assert(SameInput(keyboardFrames[i], gamepadFrames[i]));
    }
    assert(keyboardFrames[0].dodge.pressed == 1 && keyboardFrames[0].dodge.held == 1);
    assert(keyboardFrames[1].dodge.pressed == 0 && keyboardFrames[1].dodge.held == 1);  // repeat is not a press
    assert(keyboardFrames[2].dodge.released == 1 && keyboardFrames[2].dodge.held == 0);
    // A tap that starts and ends inside one frame is not lost.
    assert(keyboardFrames[2].jump.pressed == 1 && keyboardFrames[2].jump.released == 1);
    assert(keyboardFrames[2].jump.held == 0);
    assert(keyboardFrames[3].light.pressed == 1);
    assert(keyboardFrames[4].light.released == 1 && keyboardFrames[4].guard.pressed == 1);
    assert(keyboardFrames[5].heavy.pressed == 1 && keyboardFrames[5].guard.held == 1);
    assert(keyboardFrames[5].light.pressed == 0);  // a dedicated heavy button never fires light
    assert(keyboardFrames[7].walk == 1);

    // Edges are cleared by Take and held state is not.
    {
        Controls controls;
        controls.SetBindings(all);
        controls.Button(kKeyboard.guard, true, true);
        assert(controls.Take(0).guard.pressed == 1);
        const auto next = controls.Take(0);
        assert(next.guard.pressed == 0 && next.guard.held == 1);
    }
    // Releasing something never pressed creates no edge.
    {
        Controls controls;
        controls.SetBindings(all);
        controls.Button(kKeyboard.dodge, false, false);
        const auto frame = controls.Take(0);
        assert(frame.dodge.released == 0 && frame.dodge.held == 0);
    }
    // Walk toggles on the first-down event only.
    {
        Controls controls;
        controls.SetBindings(all);
        controls.Button(kKeyboard.walk, true, true);
        controls.Button(kKeyboard.walk, true, false);
        assert(controls.Walking());
        controls.Button(kKeyboard.walk, true, true);
        assert(!controls.Walking());
    }
    // Unbound or unknown controls change nothing.
    {
        Controls controls;
        controls.SetBindings(all);
        controls.Button(kUnbound, true, true);
        controls.Button(Code(Device::Gamepad, 0x8000), true, true);
        const auto frame = controls.Take(0);
        assert(!frame.dodge.held && !frame.jump.held && !frame.light.held && !frame.heavy.held && !frame.guard.held);
    }

    // --- remapping --------------------------------------------------------------
    {
        Controls controls;
        Bindings remapped = all;
        remapped.dodge.primary = Code(Device::Keyboard, 0x2C);  // Z
        controls.SetBindings(remapped);
        controls.Button(kKeyboard.dodge, true, true);  // the old key no longer rolls
        assert(controls.Take(0).dodge.held == 0);
        controls.Button(Code(Device::Keyboard, 0x2C), true, true);
        assert(controls.Take(0).dodge.pressed == 1);
        controls.Button(kGamepad.dodge, true, true);  // the pad binding is unaffected
        assert(controls.Take(0).dodge.held == 1);
    }

    // --- modifier + light is heavy (keyboard), decided when the press starts ----
    {
        Controls controls;
        controls.SetBindings(KeyboardBindings());
        controls.Button(0x2A, true, true);   // modifier down
        controls.Button(256, true, true);    // light down -> heavy
        auto frame = controls.Take(0);
        assert(frame.heavy.pressed == 1 && frame.light.pressed == 0);
        controls.Button(0x2A, false, false);  // modifier up first
        frame = controls.Take(0);
        assert(frame.heavy.held == 1);  // still the same heavy press
        controls.Button(256, false, false);
        frame = controls.Take(0);
        assert(frame.heavy.released == 1 && frame.light.released == 0 && frame.light.held == 0);

        // A press that began as light stays light when the modifier arrives.
        controls.Button(256, true, true);
        controls.Button(0x2A, true, true);
        controls.Button(256, true, false);  // auto-repeat after the modifier arrived
        frame = controls.Take(0);
        assert(frame.light.pressed == 1 && frame.heavy.pressed == 0);
        controls.Button(256, false, false);
        frame = controls.Take(0);
        assert(frame.light.released == 1 && frame.heavy.held == 0);
    }
    // A held dedicated heavy button does not turn a light press into heavy.
    {
        Controls controls;
        controls.SetBindings(all);
        controls.Button(kGamepad.heavy, true, true);
        controls.Button(kGamepad.light, true, true);
        const auto frame = controls.Take(0);
        assert(frame.heavy.pressed == 1 && frame.light.pressed == 1);
    }

    // --- movement composition -----------------------------------------------------
    {
        Controls controls;
        controls.SetBindings(all);
        controls.Stick(0.0f, 1.0f);
        auto frame = controls.Take(1.25f);
        assert(Near(frame.move_y, 1.0f) && Near(frame.camera_heading, 1.25f));
        // Keyboard movement wins over a resting stick, and diagonals are unit length.
        controls.Move(Direction::Forward, true);
        controls.Move(Direction::Right, true);
        frame = controls.Take(0);
        assert(Near(std::sqrt(frame.move_x * frame.move_x + frame.move_y * frame.move_y), 1.0f));
        assert(frame.move_x > 0.0f && frame.move_y > 0.0f);
        controls.Move(Direction::Forward, false);
        controls.Move(Direction::Right, false);
        // A stick pushed past unit length is clamped, not amplified.
        controls.Stick(3.0f, 4.0f);
        frame = controls.Take(0);
        assert(Near(frame.move_x, 0.6f) && Near(frame.move_y, 0.8f));
        // Walking tilts lightly.
        controls.Button(kKeyboard.walk, true, true);
        frame = controls.Take(0);
        assert(Near(frame.move_x, 0.24f) && Near(frame.move_y, 0.32f));
        // Non-finite stick axes read as centered.
        controls.Stick(kNaN, std::numeric_limits<float>::infinity());
        frame = controls.Take(0);
        assert(frame.move_x == 0.0f && frame.move_y == 0.0f);
    }

    // --- camera heading, perspective-free ------------------------------------------
    using namespace BridgePlan;
    assert(Near(NormalizeAngle(0.0f), 0.0f));
    assert(Near(NormalizeAngle(2.0f * kPi + 0.25f), 0.25f));
    assert(Near(NormalizeAngle(-2.0f * kPi - 0.25f), -0.25f));
    assert(Near(NormalizeAngle(kPi + 0.1f), -kPi + 0.1f));    // wraps across +pi
    assert(Near(NormalizeAngle(-kPi - 0.1f), kPi - 0.1f));    // wraps across -pi
    assert(NormalizeAngle(kNaN) == 0.0f);
    assert(NormalizeAngle(std::numeric_limits<float>::infinity()) == 0.0f);

    // First person: no free rotation. The heading is the player's yaw.
    assert(Near(CameraHeading({ 1.0f, false, 0.0f }), 1.0f));
    // Third person with an orbit: yaw plus offset, wrapped.
    assert(Near(CameraHeading({ 3.0f, true, 0.5f }), 3.5f - 2.0f * kPi));
    assert(Near(CameraHeading({ -3.0f, true, -0.5f }), -3.5f + 2.0f * kPi));
    // The orbit offset is ignored unless the camera reports it enabled.
    assert(Near(CameraHeading({ 1.0f, false, 2.0f }), 1.0f));
    // Equivalent facts give one heading regardless of how the sum is split or wrapped.
    assert(Near(CameraHeading({ 1.0f, true, 2.0f }), CameraHeading({ 3.0f, false, 0.0f })));
    assert(Near(CameraHeading({ 0.4f, false, 0.0f }), CameraHeading({ 0.4f + 2.0f * kPi, false, 0.0f }), 1e-4f));
    // Bad reads degrade to a safe heading rather than NaN.
    assert(Near(CameraHeading({ kNaN, true, 1.0f }), 1.0f));  // unreadable yaw reads as 0
    assert(Near(CameraHeading({ 1.0f, true, kNaN }), 1.0f));
    assert(std::abs(CameraHeading({ 1000.0f, true, 1000.0f })) <= kPi);

    // --- ground ----------------------------------------------------------------------
    assert(Near(ResolveGroundZ(100.0f, { false, 0.0f }, 30.0f), 100.0f));           // nothing found
    assert(Near(ResolveGroundZ(100.0f, { true, 98.0f }, 30.0f), 98.0f));            // standing on a slope
    assert(Near(ResolveGroundZ(500.0f, { true, 80.0f }, 30.0f), 80.0f));            // airborne: ground is far below
    assert(Near(ResolveGroundZ(100.0f, { true, 120.0f }, 30.0f), 120.0f));          // a climbable step
    assert(Near(ResolveGroundZ(100.0f, { true, 400.0f }, 30.0f), 100.0f));          // cast started inside geometry
    assert(Near(ResolveGroundZ(100.0f, { true, kNaN }, 30.0f), 100.0f));
    assert(Near(ResolveGroundZ(100.0f, { true, 90.0f }, kNaN), 100.0f));
    assert(Near(ResolveGroundZ(100.0f, { true, 90.0f }, -1.0f), 100.0f));

    // --- lock-on -----------------------------------------------------------------------
    std::array<LockCandidate, 3> actors{
        LockCandidate{ 0x10, { 1, 2, 3 }, true, true, true, true },
        LockCandidate{ 0x20, { 4, 5, 6 }, true, true, true, true },
        LockCandidate{ 0x30, { 7, 8, 9 }, true, true, true, true },
    };
    auto lock = ResolveLockTarget(0x20, actors);
    assert(lock.hasTarget && lock.stableId == 0x20 && Near(lock.position.y, 5.0f));
    // Independent of candidate order.
    std::swap(actors[0], actors[2]);
    assert(ResolveLockTarget(0x20, actors).stableId == 0x20);
    // No request means no target, however many hostiles are around.
    assert(!ResolveLockTarget(0, actors).hasTarget);
    assert(!ResolveLockTarget(0x99, actors).hasTarget);
    // A lost target is never replaced by a different actor.
    for (auto& actor : actors) {
        if (actor.stableId == 0x20) {
            actor.alive = false;
        }
    }
    assert(!ResolveLockTarget(0x20, actors).hasTarget);
    for (auto& actor : actors) {
        actor.alive = true;
    }
    for (auto flag : { 0, 1, 2, 3 }) {
        auto changed = actors;
        for (auto& actor : changed) {
            if (actor.stableId == 0x20) {
                (flag == 0 ? actor.hostile : flag == 1 ? actor.enabled : flag == 2 ? actor.loaded : actor.alive) = false;
            }
        }
        assert(!ResolveLockTarget(0x20, changed).hasTarget);
    }
    for (auto& actor : actors) {
        if (actor.stableId == 0x20) {
            actor.position.x = kNaN;
        }
    }
    assert(!ResolveLockTarget(0x20, actors).hasTarget);

    return 0;
}
