#pragma once

#include "BridgePlan.h"

namespace ActorAdapter
{
    // Reads high-process actors into plain candidates, then delegates all
    // ordering and geometry to the native-testable bridge plan.
    BridgePlan::TargetSelection SelectSweepTarget(RE::PlayerCharacter* player, const ErOutput& swing);
}
