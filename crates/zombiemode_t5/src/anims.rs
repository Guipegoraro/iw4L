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

/// `maps\_zombiemode_spawner.gsc::set_run_speed`: a roll in
/// `level.zombie_move_speed .. +35` picks walk (≤ 35), run (≤ 70) or sprint.
pub fn set_run_speed(level: &mut Level) -> MoveSpeed {
    // `RandomIntRange( min, max )` is `min .. max - 1`.
    let roll = level.zombie_move_speed + level.random_int(35);
    if roll <= 35 {
        MoveSpeed::Walk
    } else if roll <= 70 {
        MoveSpeed::Run
    } else {
        MoveSpeed::Sprint
    }
}

/// `maps\_zombiemode_spawner.gsc::set_zombie_run_cycle`: the move clip for
/// `speed` (`RandomIntRange( 1, 8 )` over the walks, `( 1, 6 )` over the
/// runs, `( 1, 4 )` over the sprints).
pub fn set_zombie_run_cycle(level: &mut Level, speed: MoveSpeed) -> &'static str {
    let pool: &[&'static str] = match speed {
        MoveSpeed::Walk => &WALK[..7],
        MoveSpeed::Run => &RUN[..5],
        MoveSpeed::Sprint => &SPRINT[..3],
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
