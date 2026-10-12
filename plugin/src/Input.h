#pragma once

#include "InputPlan.h"
#include "er_combat.h"

// Listens to Skyrim's raw input and feeds InputPlan::Controls, which owns the
// device-independent rules (edges, heavy modifier, walk toggle, movement).
// Movement comes from the remappable Forward/Back/Strafe user events and the
// left stick, so it follows the player's own bindings. Keyboard, mouse and
// gamepad all arrive as one device-tagged control code.
class Input : public RE::BSTEventSink<RE::InputEvent*>
{
public:
    static Input* Get();
    void Register();

    // Re-reads the bindings from Settings.
    void ApplyBindings();

    // The controls since the last call. Clears the one-frame edges.
    ErInput Take(float cameraHeading);

    // Activate was pressed since the last call (for the bridge to decide
    // whether it may go through).
    bool activatePressed = false;

protected:
    RE::BSEventNotifyControl ProcessEvent(RE::InputEvent* const* a_event, RE::BSTEventSource<RE::InputEvent*>*) override;

private:
    InputPlan::Controls controls;
};
