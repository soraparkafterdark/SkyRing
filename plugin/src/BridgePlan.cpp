#include "BridgePlan.h"

#include <algorithm>
#include <cmath>
#include <limits>

namespace
{
    bool Finite(ErVec3 value)
    {
        return std::isfinite(value.x) && std::isfinite(value.y) && std::isfinite(value.z);
    }

    bool Eligible(const BridgePlan::TargetCandidate& candidate)
    {
        return candidate.stableId != 0 && candidate.hostile && candidate.alive && candidate.enabled && candidate.loaded;
    }
}

namespace BridgePlan
{
    bool ShouldEnableCombat(bool weaponDrawn, bool menuUp, bool dead)
    {
        return weaponDrawn && !menuUp && !dead;
    }

    bool CanConfirmNativeHit(
        bool hitLive,
        std::uint32_t currentActionSerial,
        std::uint32_t observedActionSerial,
        std::uint32_t observedTargetId,
        std::uint32_t lastConfirmedActionSerial,
        std::uint32_t nativeActionSerial,
        std::uint32_t nativeTargetId)
    {
        return hitLive && observedActionSerial != 0 && observedTargetId != 0 &&
               currentActionSerial == observedActionSerial && nativeActionSerial == observedActionSerial &&
               nativeTargetId == observedTargetId && lastConfirmedActionSerial != observedActionSerial;
    }

    CombatMath::Capsule ActorCapsule(ErVec3 feet, ErVec3 modelMin, ErVec3 modelMax, float actualHeight)
    {
        if (!Finite(feet) || !Finite(modelMin) || !Finite(modelMax) || !std::isfinite(actualHeight) ||
            actualHeight <= 0.0f) {
            return CombatMath::UprightCapsule(feet, -1.0f, -1.0f);
        }

        const float modelHeight = modelMax.z - modelMin.z;
        const float modelWidth = std::max(modelMax.x - modelMin.x, modelMax.y - modelMin.y);
        if (!std::isfinite(modelHeight) || !std::isfinite(modelWidth) || modelHeight <= 0.0f || modelWidth < 0.0f) {
            return CombatMath::UprightCapsule(feet, -1.0f, -1.0f);
        }

        const float scale = actualHeight / modelHeight;
        const float radius = modelWidth * scale * 0.5f;
        return CombatMath::UprightCapsule(feet, actualHeight, radius);
    }

    TargetSelection SelectSweepTarget(const ErOutput& swing, std::span<const TargetCandidate> candidates)
    {
        TargetSelection result{};
        result.surfaceClearance = std::numeric_limits<float>::infinity();

        for (const auto& candidate : candidates) {
            if (!Eligible(candidate)) {
                continue;
            }

            const auto contact = CombatMath::IntersectSweep(swing, candidate.bounds);
            if (!contact.hit) {
                continue;
            }
            const float clearance = contact.distance - candidate.bounds.radius - swing.hit_radius;
            const bool earlier = !result.found || contact.step < result.contact.step;
            const bool closer = result.found && contact.step == result.contact.step &&
                                clearance < result.surfaceClearance;
            const bool lowerId = result.found && contact.step == result.contact.step &&
                                 clearance == result.surfaceClearance && candidate.stableId < result.stableId;
            if (earlier || closer || lowerId) {
                result.found = true;
                result.stableId = candidate.stableId;
                result.contact = contact;
                result.surfaceClearance = clearance;
            }
        }
        return result;
    }

    MovementRequest ReconcileMovement(ErVec3 actual, const ErOutput& desired, MovementLimits limits)
    {
        MovementRequest result{};
        if (!Finite(actual) || !Finite(desired.position) || !std::isfinite(desired.heading) ||
            !std::isfinite(limits.maxHorizontalDistance) || !std::isfinite(limits.maxVerticalDistance) ||
            limits.maxHorizontalDistance < 0.0f || limits.maxVerticalDistance < 0.0f) {
            return result;
        }

        result.accepted = true;
        result.displacement = {
            desired.position.x - actual.x,
            desired.position.y - actual.y,
            desired.position.z - actual.z,
        };

        const float horizontalSquared = result.displacement.x * result.displacement.x +
                                        result.displacement.y * result.displacement.y;
        const float horizontal = std::sqrt(horizontalSquared);
        if (horizontal > limits.maxHorizontalDistance) {
            const float scale = horizontal > 0.0f ? limits.maxHorizontalDistance / horizontal : 0.0f;
            result.displacement.x *= scale;
            result.displacement.y *= scale;
            result.horizontalClamped = true;
        }

        if (desired.airborne == 0) {
            result.displacement.z = 0.0f;
        } else {
            const float bounded = std::clamp(
                result.displacement.z,
                -limits.maxVerticalDistance,
                limits.maxVerticalDistance);
            result.verticalClamped = bounded != result.displacement.z;
            result.displacement.z = bounded;
        }

        constexpr float tau = 6.2831853071795864769f;
        result.heading = std::remainder(desired.heading, tau);
        constexpr float translationEpsilonSquared = 1e-8f;
        const float lengthSquared = result.displacement.x * result.displacement.x +
                                    result.displacement.y * result.displacement.y +
                                    result.displacement.z * result.displacement.z;
        result.hasTranslation = lengthSquared > translationEpsilonSquared;
        return result;
    }

    float NormalizeAngle(float radians)
    {
        if (!std::isfinite(radians)) {
            return 0.0f;
        }
        constexpr float tau = 6.2831853071795864769f;
        return std::remainder(radians, tau);
    }

    float CameraHeading(const CameraFacts& facts)
    {
        const float yaw = std::isfinite(facts.playerYaw) ? facts.playerYaw : 0.0f;
        const float orbit = facts.freeRotationEnabled && std::isfinite(facts.freeRotationYaw)
                                ? facts.freeRotationYaw
                                : 0.0f;
        return NormalizeAngle(yaw + orbit);
    }

    float ResolveGroundZ(float feetZ, GroundProbe probe, float maxStepUp)
    {
        if (!probe.hit || !std::isfinite(probe.z) || !std::isfinite(feetZ) || !(maxStepUp >= 0.0f)) {
            return feetZ;
        }
        return probe.z > feetZ + maxStepUp ? feetZ : probe.z;
    }

    LockFacts ResolveLockTarget(std::uint32_t requestedId, std::span<const LockCandidate> candidates)
    {
        if (requestedId == 0) {
            return {};
        }
        for (const auto& candidate : candidates) {
            if (candidate.stableId != requestedId) {
                continue;
            }
            if (candidate.hostile && candidate.alive && candidate.enabled && candidate.loaded &&
                Finite(candidate.position)) {
                return { true, candidate.stableId, candidate.position };
            }
        }
        return {};
    }
}
