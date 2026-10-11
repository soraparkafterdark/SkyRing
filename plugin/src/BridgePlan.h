#pragma once

#include "CombatMath.h"

#include <cstdint>
#include <span>

namespace BridgePlan
{
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
}
