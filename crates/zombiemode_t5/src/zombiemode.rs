//! `maps\_zombiemode.gsc`

use gsc_threads::{Cx, Owner, Thread, Yield, call};

use crate::Level;
use crate::level::{EngineCommand, PlayerScore};
use crate::utility::{SpawnFailed, ZombieVar, set_zombie_var, spawn_zombie};

/// `level notify( "intermission" )`: the game is over.
pub const INTERMISSION: &str = "intermission";
/// `level notify( "end_of_round" )`.
pub const END_OF_ROUND: &str = "end_of_round";
/// `level notify( "restart_round" )`.
pub const RESTART_ROUND: &str = "restart_round";

/// Set by the engine side once every expected player is in (see
/// [`crate::ZombiesMode`]); `maps\_callbackglobal.gsc::synchronize_players`
/// gives its meaning: connected players == expected players.
pub const ALL_PLAYERS_CONNECTED: &str = "all_players_connected";

/// `difficulty = 1; column = int(difficulty) + 1;` in `init_levelvars` and
/// `difficulty_init`: retail zombies always reads the MEDIUM column.
pub const DIFFICULTY_COLUMN: usize = 2;

/// `flag( "spawn_zombies" )`: `round_spawning` spawns only while it is set.
pub const SPAWN_ZOMBIES: &str = "spawn_zombies";

/// `level.zombie_ai_limit`, as `main` sets it: the most zombies alive at once.
pub const ZOMBIE_AI_LIMIT: i32 = 24;

/// `maps\_zombiemode.gsc::init_flags`, with `flag_init( "spawn_zombies", true )`.
pub const INIT_FLAGS: &[(&str, bool)] = &[
    ("spawn_point_override", false),
    ("power_on", false),
    ("crawler_round", false),
    (SPAWN_ZOMBIES, true),
    ("dog_round", false),
    ("begin_spawning", false),
    ("end_round_wait", false),
    ("wait_and_revive", false),
    ("instant_revive", false),
];

/// `maps\_zombiemode.gsc::init_levelvars`.
pub fn init_levelvars(level: &mut Level) {
    use ZombieVar::{Float, Int};
    level.first_round = true;
    level.round_number = 1;
    level.intermission = false;
    level.zombie_total = 0;
    level.total_zombies_killed = 0;
    level.zombie_move_speed = 1;
    level.enemy_spawns.clear();
    let column = DIFFICULTY_COLUMN;
    // AI
    set_zombie_var(level, "zombie_health_increase", Int(100), false, column);
    set_zombie_var(
        level,
        "zombie_health_increase_multiplier",
        Float(0.1),
        true,
        column,
    );
    set_zombie_var(level, "zombie_health_start", Int(150), false, column);
    set_zombie_var(level, "zombie_spawn_delay", Float(2.0), true, column);
    set_zombie_var(level, "zombie_new_runner_interval", Int(10), false, column);
    set_zombie_var(level, "zombie_move_speed_multiplier", Int(8), false, column);
    set_zombie_var(level, "zombie_max_ai", Int(24), false, column);
    set_zombie_var(level, "zombie_ai_per_player", Int(6), false, column);
    set_zombie_var(level, "below_world_check", Int(-1000), false, 1);
    // Round
    set_zombie_var(level, "spectators_respawn", Int(1), false, 1);
    set_zombie_var(level, "zombie_use_failsafe", Int(1), false, 1);
    set_zombie_var(level, "zombie_between_round_time", Int(10), false, 1);
    set_zombie_var(level, "zombie_intermission_time", Int(15), false, 1);
    set_zombie_var(level, "game_start_delay", Int(0), false, column);
    // Life and death
    set_zombie_var(level, "penalty_no_revive", Float(0.10), true, column);
    set_zombie_var(level, "penalty_died", Float(0.0), true, column);
    set_zombie_var(level, "penalty_downed", Float(0.05), true, column);
    set_zombie_var(level, "starting_lives", Int(1), false, column);
    let start = format!("zombie_score_start_{}p", level.players.len());
    set_zombie_var(level, &start, Int(3000), false, column);
    for (var, value) in [
        ("zombie_score_kill_4player", 50),
        ("zombie_score_kill_3player", 50),
        ("zombie_score_kill_2player", 50),
        ("zombie_score_kill_1player", 50),
        ("zombie_score_kill_4p_team", 30),
        ("zombie_score_kill_3p_team", 35),
        ("zombie_score_kill_2p_team", 45),
        ("zombie_score_kill_1p_team", 0),
        ("zombie_score_damage_normal", 10),
        ("zombie_score_damage_light", 10),
        ("zombie_score_bonus_melee", 80),
        ("zombie_score_bonus_head", 50),
        ("zombie_score_bonus_neck", 20),
        ("zombie_score_bonus_torso", 10),
        ("zombie_score_bonus_burn", 10),
        ("zombie_flame_dmg_point_delay", 500),
        ("zombify_player", 0),
    ] {
        set_zombie_var(level, var, Int(value), false, 1);
    }
}

