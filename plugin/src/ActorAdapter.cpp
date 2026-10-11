#include "ActorAdapter.h"

#include <vector>

namespace
{
    ErVec3 ToEr(const RE::NiPoint3& point)
    {
        return { point.x, point.y, point.z };
    }

    BridgePlan::TargetCandidate ReadCandidate(RE::PlayerCharacter* player, RE::Actor* actor)
    {
        BridgePlan::TargetCandidate candidate{};
        if (!player || !actor || actor == player) {
            return candidate;
        }

        candidate.stableId = actor->GetFormID();
        candidate.hostile = actor->IsHostileToActor(player);
        candidate.alive = !actor->IsDead();
        candidate.enabled = !actor->IsDisabled();
        candidate.loaded = actor->Is3DLoaded();
        if (!candidate.hostile || !candidate.alive || !candidate.enabled || !candidate.loaded) {
            return candidate;
        }
        candidate.bounds = BridgePlan::ActorCapsule(
            ToEr(actor->GetPosition()),
            ToEr(actor->GetBoundMin()),
            ToEr(actor->GetBoundMax()),
            actor->GetHeight());
        return candidate;
    }
}

namespace ActorAdapter
{
    BridgePlan::TargetSelection SelectSweepTarget(RE::PlayerCharacter* player, const ErOutput& swing)
    {
        std::vector<BridgePlan::TargetCandidate> candidates;
        auto* processes = RE::ProcessLists::GetSingleton();
        if (!player || !processes) {
            return {};
        }

        processes->ForEachHighActor([&](RE::Actor* actor) {
            candidates.push_back(ReadCandidate(player, actor));
            return RE::BSContainer::ForEachResult::kContinue;
        });
        return BridgePlan::SelectSweepTarget(swing, candidates);
    }
}
