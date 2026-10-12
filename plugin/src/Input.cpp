#include "Input.h"

#include "Settings.h"

#include <optional>

namespace
{
    std::optional<InputPlan::Device> ToDevice(RE::INPUT_DEVICE device)
    {
        switch (device) {
        case RE::INPUT_DEVICE::kKeyboard:
            return InputPlan::Device::Keyboard;
        case RE::INPUT_DEVICE::kMouse:
            return InputPlan::Device::Mouse;
        case RE::INPUT_DEVICE::kGamepad:
            return InputPlan::Device::Gamepad;
        default:
            return std::nullopt;
        }
    }
}

Input* Input::Get()
{
    static Input input;
    return &input;
}

void Input::Register()
{
    ApplyBindings();
    if (auto* devices = RE::BSInputDeviceManager::GetSingleton()) {
        devices->AddEventSink<RE::InputEvent*>(this);
    }
}

void Input::ApplyBindings()
{
    controls.SetBindings(Settings::Get().InputBindings());
}

ErInput Input::Take(float cameraHeading)
{
    return controls.Take(cameraHeading);
}

RE::BSEventNotifyControl Input::ProcessEvent(RE::InputEvent* const* a_event, RE::BSTEventSource<RE::InputEvent*>*)
{
    if (!a_event) {
        return RE::BSEventNotifyControl::kContinue;
    }
    const auto* userEvents = RE::UserEvents::GetSingleton();

    for (auto* event = *a_event; event; event = event->next) {
        if (event->GetEventType() == RE::INPUT_EVENT_TYPE::kThumbstick) {
            auto* stick = static_cast<RE::ThumbstickEvent*>(event);
            if (stick->IsLeft()) {
                controls.Stick(stick->xValue, stick->yValue);
            }
            continue;
        }
        auto* button = event->AsButtonEvent();
        if (!button) {
            continue;
        }
        // IsPressed is true while held, auto-repeat included; IsDown only on
        // the event that put the control down. Controls turns transitions of
        // the first into edges, so repeats cannot fire an action twice.
        const bool down = button->IsPressed();
        const auto& user = button->QUserEvent();
        if (user == userEvents->forward) {
            controls.Move(InputPlan::Direction::Forward, down);
        } else if (user == userEvents->back) {
            controls.Move(InputPlan::Direction::Back, down);
        } else if (user == userEvents->strafeLeft) {
            controls.Move(InputPlan::Direction::Left, down);
        } else if (user == userEvents->strafeRight) {
            controls.Move(InputPlan::Direction::Right, down);
        } else if (user == userEvents->activate && button->IsDown()) {
            activatePressed = true;
        }

        // Our own controls, by device-tagged code. Every device goes through
        // the same path, so they cannot differ in edge behaviour.
        if (const auto device = ToDevice(button->GetDevice())) {
            controls.Button(InputPlan::Code(*device, button->GetIDCode()), down, button->IsDown());
        }
    }
    return RE::BSEventNotifyControl::kContinue;
}