/// `maps\_zombiemode.gsc::ai_calculate_health`.
pub fn ai_calculate_health(level: &Level, round_number: i32) -> i32 {
    let mut health = level.zombie_var("zombie_health_start").as_i32();
    let increase = level.zombie_var("zombie_health_increase").as_f32();
    let multiplier = level
        .zombie_var("zombie_health_increase_multiplier")
        .as_f32();
    for i in 2..=round_number {
        // After round 10, get exponentially harder
        if i >= 10 {
            health += (health as f32 * multiplier) as i32;
        } else {
            health = (health as f32 + increase) as i32;
        }
    }
    health
}

/// The zombie count `round_spawning` sets for a round, before
/// `level.max_zombie_func`.
pub fn round_spawning_max(level: &Level, round_number: i32, player_num: usize) -> i32 {
    let mut max = level.zombie_var("zombie_max_ai").as_i32();
    let per_player = level.zombie_var("zombie_ai_per_player").as_f32();
    let mut multiplier = round_number as f32 / 5.0;
    if multiplier < 1.0 {
        multiplier = 1.0;
    }
    // After round 10, exponentially have more AI attack the player
    if round_number >= 10 {
        multiplier *= round_number as f32 * 0.15;
    }
    if player_num == 1 {
        max += (0.5 * per_player * multiplier) as i32;
    } else {
        max += ((player_num as f32 - 1.0) * per_player * multiplier) as i32;
    }
    max
}

/// `maps\_zombiemode.gsc::default_max_zombie_func`.
pub fn default_max_zombie_func(first_round: bool, round_number: i32, max_num: i32) -> i32 {
    let scale = if first_round {
        0.25
    } else if round_number < 3 {
        0.3
    } else if round_number < 4 {
        0.5
    } else if round_number < 5 {
        0.7
    } else if round_number < 6 {
        0.9
    } else {
        return max_num;
    };
    (max_num as f32 * scale) as i32
}

/// `maps\_zombiemode.gsc::main`, the parts ported so far.
#[derive(Clone, Debug, Default)]
pub struct Main;

impl Thread<Level> for Main {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        init_levelvars(cx.world);
        cx.world.zombie_ai_limit = ZOMBIE_AI_LIMIT;
        crate::powerups::init_vars(cx.world);
        for (flag, set) in INIT_FLAGS {
            if *set {
                cx.flag_set(*flag);
            } else {
                cx.flag_clear(*flag);
            }
        }
        cx.thread(Owner::LEVEL, PostAllPlayersConnected::default());
        Yield::Done
    }
}

/// `maps\_zombiemode.gsc::post_all_players_connected`.
#[derive(Clone, Debug, Default)]
pub struct PostAllPlayersConnected {
    waited: bool,
}

