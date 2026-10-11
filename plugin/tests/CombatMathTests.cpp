#include "CombatMath.h"
#include "BridgePlan.h"

#include <array>
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

    // The first contact sample and closest approach carry different facts.
    swing.sweep_a[0] = { -50.0f, 28.0f, 64.0f };
    swing.sweep_b[0] = { 50.0f, 28.0f, 64.0f };
    swing.sweep_a[1] = { -50.0f, 2.0f, 64.0f };
    swing.sweep_b[1] = { 50.0f, 2.0f, 64.0f };
    const auto timedContact = IntersectSweep(swing, capsule);
    assert(timedContact.hit);
    assert(timedContact.step == 0);
    assert(Near(timedContact.distance, 2.0f));

    const auto distant = UprightCapsule({ 0, 100, 0 }, 128.0f, 24.0f);
    assert(!IntersectSweep(swing, distant).hit);
    swing.hit_live = 0;
    assert(!IntersectSweep(swing, capsule).hit);

    const auto invalid = UprightCapsule({ 0, 0, 0 }, -1.0f, 24.0f);
    swing.hit_live = 1;
    assert(!IntersectSweep(swing, invalid).hit);

    using namespace BridgePlan;

    ErOutput orderedSwing{};
    orderedSwing.hit_live = 1;
    orderedSwing.hit_radius = 2.0f;
    for (std::size_t i = 0; i < ER_SWEEP_STEPS; ++i) {
        orderedSwing.sweep_a[i] = { -10.0f, 100.0f, 10.0f };
        orderedSwing.sweep_b[i] = { 10.0f, 100.0f, 10.0f };
    }
    orderedSwing.sweep_a[0] = { -10.0f, 20.0f, 10.0f };
    orderedSwing.sweep_b[0] = { 10.0f, 20.0f, 10.0f };
    orderedSwing.sweep_a[1] = { -10.0f, 0.0f, 10.0f };
    orderedSwing.sweep_b[1] = { 10.0f, 0.0f, 10.0f };

    const auto atOrigin = UprightCapsule({ 0, 0, 0 }, 20.0f, 2.0f);
    const auto atTwenty = UprightCapsule({ 0, 20, 0 }, 20.0f, 2.0f);
    std::array candidates{
        TargetCandidate{ 80, atOrigin, true, true, true, true },
        TargetCandidate{ 90, atTwenty, true, true, true, true },
    };
    auto selection = SelectSweepTarget(orderedSwing, candidates);
    assert(selection.found);
    assert(selection.stableId == 90);  // earlier contact wins
    assert(selection.contact.step == 0);

    // Selection is stable across input order and uses the form ID as its last
    // tie-break, not the process-list iteration order.
    std::array tied{
        TargetCandidate{ 42, atTwenty, true, true, true, true },
        TargetCandidate{ 7, atTwenty, true, true, true, true },
    };
    selection = SelectSweepTarget(orderedSwing, tied);
    assert(selection.stableId == 7);
    std::swap(tied[0], tied[1]);
    assert(SelectSweepTarget(orderedSwing, tied).stableId == 7);

    tied[1].hostile = false;
    tied[0].stableId = 0;
    assert(!SelectSweepTarget(orderedSwing, tied).found);

    ErOutput desired{};
    desired.position = { 3.0f, 4.0f, 2.0f };
    desired.heading = 4.0f * 3.14159265358979323846f + 0.5f;
    auto movement = ReconcileMovement({ 0, 0, 0 }, desired, { 10.0f, 1.0f });
    assert(movement.accepted);
    assert(movement.hasTranslation);
    assert(Near(movement.displacement.x, 3.0f));
    assert(Near(movement.displacement.y, 4.0f));
    assert(Near(movement.displacement.z, 0.0f));  // ground belongs to Skyrim
    assert(Near(movement.heading, 0.5f));

    desired.position = { 6.0f, 8.0f, 3.0f };
    desired.airborne = 1;
    movement = ReconcileMovement({ 0, 0, 0 }, desired, { 5.0f, 2.0f });
    assert(movement.horizontalClamped);
    assert(movement.verticalClamped);
    assert(Near(movement.displacement.x, 3.0f));
    assert(Near(movement.displacement.y, 4.0f));
    assert(Near(movement.displacement.z, 2.0f));

    desired.position.x = std::numeric_limits<float>::quiet_NaN();
    movement = ReconcileMovement({ 0, 0, 0 }, desired, { 5.0f, 2.0f });
    assert(!movement.accepted);
}
