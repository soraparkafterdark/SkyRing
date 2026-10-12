#include "Bridge.h"

#include "ActorAdapter.h"
#include "Input.h"
#include "Settings.h"

static_assert(sizeof(ErInput) == 32);
static_assert(sizeof(ErWorld) == 36);
static_assert(sizeof(ErOutput) == 224);
static_assert(offsetof(ErOutput, sweep_a) == 88);

namespace
{
    ErVec3 ToEr(const RE::NiPoint3& p) { return { p.x, p.y, p.z }; }

    std::uint64_t PackContact(std::uint32_t serial, std::uint32_t targetId)
    {
        return static_cast<std::uint64_t>(serial) << 32 | targetId;
    }

    std::uint32_t ContactSerial(std::uint64_t contact) { return static_cast<std::uint32_t>(contact >> 32); }
    std::uint32_t ContactTarget(std::uint64_t contact) { return static_cast<std::uint32_t>(contact); }

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

    class NativeHitSink final : public RE::BSTEventSink<RE::TESHitEvent>
    {
    public:
        static NativeHitSink* GetSingleton()
        {
            static NativeHitSink sink;
            return &sink;
        }

        RE::BSEventNotifyControl ProcessEvent(
            const RE::TESHitEvent* event,
            RE::BSTEventSource<RE::TESHitEvent>*) override
        {
            if (!event || !event->cause || !event->target || event->projectile != 0 || !event->cause->IsPlayerRef()) {
                return RE::BSEventNotifyControl::kContinue;
            }

            auto* player = RE::PlayerCharacter::GetSingleton();
            auto* weapon = player ? player->GetEquippedObject(false) : nullptr;
            if (!weapon || event->source != weapon->GetFormID()) {
                return RE::BSEventNotifyControl::kContinue;
            }

            Bridge::QueueNativeHit(event->target->GetFormID());
            return RE::BSEventNotifyControl::kContinue;
        }
    };
}

void Bridge::Install()
{
    REL::Relocation<std::uintptr_t> vtbl{ RE::VTABLE_PlayerCharacter[0] };
    _Update = vtbl.write_vfunc(0xAD, Update);
    logger::info("Hooked PlayerCharacter::Update");
}

void Bridge::RegisterEvents()
{
    if (auto* events = RE::ScriptEventSourceHolder::GetSingleton()) {
        events->AddEventSink(NativeHitSink::GetSingleton());
        logger::info("Listening for Skyrim-native melee hits");
    } else {
        logger::error("Could not register Skyrim-native hit listener");
    }
}

void Bridge::QueueNativeHit(std::uint32_t targetId)
{
    // TESHitEvent is post-damage. Snapshot only a target that the core has
    // already observed for this action; Tick performs the core mutation.
    const auto contact = observedContact.load(std::memory_order_acquire);
    if (contact != 0 && ContactTarget(contact) == targetId) {
        pendingNativeHit.store(contact, std::memory_order_release);
    }
}

void Bridge::Update(RE::PlayerCharacter* a_this, float a_delta)
{
    _Update(a_this, a_delta);
    Tick(a_this, a_delta);
}

void Bridge::Tick(RE::PlayerCharacter* player, float delta)
{
    auto* state = player->AsActorState();
    // Perspective is presentation-only. In particular, changing to first
    // person must not destroy and recreate `combat`, which would reset the
    // live action clock, serial, stamina, i-frames, buffers, and contacts.
    const bool want = state && BridgePlan::ShouldEnableCombat(state->IsWeaponDrawn(), MenuUp(), player->IsDead());
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
        observedContact.store(0, std::memory_order_release);
        pendingNativeHit.store(0, std::memory_order_release);
        lastContactTarget = 0;
        PlayAction(player, out);
    }

    // I-frames through the graph variables the game and MaxsuIFrame read.
    const bool invincible = out.invincible != 0;
    if (invincible != iframes) {
        iframes = invincible;
        player->SetGraphVariableBool("bIframeActive"sv, invincible);
        player->SetGraphVariableBool("bInIframe"sv, invincible);
    }

    // Observation only until the normal Skyrim hit dispatch is proven. Do not
    // confirm the core hit here: confirmation belongs in the same branch as a
    // successfully submitted native damage event.
    if (Settings::Get().coreHitDetection && out.hit_live != 0 && out.action_serial != lastContactSerial) {
        const auto selection = ActorAdapter::SelectSweepTarget(player, out);
        if (selection.found) {
            lastContactSerial = out.action_serial;
            lastContactTarget = selection.stableId;
            observedContact.store(
                PackContact(lastContactSerial, lastContactTarget),
                std::memory_order_release);
            logger::info(
                "CONTACT_OBSERVED serial={} action={} target={:08X} sample={} clearance={:.3f} damage_mul={:.3f} stance_mul={:.3f}",
                out.action_serial,
                static_cast<std::uint32_t>(out.action_kind),
                selection.stableId,
                selection.contact.step,
                selection.surfaceClearance,
                out.damage_multiplier,
                out.stance_multiplier);
        }
    }

    const auto nativeHit = pendingNativeHit.exchange(0, std::memory_order_acq_rel);
    if (nativeHit != 0 && BridgePlan::CanConfirmNativeHit(
                              out.hit_live != 0,
                              out.action_serial,
                              lastContactSerial,
                              lastContactTarget,
                              lastConfirmedSerial,
                              ContactSerial(nativeHit),
                              ContactTarget(nativeHit))) {
        lastConfirmedSerial = lastContactSerial;
        er_confirm_hit(combat);
        logger::info(
            "CONTACT_CONFIRMED_NATIVE serial={} target={:08X}",
            lastConfirmedSerial,
            lastContactTarget);
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
        lastContactSerial = 0;
        lastContactTarget = 0;
        lastConfirmedSerial = 0;
        observedContact.store(0, std::memory_order_release);
        pendingNativeHit.store(0, std::memory_order_release);
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
