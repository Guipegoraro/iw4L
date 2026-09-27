//! Black Ops zombies rules (`zombiemode_t5`) against the numbers the retail
//! scripts produce: zombie health and count per round, points per event, and
//! how `mp/zombiemode.csv` overrides a script default. The table here is a
//! made-up one with the retail layout, not the game's.

use sim::{ClientId, ScriptPlayer};
use zombiemode_t5::score::{PointsEvent, player_points};
use zombiemode_t5::utility::{ZombieVar, gsc_float, gsc_int, round_up_score, set_zombie_var};
use zombiemode_t5::zombiemode::{
    DIFFICULTY_COLUMN, ai_calculate_health, default_max_zombie_func, init_levelvars,
    round_spawning_max,
};
use zombiemode_t5::{Level, StringTables};

fn players(n: usize) -> Vec<ScriptPlayer> {
    (0..n)
        .map(|i| ScriptPlayer {
            client: ClientId(i as u32),
            entnum: i as i32,
            alive: true,
            origin: [0.0; 3],
            team: 2,
            lethal: None,
        })
        .collect()
}

/// `init_levelvars` and the powerups' vars on an empty table: the script
/// defaults.
fn level_with(n: usize, tables: StringTables) -> Level {
    let mut level = Level::new(
        "zombie_test",
        tables,
        Default::default(),
        Default::default(),
    );
    level.players = players(n);
    init_levelvars(&mut level);
    zombiemode_t5::powerups::init_vars(&mut level);
    level
}

fn zombie_count(level: &Level, round: i32, n: usize) -> i32 {
    let max = round_spawning_max(level, round, n);
    default_max_zombie_func(round == 1, round, max)
}

#[test]
fn zombie_health_per_round_follows_ai_calculate_health() {
    let level = level_with(1, StringTables::default());
    let health: Vec<i32> = (1..=12).map(|r| ai_calculate_health(&level, r)).collect();
    assert_eq!(
        health,
        [
            150, 250, 350, 450, 550, 650, 750, 850, 950, 1045, 1149, 1263
        ]
    );
}

#[test]
fn solo_zombie_count_per_round_follows_round_spawning() {
    let level = level_with(1, StringTables::default());
    let counts: Vec<i32> = (1..=11).map(|r| zombie_count(&level, r, 1)).collect();
    assert_eq!(counts, [6, 8, 13, 18, 24, 27, 28, 28, 29, 33, 34]);
}

#[test]
fn coop_zombie_count_scales_with_players() {
    let level = level_with(4, StringTables::default());
    let four: Vec<i32> = [1, 5, 10]
        .iter()
        .map(|&r| zombie_count(&level, r, 4))
        .collect();
    // 24 + 3 * 6 * multiplier, then the early-round scale.
    assert_eq!(four, [10, 37, 78]);
    let two = zombie_count(&level, 6, 2);
    assert_eq!(two, 31);
}

#[test]
fn points_for_hits_and_kills_by_hit_location() {
    let level = level_with(1, StringTables::default());
    let kill = |mod_: &str, loc: &str| {
        player_points(
            &level,
            PointsEvent::Death {
                means_of_death: mod_,
                hit_location: loc,
            },
        )
    };
    assert_eq!(kill("MOD_PISTOL_BULLET", "head"), 100);
    assert_eq!(kill("MOD_PISTOL_BULLET", "neck"), 70);
    assert_eq!(kill("MOD_RIFLE_BULLET", "torso_upper"), 60);
    assert_eq!(kill("MOD_RIFLE_BULLET", "left_leg_lower"), 50);
    assert_eq!(kill("MOD_MELEE", "head"), 130);
    assert_eq!(player_points(&level, PointsEvent::Damage), 10);
    assert_eq!(player_points(&level, PointsEvent::DamageLight), 10);
    // int( 10 * 1.25 ) is 12, rounded up to a multiple of 5.
    assert_eq!(player_points(&level, PointsEvent::DamageAds), 15);
}

#[test]
fn round_up_score_rounds_up_to_the_multiple() {
    assert_eq!(round_up_score(12, 5), 15);
    assert_eq!(round_up_score(15, 5), 15);
    assert_eq!(round_up_score(0, 10), 0);
    assert_eq!(round_up_score(101, 10), 110);
}

#[test]
fn a_table_value_in_the_difficulty_column_overrides_the_script_default() {
    let csv = "NAME,EASY,MEDIUM,HARD,VETERAN\n\
               zombie_score_start_1p,111,222,333,444\n\
               zombie_health_increase_multiplier,0.5,0.25,,\n\
               zombie_score_kill_1p_team,7,,,\n";
    let tables = StringTables::from_csv([("mp/zombiemode.csv", csv)]);
    let mut level = level_with(1, tables);
    assert_eq!(
        level.zombie_var("zombie_score_start_1p"),
        ZombieVar::Int(222)
    );
    assert_eq!(
        level.zombie_var("zombie_health_increase_multiplier"),
        ZombieVar::Float(0.25)
    );
    // An empty cell in the column keeps the script's default.
    let team = set_zombie_var(
        &mut level,
        "zombie_score_kill_1p_team",
        ZombieVar::Int(0),
        false,
        DIFFICULTY_COLUMN,
    );
    assert_eq!(team, ZombieVar::Int(0));
    // A var absent from the table keeps the default too.
    assert_eq!(level.zombie_var("zombie_max_ai"), ZombieVar::Int(24));
}

#[test]
fn gsc_int_and_float_parse_like_retail() {
    assert_eq!(gsc_int("2.0"), 2);
    assert_eq!(gsc_int("-15"), -15);
    assert_eq!(gsc_int(""), 0);
    assert_eq!(gsc_float("0.075"), 0.075);
    assert_eq!(gsc_float("2"), 2.0);
}
