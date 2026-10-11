#pragma once

#include "er_combat.h"

#include <cstddef>

namespace CombatMath
{
    struct Capsule
    {
        ErVec3 a;
        ErVec3 b;
        float radius;
    };

    struct SweepContact
    {
        bool hit = false;
        // Earliest temporal sample that intersects the target.
        std::size_t step = 0;
        // Closest centreline distance across the complete sweep.
        float distance = 0.0f;
    };

    // Actor positions are at their feet. This approximation is deliberately
    // isolated so the bridge can later replace it with controller geometry
    // without touching the sweep test.
    Capsule UprightCapsule(ErVec3 feet, float height, float radius);

    float SegmentDistanceSquared(ErVec3 a0, ErVec3 a1, ErVec3 b0, ErVec3 b1);

    // Tests each temporal weapon segment emitted by the core against an actor
    // capsule. A swing is confirmed once by the caller after choosing a valid
    // hostile target; this function has no gameplay side effects.
    SweepContact IntersectSweep(const ErOutput& swing, const Capsule& target);
}
