#pragma once

#include "CombatMath.h"

#include <cstdint>
#include <span>

namespace BridgePlan
{
    // Combat ownership depends on gameplay state, never camera perspective.
    // Keeping this decision in the perspective-free plan prevents a view
    // switch from tearing down the authoritative combat core.
    bool ShouldEnableCombat(bool weaponDrawn, bool menuUp, bool dead);

    // The CommonLib adapter supplies these facts. Keeping them here as plain
    // data makes target choice testable without a Skyrim process.
    struct TargetCandidate
    {
        std::uint32_t stableId = 0;
        CombatMath::Capsule bounds{};
        bool hostile = false;
        bool alive = false;
        bool enabled = false;
        bool loaded = false;
    };

    struct TargetSelection
    {
        bool found = false;
        std::uint32_t stableId = 0;
        CombatMath::SweepContact contact{};
        float surfaceClearance = 0.0f;
    };

    // Chooses independently of actor iteration order: earliest contact, then
    // closest surface approach, then lowest stable form ID.
    TargetSelection SelectSweepTarget(const ErOutput& swing, std::span<const TargetCandidate> candidates);

    // A post-damage Skyrim event confirms the core contact only when both
    // refer to the same still-current action and target, exactly once.
    bool CanConfirmNativeHit(
        bool hitLive,
        std::uint32_t currentActionSerial,
        std::uint32_t observedActionSerial,
        std::uint32_t observedTargetId,
        std::uint32_t lastConfirmedActionSerial,
        std::uint32_t nativeActionSerial,
        std::uint32_t nativeTargetId);

    // Converts model-space bounds plus Skyrim's scaled actor height into the
    // upright world-space approximation used by the sweep selector.
    CombatMath::Capsule ActorCapsule(ErVec3 feet, ErVec3 modelMin, ErVec3 modelMax, float actualHeight);

    struct MovementLimits
    {
        float maxHorizontalDistance = 0.0f;
        float maxVerticalDistance = 0.0f;
    };

    struct MovementRequest
    {
        bool accepted = false;
        bool hasTranslation = false;
        bool horizontalClamped = false;
        bool verticalClamped = false;
        ErVec3 displacement{};
        float heading = 0.0f;
    };

    // Converts the core's desired position into a bounded per-tick request.
    // The runtime must feed this through Skyrim's character controller and
    // report the resulting actual position next tick; it must not teleport.
    MovementRequest ReconcileMovement(ErVec3 actual, const ErOutput& desired, MovementLimits limits);

    // Camera facts the engine adapter reads. Perspective is deliberately not a
    // field: first person reports no free rotation, third person reports its
    // orbit offset, and the core only ever sees the resulting heading.
    struct CameraFacts
    {
        float playerYaw = 0.0f;
        bool freeRotationEnabled = false;
        float freeRotationYaw = 0.0f;
    };

    // Radians in [-pi, pi]; non-finite input becomes 0 so a bad read cannot
    // poison the core's position.
    float NormalizeAngle(float radians);

    // The heading the stick is relative to, normalized so wraparound at +-pi
    // cannot change what the core computes.
    float CameraHeading(const CameraFacts& facts);

    // Result of a downward cast from the player's feet, supplied by the
    // adapter that owns the collision query.
    struct GroundProbe
    {
        bool hit = false;
        float z = 0.0f;
    };

    // Ground height for ErWorld. No hit, a non-finite hit, or a hit above the
    // step the character controller could have climbed (the cast started
    // inside geometry) means "no better information": stand where Skyrim
    // already put the player.
    float ResolveGroundZ(float feetZ, GroundProbe probe, float maxStepUp);

    // The lock-on provider (True Directional Movement) names a target by its
    // stable form ID; the adapter reads each loaded candidate's facts.
    struct LockCandidate
    {
        std::uint32_t stableId = 0;
        ErVec3 position{};
        bool hostile = false;
        bool alive = false;
        bool enabled = false;
        bool loaded = false;
    };

    struct LockFacts
    {
        bool hasTarget = false;
        std::uint32_t stableId = 0;
        ErVec3 position{};
    };

    // Reports the requested target only while it is a live, loaded, hostile
    // actor. It never substitutes another actor: when the requested target is
    // lost, the core gets no target, independent of candidate order.
    LockFacts ResolveLockTarget(std::uint32_t requestedId, std::span<const LockCandidate> candidates);
}
