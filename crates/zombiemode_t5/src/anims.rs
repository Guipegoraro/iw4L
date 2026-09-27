//! `maps\_zombiemode.gsc::init_anims`: the clips a zombie walks, runs,
//! sprints, idles and attacks with (`level.scr_anim["zombie"]`,
//! `level._zombie_melee`, `level._zombie_walk_melee`,
//! `level._zombie_run_melee`), and `anim.idleAnimArray["stand"]`.
//! Crawlers, deaths and traversals come with the cards that use them.

use crate::Level;

/// `level.scr_anim["zombie"]["walk1".."walk8"]`.
pub const WALK: [&str; 8] = [
    "ai_zombie_walk_v1",
    "ai_zombie_walk_v2",
    "ai_zombie_walk_v3",
    "ai_zombie_walk_v4",
    "ai_zombie_walk_v6",
    "ai_zombie_walk_v7",
    "ai_zombie_walk_v9",
    "ai_zombie_walk_v9",
];

/// `level.scr_anim["zombie"]["run1".."run6"]`.
pub const RUN: [&str; 6] = [
    "ai_zombie_walk_fast_v1",
    "ai_zombie_walk_fast_v2",
    "ai_zombie_walk_fast_v3",
    "ai_zombie_run_v2",
    "ai_zombie_run_v4",
    "ai_zombie_run_v3",
];

/// `level.scr_anim["zombie"]["sprint1".."sprint4"]`.
pub const SPRINT: [&str; 4] = [
    "ai_zombie_sprint_v1",
    "ai_zombie_sprint_v2",
    "ai_zombie_sprint_v1",
    "ai_zombie_sprint_v2",
];

/// `level._zombie_melee["zombie"]`.
pub const MELEE: [&str; 6] = [
    "ai_zombie_attack_v2",
    "ai_zombie_attack_v4",
    "ai_zombie_attack_v6",
    "ai_zombie_attack_v1",
    "ai_zombie_attack_forward_v1",
    "ai_zombie_attack_forward_v2",
];

/// `level._zombie_walk_melee["zombie"]`.
pub const WALK_MELEE: [&str; 4] = [
    "ai_zombie_walk_attack_v1",
    "ai_zombie_walk_attack_v2",
    "ai_zombie_walk_attack_v3",
    "ai_zombie_walk_attack_v4",
];

/// `level._zombie_run_melee["zombie"]`.
pub const RUN_MELEE: [&str; 3] = [
    "ai_zombie_run_attack_v1",
    "ai_zombie_run_attack_v2",
    "ai_zombie_run_attack_v3",
];

/// `anim.idleAnimArray["stand"][0][0]`.
pub const IDLE: &str = "ai_zombie_idle_v1_delta";

/// The clips the host decodes for zombie actors.
pub fn actor_clips() -> Vec<&'static str> {
    let mut clips: Vec<&'static str> = WALK
        .iter()
        .chain(&RUN)
        .chain(&SPRINT)
        .chain(&MELEE)
        .chain(&WALK_MELEE)
        .chain(&RUN_MELEE)
        .copied()
        .chain([IDLE])
        .collect();
    clips.sort_unstable();
    clips.dedup();
    clips
}

/// `self.zombie_move_speed`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MoveSpeed {
    #[default]
    Walk,
    Run,
    Sprint,
}

/// `set_run_speed`: `RandomIntRange( level.zombie_move_speed,
/// level.zombie_move_speed + 35 )`, the width of the roll.
pub const RUN_SPEED_ROLL: i32 = 35;
/// `set_run_speed`: a roll at or under this walks.
pub const WALK_MAX_ROLL: i32 = 35;
/// `set_run_speed`: a roll at or under this (and over the walk's) runs;
/// above it sprints.
pub const RUN_MAX_ROLL: i32 = 70;

/// `set_zombie_run_cycle`: `RandomIntRange( 1, 8 )`, walk1 to walk7.
pub const WALK_CYCLES: usize = 7;
/// `set_zombie_run_cycle`: `RandomIntRange( 1, 6 )`, run1 to run5.
pub const RUN_CYCLES: usize = 5;
/// `set_zombie_run_cycle`: `RandomIntRange( 1, 4 )`, sprint1 to sprint3.
pub const SPRINT_CYCLES: usize = 3;

/// `maps\_zombiemode_spawner.gsc::set_run_speed`: a roll in
/// `level.zombie_move_speed ..` [`RUN_SPEED_ROLL`] more picks walk, run or
/// sprint.
pub fn set_run_speed(level: &mut Level) -> MoveSpeed {
    // `RandomIntRange( min, max )` is `min .. max - 1`.
    let roll = level.zombie_move_speed + level.random_int(RUN_SPEED_ROLL);
    if roll <= WALK_MAX_ROLL {
        MoveSpeed::Walk
    } else if roll <= RUN_MAX_ROLL {
        MoveSpeed::Run
    } else {
        MoveSpeed::Sprint
    }
}

/// `maps\_zombiemode_spawner.gsc::set_zombie_run_cycle`: the move clip for
/// `speed`, among the first [`WALK_CYCLES`], [`RUN_CYCLES`] or
/// [`SPRINT_CYCLES`] of its list.
pub fn set_zombie_run_cycle(level: &mut Level, speed: MoveSpeed) -> &'static str {
    let pool: &[&'static str] = match speed {
        MoveSpeed::Walk => &WALK[..WALK_CYCLES],
        MoveSpeed::Run => &RUN[..RUN_CYCLES],
        MoveSpeed::Sprint => &SPRINT[..SPRINT_CYCLES],
    };
    pool[level.random_int(pool.len() as i32) as usize]
}

/// `animscripts\zombie_melee.gsc::pick_zombie_melee_anim`, for a zombie with
/// legs: the standing swipes plus the walk or run ones.
pub fn pick_zombie_melee_anim(level: &mut Level, speed: MoveSpeed) -> &'static str {
    let moving: &[&'static str] = match speed {
        MoveSpeed::Walk => &WALK_MELEE,
        MoveSpeed::Run | MoveSpeed::Sprint => &RUN_MELEE,
    };
    let count = MELEE.len() + moving.len();
    let pick = level.random_int(count as i32) as usize;
    MELEE
        .get(pick)
        .copied()
        .unwrap_or_else(|| moving[pick - MELEE.len()])
}
