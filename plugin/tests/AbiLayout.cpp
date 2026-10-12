// Prints the C side of the ABI as JSON, in the same shape as the Rust
// `abi_layout` binary. tools/abi_gate.py compiles and runs this against
// core/include/er_combat.h, then compares it with the Rust output.
//
// The prototypes are pinned with static_assert so a header signature that
// drifts from the shapes below stops the build.
#include "er_combat.h"

#include <cstddef>
#include <cstdio>
#include <type_traits>

static bool first_item = true;
static void sep() {
    std::printf("%s", first_item ? "" : ",");
    first_item = false;
}

#define FIELD(T, f) std::printf("%s\"%s\":%zu", (field_first ? "" : ","), #f, offsetof(T, f)), field_first = false
#define BEGIN_STRUCT(T)                                                             \
    sep();                                                                          \
    std::printf("\"%s\":{\"size\":%zu,\"align\":%zu,\"fields\":{", #T, sizeof(T), alignof(T)); \
    field_first = true
#define END_STRUCT() std::printf("}}")
#define VALUE(prefix, name) std::printf("%s\"%s\":%u", (field_first ? "" : ","), #name, (unsigned)prefix##name), field_first = false
#define BEGIN_ENUM(T)                                                \
    sep();                                                           \
    std::printf("\"%s\":{\"size\":%zu,\"values\":{", #T, sizeof(T)); \
    field_first = true
#define END_ENUM() std::printf("}}")

static bool field_first = true;

// Compile-time proof that the prototypes have the shapes the Rust side exports.
static_assert(std::is_same_v<decltype(&er_create), ErCombat* (*)(ErVec3)>);
static_assert(std::is_same_v<decltype(&er_destroy), void (*)(ErCombat*)>);
static_assert(std::is_same_v<decltype(&er_update), void (*)(ErCombat*, float, const ErInput*, const ErWorld*, ErOutput*)>);
static_assert(std::is_same_v<decltype(&er_receive_hit), ErHitResult (*)(ErCombat*, ErVec3, uint8_t, uint32_t)>);
static_assert(std::is_same_v<decltype(&er_confirm_hit), void (*)(ErCombat*)>);

int main() {
    std::printf("{\"structs\":{");
    BEGIN_STRUCT(ErVec3); FIELD(ErVec3, x); FIELD(ErVec3, y); FIELD(ErVec3, z); END_STRUCT();
    BEGIN_STRUCT(ErButton); FIELD(ErButton, held); FIELD(ErButton, pressed); FIELD(ErButton, released); END_STRUCT();
    BEGIN_STRUCT(ErInput);
    FIELD(ErInput, move_x); FIELD(ErInput, move_y); FIELD(ErInput, camera_heading);
    FIELD(ErInput, dodge); FIELD(ErInput, jump); FIELD(ErInput, light); FIELD(ErInput, heavy); FIELD(ErInput, guard);
    FIELD(ErInput, crouch); FIELD(ErInput, walk); FIELD(ErInput, lock);
    FIELD(ErInput, two_hand_right); FIELD(ErInput, two_hand_left);
    END_STRUCT();
    BEGIN_STRUCT(ErWorld);
    FIELD(ErWorld, position); FIELD(ErWorld, ground_z); FIELD(ErWorld, has_target); FIELD(ErWorld, target);
    FIELD(ErWorld, in_combat); FIELD(ErWorld, stamina_current); FIELD(ErWorld, stamina_max);
    END_STRUCT();
    BEGIN_STRUCT(ErOutput);
    FIELD(ErOutput, position); FIELD(ErOutput, heading); FIELD(ErOutput, state); FIELD(ErOutput, action);
    FIELD(ErOutput, action_frame); FIELD(ErOutput, action_kind); FIELD(ErOutput, action_dir);
    FIELD(ErOutput, action_serial); FIELD(ErOutput, speed); FIELD(ErOutput, sprinting); FIELD(ErOutput, crouching);
    FIELD(ErOutput, guarding); FIELD(ErOutput, invincible); FIELD(ErOutput, airborne);
    FIELD(ErOutput, interruptible); FIELD(ErOutput, stamina); FIELD(ErOutput, hit_live);
    FIELD(ErOutput, sweep_a); FIELD(ErOutput, sweep_b); FIELD(ErOutput, hit_radius);
    FIELD(ErOutput, damage_multiplier); FIELD(ErOutput, stance_multiplier); FIELD(ErOutput, hit_stop);
    FIELD(ErOutput, stamina_spent);
    END_STRUCT();

    first_item = true;
    std::printf("},\"enums\":{");
    BEGIN_ENUM(ErState);
    VALUE(ER_STATE_, GROUND); VALUE(ER_STATE_, ACTION); VALUE(ER_STATE_, AIR); VALUE(ER_STATE_, DEAD);
    END_ENUM();
    BEGIN_ENUM(ErAction);
    VALUE(ER_ACTION_, NONE); VALUE(ER_ACTION_, ROLL); VALUE(ER_ACTION_, CROUCH_ROLL); VALUE(ER_ACTION_, BACKSTEP);
    VALUE(ER_ACTION_, SPRINT_STOP); VALUE(ER_ACTION_, JUMP); VALUE(ER_ACTION_, LAND);
    VALUE(ER_ACTION_, LIGHT_ATTACK); VALUE(ER_ACTION_, HEAVY_CHARGE); VALUE(ER_ACTION_, HEAVY_ATTACK);
    VALUE(ER_ACTION_, RUN_ATTACK); VALUE(ER_ACTION_, ROLL_ATTACK); VALUE(ER_ACTION_, BACKSTEP_ATTACK);
    VALUE(ER_ACTION_, CROUCH_ATTACK); VALUE(ER_ACTION_, JUMP_ATTACK_LAND); VALUE(ER_ACTION_, GUARD_COUNTER);
    VALUE(ER_ACTION_, LEFT_ATTACK); VALUE(ER_ACTION_, GUARD_HIT); VALUE(ER_ACTION_, GUARD_BREAK);
    VALUE(ER_ACTION_, HURT); VALUE(ER_ACTION_, COUNT);
    END_ENUM();
    BEGIN_ENUM(ErHurt);
    VALUE(ER_HURT_, SMALL); VALUE(ER_HURT_, MIDDLE); VALUE(ER_HURT_, LARGE); VALUE(ER_HURT_, KNOCKDOWN); VALUE(ER_HURT_, COUNT);
    END_ENUM();
    BEGIN_ENUM(ErHitResult);
    VALUE(ER_HIT_, IGNORED); VALUE(ER_HIT_, DODGED); VALUE(ER_HIT_, JUMPED); VALUE(ER_HIT_, BLOCKED);
    VALUE(ER_HIT_, GUARD_BROKEN); VALUE(ER_HIT_, HIT);
    END_ENUM();
    std::printf("}}\n");
    return 0;
}