impl Thread<Level> for PostAllPlayersConnected {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        if !self.waited {
            self.waited = true;
            return Yield::FlagWait(ALL_PLAYERS_CONNECTED.into());
        }
        let line = format!(
            "sessions: mapname={} gametype zom isserver 1 player_count={}",
            cx.world.script,
            cx.world.players.len()
        );
        cx.world.println(line);
        // `maps\_zombiemode_score::init` only builds the team pools, whose
        // points retail no longer adds (`add_to_team_score` is commented out).
        cx.thread(Owner::LEVEL, DifficultyInit::default());
        cx.thread(Owner::LEVEL, RoundStart::default());
        Yield::Done
    }
}

/// `maps\_zombiemode.gsc::difficulty_init` (difficulty 1: no changes after the
/// starting points).
#[derive(Clone, Debug, Default)]
pub struct DifficultyInit {
    waited: bool,
}

impl Thread<Level> for DifficultyInit {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        if !self.waited {
            self.waited = true;
            return Yield::FlagWait(ALL_PLAYERS_CONNECTED.into());
        }
        let level = &mut *cx.world;
        let start = format!("zombie_score_start_{}p", level.players.len());
        let points = set_zombie_var(
            level,
            &start,
            ZombieVar::Int(3000),
            false,
            DIFFICULTY_COLUMN,
        );
        for player in level.players.clone() {
            level.scores.insert(
                player.entnum,
                PlayerScore {
                    score: points.as_i32(),
                    score_total: points.as_i32(),
                    old_score: points.as_i32(),
                },
            );
        }
        Yield::Done
    }
}

/// `maps\_zombiemode.gsc::round_start`.
#[derive(Clone, Debug, Default)]
pub struct RoundStart {
    pc: u8,
}

impl Thread<Level> for RoundStart {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        loop {
            match self.pc {
                0 => {
                    self.pc = 1;
                    // No level.round_prestart_func on the ported maps.
                    return Yield::wait_seconds(2.0);
                }
                1 => {
                    let level = &mut *cx.world;
                    level.zombie_health = level.zombie_var("zombie_health_start").as_i32();
                    // So players get init'ed with grenades.
                    for player in level.players.clone() {
                        if let Some(lethal) = player.lethal {
                            level.commands.push(EngineCommand::SetWeaponAmmoClip {
                                entnum: player.entnum,
                                weapon: lethal.weapon,
                                clip: 0,
                            });
                        }
                    }
                    self.pc = 2;
                    let delay = level.zombie_var("game_start_delay").as_i32();
                    if delay > 0 {
                        // `round_pause`: its countdown hud waits 2 + 3 s, then
                        // one second per count, then 1 s.
                        return Yield::wait_seconds(2.0 + 3.0 + delay as f32 + 1.0);
                    }
                }
                _ => {
                    cx.flag_set("begin_spawning");
                    cx.thread(Owner::LEVEL, RoundThink::default());
                    return Yield::Done;
                }
            }
        }
    }
}

/// `maps\_zombiemode.gsc::round_think`.
#[derive(Clone, Debug, Default)]
pub struct RoundThink {
    pc: u8,
    chalk_one_up: ChalkOneUp,
    round_wait: RoundWait,
    chalk_round_over: ChalkRoundOver,
}

