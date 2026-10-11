#include "CombatMath.h"

#include <algorithm>
#include <cmath>
#include <limits>

namespace
{
    ErVec3 Add(ErVec3 a, ErVec3 b) { return { a.x + b.x, a.y + b.y, a.z + b.z }; }
    ErVec3 Sub(ErVec3 a, ErVec3 b) { return { a.x - b.x, a.y - b.y, a.z - b.z }; }
    ErVec3 Mul(ErVec3 v, float scalar) { return { v.x * scalar, v.y * scalar, v.z * scalar }; }
    float Dot(ErVec3 a, ErVec3 b) { return a.x * b.x + a.y * b.y + a.z * b.z; }

    bool Finite(ErVec3 v) { return std::isfinite(v.x) && std::isfinite(v.y) && std::isfinite(v.z); }
    bool Valid(const CombatMath::Capsule& capsule)
    {
        return Finite(capsule.a) && Finite(capsule.b) && std::isfinite(capsule.radius) && capsule.radius >= 0.0f;
    }
}

namespace CombatMath
{
    Capsule UprightCapsule(ErVec3 feet, float height, float radius)
    {
        if (!Finite(feet) || !std::isfinite(height) || !std::isfinite(radius) || height <= 0.0f || radius < 0.0f) {
            return { feet, feet, -1.0f };
        }
        const float boundedRadius = std::min(radius, height * 0.5f);
        auto bottom = feet;
        auto top = feet;
        bottom.z += boundedRadius;
        top.z += height - boundedRadius;
        return { bottom, top, boundedRadius };
    }

    float SegmentDistanceSquared(ErVec3 a0, ErVec3 a1, ErVec3 b0, ErVec3 b1)
    {
        if (!Finite(a0) || !Finite(a1) || !Finite(b0) || !Finite(b1)) {
            return std::numeric_limits<float>::infinity();
        }

        constexpr float epsilon = 1e-6f;
        const ErVec3 d1 = Sub(a1, a0);
        const ErVec3 d2 = Sub(b1, b0);
        const ErVec3 r = Sub(a0, b0);
        const float a = Dot(d1, d1);
        const float e = Dot(d2, d2);
        const float f = Dot(d2, r);
        float s = 0.0f;
        float t = 0.0f;

        if (a <= epsilon && e <= epsilon) {
            return Dot(r, r);
        }
        if (a <= epsilon) {
            t = std::clamp(f / e, 0.0f, 1.0f);
        } else {
            const float c = Dot(d1, r);
            if (e <= epsilon) {
                s = std::clamp(-c / a, 0.0f, 1.0f);
            } else {
                const float b = Dot(d1, d2);
                const float denominator = a * e - b * b;
                if (denominator > epsilon) {
                    s = std::clamp((b * f - c * e) / denominator, 0.0f, 1.0f);
                }
                t = (b * s + f) / e;
                if (t < 0.0f) {
                    t = 0.0f;
                    s = std::clamp(-c / a, 0.0f, 1.0f);
                } else if (t > 1.0f) {
                    t = 1.0f;
                    s = std::clamp((b - c) / a, 0.0f, 1.0f);
                }
            }
        }

        const ErVec3 closestA = Add(a0, Mul(d1, s));
        const ErVec3 closestB = Add(b0, Mul(d2, t));
        const ErVec3 separation = Sub(closestA, closestB);
        return Dot(separation, separation);
    }

    SweepContact IntersectSweep(const ErOutput& swing, const Capsule& target)
    {
        SweepContact result{};
        result.distance = std::numeric_limits<float>::infinity();
        if (swing.hit_live == 0 || !Valid(target) || !std::isfinite(swing.hit_radius) || swing.hit_radius < 0.0f) {
            return result;
        }

        const float combinedRadius = swing.hit_radius + target.radius;
        const float thresholdSquared = combinedRadius * combinedRadius;
        if (!std::isfinite(thresholdSquared)) {
            return result;
        }
        for (std::size_t step = 0; step < ER_SWEEP_STEPS; ++step) {
            const float distanceSquared =
                SegmentDistanceSquared(swing.sweep_a[step], swing.sweep_b[step], target.a, target.b);
            if (distanceSquared < result.distance * result.distance) {
                result.distance = std::sqrt(distanceSquared);
            }
            if (!result.hit && distanceSquared <= thresholdSquared) {
                result.hit = true;
                result.step = step;
            }
        }
        return result;
    }
}
