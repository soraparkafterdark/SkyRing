#include "CombatMath.h"

#include <cassert>
#include <cmath>
#include <limits>

namespace
{
    bool Near(float actual, float expected) { return std::abs(actual - expected) < 1e-5f; }
}

int main()
{
    using namespace CombatMath;

    assert(Near(SegmentDistanceSquared({ 0, 0, 0 }, { 2, 0, 0 }, { 1, -1, 0 }, { 1, 1, 0 }), 0.0f));
    assert(Near(SegmentDistanceSquared({ 0, 0, 0 }, { 2, 0, 0 }, { 0, 3, 0 }, { 2, 3, 0 }), 9.0f));
    assert(Near(SegmentDistanceSquared({ 0, 0, 0 }, { 0, 0, 0 }, { 3, 4, 0 }, { 3, 4, 0 }), 25.0f));
    assert(std::isinf(SegmentDistanceSquared(
        { std::numeric_limits<float>::quiet_NaN(), 0, 0 }, { 0, 0, 0 }, { 0, 0, 0 }, { 0, 0, 0 })));

    const auto capsule = UprightCapsule({ 0, 0, 0 }, 128.0f, 24.0f);
    assert(Near(capsule.a.z, 24.0f));
    assert(Near(capsule.b.z, 104.0f));

    ErOutput swing{};
    swing.hit_live = 1;
    swing.hit_radius = 5.0f;
    for (std::size_t i = 0; i < ER_SWEEP_STEPS; ++i) {
        swing.sweep_a[i] = { -50.0f, 20.0f, 64.0f };
        swing.sweep_b[i] = { 50.0f, 20.0f, 64.0f };
    }
    const auto contact = IntersectSweep(swing, capsule);
    assert(contact.hit);
    assert(Near(contact.distance, 20.0f));

    const auto distant = UprightCapsule({ 0, 100, 0 }, 128.0f, 24.0f);
    assert(!IntersectSweep(swing, distant).hit);
    swing.hit_live = 0;
    assert(!IntersectSweep(swing, capsule).hit);

    const auto invalid = UprightCapsule({ 0, 0, 0 }, -1.0f, 24.0f);
    swing.hit_live = 1;
    assert(!IntersectSweep(swing, invalid).hit);
}