impl Thread<Level> for RoundThink {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        loop {
            match self.pc {
                0 => {
                    let level = &mut *cx.world;
                    let max_reward = (50 * level.round_number).min(500);
                    level.zombie_vars.insert(
                        "rebuild_barrier_cap_per_round".into(),
                        ZombieVar::Int(max_reward),
                    );
                    self.chalk_one_up = ChalkOneUp::default();
                    self.pc = 1;
                }
                1 => {
                    if let Some(wait) = call(&mut self.chalk_one_up, cx) {
                        return wait;
                    }
                    award_grenades_for_survivors(cx.world);
                    let line = format!(
                        "zombie_rounds: round {} player_count {}",
                        cx.world.round_number,
                        cx.world.players.len()
                    );
                    cx.world.println(line);
                    cx.thread(Owner::LEVEL, RoundSpawning::default());
                    cx.notify(Owner::LEVEL, "start_of_round", Vec::new());
                    self.round_wait = RoundWait::default();
                    self.pc = 2;
                }
                2 => {
                    if let Some(wait) = call(&mut self.round_wait, cx) {
                        return wait;
                    }
                    cx.world.first_round = false;
                    cx.notify(Owner::LEVEL, END_OF_ROUND, Vec::new());
                    self.chalk_round_over = ChalkRoundOver::default();
                    self.pc = 3;
                }
                3 => {
                    if let Some(wait) = call(&mut self.chalk_round_over, cx) {
                        return wait;
                    }
                    let level = &mut *cx.world;
                    let timer = level.zombie_var("zombie_spawn_delay").as_f32();
                    if timer > 0.08 {
                        level
                            .zombie_vars
                            .insert("zombie_spawn_delay".into(), ZombieVar::Float(timer * 0.95));
                    } else if timer < 0.08 {
                        level
                            .zombie_vars
                            .insert("zombie_spawn_delay".into(), ZombieVar::Float(0.08));
                    }
                    level.zombie_move_speed = level.round_number
                        * level.zombie_var("zombie_move_speed_multiplier").as_i32();
                    level.round_number += 1;
                    cx.notify(Owner::LEVEL, "between_round_over", Vec::new());
                    self.pc = 0;
                }
                _ => unreachable!("round_think has four steps"),
            }
        }
    }
}

/// `maps\_zombiemode.gsc::award_grenades_for_survivors`.
pub fn award_grenades_for_survivors(level: &mut Level) {
    for player in level.players.clone() {
        let Some(lethal) = player.lethal else {
            continue;
        };
        let fraction = lethal.fraction_max_ammo();
        let clip = if fraction < 0.25 {
            2
        } else if fraction < 0.5 {
            3
        } else {
            4
        };
        level.commands.push(EngineCommand::SetWeaponAmmoClip {
            entnum: player.entnum,
            weapon: lethal.weapon,
            clip,
        });
    }
}

/// `maps\_zombiemode.gsc::chalk_one_up`: the round-number chalk. Its waits pace
/// the round start; the hud itself is ZMB-038.
#[derive(Clone, Debug, Default)]
pub struct ChalkOneUp {
    pc: u8,
}

impl Thread<Level> for ChalkOneUp {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        let intro = cx.world.first_round;
        // intro: fade in 1 s, to red 2 s, hold 3 s, 0.25 s, slide 2 s;
        // otherwise: fade out 0.5 s, hold 2 s.
        let waits: &[f32] = if intro {
            &[1.0, 2.0, 3.0, 0.25, 2.0]
        } else {
            &[0.5, 2.0]
        };
        let step = usize::from(self.pc);
        self.pc += 1;
        if step == 4 && intro {
            cx.notify(Owner::LEVEL, "intro_hud_done", Vec::new());
        }
        match waits.get(step) {
            Some(seconds) => Yield::wait_seconds(*seconds),
            None => Yield::Done,
        }
    }
}

/// `maps\_zombiemode.gsc::chalk_round_over`: the chalk pulse between rounds,
/// `zombie_between_round_time` long.
#[derive(Clone, Debug, Default)]
pub struct ChalkRoundOver {
    pc: u32,
}

impl Thread<Level> for ChalkRoundOver {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        let mut time = cx.world.zombie_var("zombie_between_round_time").as_f32();
        if time > 3.0 {
            time -= 2.0; // add this deduction back in at the bottom
        }
        let fade_time = 0.5;
        let steps = (time * 0.5) / fade_time;
        // `for( q = 0; q < steps; q++ )`: fade out, wait, fade in, wait.
        let pulses = steps.max(0.0).ceil() as u32;
        let step = self.pc;
        self.pc += 1;
        if step < pulses * 2 {
            Yield::wait_seconds(fade_time)
        } else if step == pulses * 2 {
            Yield::wait_seconds(2.0)
        } else {
            Yield::Done
        }
    }
}

