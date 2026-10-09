#include "Bridge.h"

#include "Input.h"
#include "Settings.h"

static_assert(sizeof(ErInput) == 32);
static_assert(sizeof(ErWorld) == 36);
static_assert(sizeof(ErOutput) == 220);
static_assert(offsetof(ErOutput, sweep_a) == 88);

namespace
{
    ErVec3 ToEr(const RE::NiPoint3& p) { return { p.x, p.y, p.z }; }

    constexpr std::array kDirSuffix{ "Forward"sv, "Back"sv, "Left"sv, "Right"sv };

    bool Directional(ErAction kind)
    {
        return kind == ER_ACTION_ROLL || kind == ER_ACTION_CROUCH_ROLL || kind == ER_ACTION_HURT;
    }

    bool MenuUp()
    {
        auto* ui = RE::UI::GetSingleton();
        return !ui || ui->GameIsPaused() || ui->IsApplicationMenuOpen() || ui->IsModalMenuOpen() ||
               ui->IsMenuOpen(RE::DialogueMenu::MENU_NAME);
    }
}

void Bridge::Install()
{
    REL::Relocation<std::uintptr_t> vtbl{ RE::VTABLE_PlayerCharacter[0] };
    _Update = vtbl.write_vfunc(0xAD, Update);
    logger::info("Hooked PlayerCharacter::Update");
}

void Bridge::Update(RE::PlayerCharacter* a_this, float a_delta)
{
    _Update(a_this, a_delta);
    Tick(a_this, a_delta);
}

void Bridge::Tick(RE::PlayerCharacter* player, float delta)
{
    auto* state = player->AsActorState();
    const bool want = state && state->IsWeaponDrawn() && !MenuUp() && !player->IsDead() &&
                      !RE::PlayerCamera::GetSingleton()->IsInFirstPerson();
    if (want != combatMode) {
        SetCombatMode(want);
    }

    auto* input = Input::Get();
    if (!combatMode) {
        input->Take(0.0f);  // drop edges so nothing fires on entry
        input->activatePressed = false;
        return;
    }

    if (!combat) {
        combat = er_create(ToEr(player->GetPosition()));
    }

    const ErInput controls = input->Take(CameraHeading(player));
    ErWorld world{};
    world.position = ToEr(player->GetPosition());
    // TODO: ray cast for the ground under the player. Standing on it is the
    // common case, and Skyrim's controller keeps the player on the floor.
    world.ground_z = player->GetPosition().z;
    world.in_combat = player->IsInCombat();
    // TODO: take the lock-on target from True Directional Movement's API.
    world.has_target = 0;

    ErOutput out{};
    er_update(combat, delta, &controls, &world, &out);

    if (out.action_serial != lastSerial) {
        lastSerial = out.action_serial;
        PlayAction(player, out);
    }

    // I-frames through the graph variables the game and MaxsuIFrame read.
    const bool invincible = out.invincible != 0;
    if (invincible != iframes) {
        iframes = invincible;
        player->SetGraphVariableBool("bIframeActive"sv, invincible);
        player->SetGraphVariableBool("bInIframe"sv, invincible);
    }

    // Activate always goes through: the core says when it would cut an action
    // short, and Skyrim's own activation simply waits for the animation.
    input->activatePressed = false;
}

void Bridge::SetCombatMode(bool on)
{
    combatMode = on;
    logger::info("Combat mode {}", on ? "on" : "off");
    if (auto* controls = RE::PlayerControls::GetSingleton()) {
        // The core owns these while a weapon is drawn. Movement, Activate,
        // shouts, sneaking and the camera stay with Skyrim.
        controls->sprintHandler->SetInputEventHandlingEnabled(!on);
        controls->jumpHandler->SetInputEventHandlingEnabled(!on);
        controls->attackBlockHandler->SetInputEventHandlingEnabled(!on);
    }
    if (!on && combat) {
        er_destroy(combat);
        combat = nullptr;
        lastSerial = 0;
        if (iframes) {
            iframes = false;
            if (auto* player = RE::PlayerCharacter::GetSingleton()) {
                player->SetGraphVariableBool("bIframeActive"sv, false);
                player->SetGraphVariableBool("bInIframe"sv, false);
            }
        }
    }
}

void Bridge::PlayAction(RE::PlayerCharacter* player, const ErOutput& out)
{
    const auto kind = out.action_kind;
    if (kind >= ER_ACTION_COUNT) {
        return;
    }
    const auto& base = Settings::Get().events[kind];
    if (base.empty()) {
        return;
    }
    if (Directional(kind) && out.action_dir < kDirSuffix.size()) {
        const auto named = base + std::string(kDirSuffix[out.action_dir]);
        if (player->NotifyAnimationGraph(named.c_str())) {
            return;
        }
    }
    if (!player->NotifyAnimationGraph(base.c_str())) {
        logger::debug("No animation answered {} for action {}", base, static_cast<std::uint32_t>(kind));
    }
}

float Bridge::CameraHeading(RE::PlayerCharacter* player)
{
    float heading = player->GetAngleZ();
    auto* camera = RE::PlayerCamera::GetSingleton();
    if (camera && camera->currentState && camera->currentState->id == RE::CameraState::kThirdPerson) {
        auto* third = static_cast<RE::ThirdPersonState*>(camera->currentState.get());
        if (third->freeRotationEnabled) {
            heading += third->freeRotation.x;
        }
    }
    return heading;
}