/// `maps\_zombiemode.gsc::round_wait`.
#[derive(Clone, Debug, Default)]
pub struct RoundWait {
    pc: u8,
}

impl Thread<Level> for RoundWait {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        if self.pc == 0 {
            self.pc = 1;
            return Yield::wait_seconds(1.0);
        }
        // Dog rounds are not ported (no dogs on the ported maps yet).
        let level = &*cx.world;
        if level.enemy_count() > 0 || level.zombie_total > 0 || level.intermission {
            if cx.flag("end_round_wait") {
                return Yield::Done;
            }
            return Yield::wait_seconds(1.0);
        }
        Yield::Done
    }
}

/// `maps\_zombiemode.gsc::round_spawning`, zombies only: dog rounds, mixed
/// spawns and `zombie_speed_up` come with dogs and runners.
#[derive(Clone, Debug, Default)]
pub struct RoundSpawning {
    pc: u8,
    old_spawn: Option<u32>,
}

impl Thread<Level> for RoundSpawning {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        loop {
            match self.pc {
                0 => {
                    for event in [INTERMISSION, END_OF_ROUND, RESTART_ROUND] {
                        cx.endon(Owner::LEVEL, event);
                    }
                    let level = &mut *cx.world;
                    if level.intermission {
                        return Yield::Done;
                    }
                    if level.enemy_spawns.is_empty() {
                        level.println(
                            "ASSERTMSG: No active spawners in the map.  Check to see if the zone is active and if it's pointing to spawners.",
                        );
                        return Yield::Done;
                    }
                    level.zombie_health = ai_calculate_health(level, level.round_number);
                    let max = round_spawning_max(level, level.round_number, level.players.len());
                    level.zombie_total =
                        default_max_zombie_func(level.first_round, level.round_number, max);
                    self.pc = 1;
                }
                1 => {
                    let level = &*cx.world;
                    if level.enemy_count() >= level.zombie_ai_limit || level.zombie_total <= 0 {
                        return Yield::wait_seconds(0.1);
                    }
                    self.pc = 2;
                    if !cx.flag(SPAWN_ZOMBIES) {
                        return Yield::FlagWait(SPAWN_ZOMBIES.into());
                    }
                }
                2 => {
                    let level = &mut *cx.world;
                    // The zone manager rebuilds the list every second; an
                    // empty one waits for the next rebuild.
                    if level.enemy_spawns.is_empty() {
                        self.pc = 1;
                        return Yield::wait_seconds(0.1);
                    }
                    let count = level.enemy_spawns.len() as i32;
                    let pick = level.random_int(count) as usize;
                    let mut spawn_point = level.enemy_spawns[pick];
                    if self.old_spawn == Some(spawn_point) {
                        let pick = level.random_int(count) as usize;
                        spawn_point = level.enemy_spawns[pick];
                    }
                    self.old_spawn = Some(spawn_point);
                    match spawn_zombie(level, spawn_point) {
                        // Retail retries a failed spawn; a type the port
                        // cannot spawn never succeeds, so it counts as
                        // spawned and the round can still end (with fewer
                        // real zombies than `zombie_total` said).
                        Ok(_) | Err(SpawnFailed::UnknownAiType) => level.zombie_total -= 1,
                        Err(SpawnFailed::NoSpawner) => {}
                    }
                    self.pc = 3;
                    return Yield::wait_seconds(level.zombie_var("zombie_spawn_delay").as_f32());
                }
                3 => {
                    self.pc = 1;
                    return crate::utility::wait_network_frame();
                }
                _ => unreachable!("round_spawning has four steps"),
            }
        }
    }
}
