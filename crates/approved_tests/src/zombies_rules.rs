//! Black Ops zombies rules (`zombiemode_t5`) against the numbers the retail
//! scripts produce: zombie health and count per round, points per event, and
//! how `mp/zombiemode.csv` overrides a script default. The table here is a
//! made-up one with the retail layout, not the game's.

use gsc_threads::{Cx, Owner, Scheduler, Thread, Yield};
use sim::{ClientId, ScriptPlayer};
use zombiemode_t5::actor::player_owner;
use zombiemode_t5::damage::{ZombieDamage, apply_zombie_damage};
use zombiemode_t5::level::EngineCommand;
use zombiemode_t5::score::{PointsEvent, player_points};
use zombiemode_t5::spawner::{ZOM_KILL, zombie_spawn_init_threads};
use zombiemode_t5::utility::{ZombieVar, gsc_float, gsc_int, round_up_score, set_zombie_var};
use zombiemode_t5::zombiemode::{
    DIFFICULTY_COLUMN, END_OF_ROUND, RoundThink, SPAWN_ZOMBIES, ZOMBIE_AI_LIMIT,
    ai_calculate_health, default_max_zombie_func, init_levelvars, round_spawning_max,
};
use zombiemode_t5::zone_manager::{enable_zone, occupied_volumes, player_in_zone, zone_init};
use zombiemode_t5::{Level, MapEnts, StringTables};

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

/// `owner waittill( "zom_kill" )` over and over, a log line each time.
#[derive(Clone, Debug)]
struct ZomKillSeen {
    owner: Owner,
    tag: &'static str,
    waiting: bool,
}

impl Thread<Level> for ZomKillSeen {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        if std::mem::replace(&mut self.waiting, true) {
            cx.world.println(self.tag);
        }
        Yield::waittill(self.owner, ZOM_KILL)
    }
}

/// `level waittill( "end_of_round" )`, then a line in the log.
#[derive(Clone, Debug, Default)]
struct EndOfRoundSeen {
    waiting: bool,
}

impl Thread<Level> for EndOfRoundSeen {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        if self.waiting {
            cx.world.println(END_OF_ROUND);
            return Yield::Done;
        }
        self.waiting = true;
        Yield::waittill(Owner::LEVEL, END_OF_ROUND)
    }
}

#[test]
fn a_round_whose_only_spawner_has_an_unknown_type_still_ends_with_one_warning() {
    const FRAME_MS: u64 = 50;
    // Round 1's handful, one per `zombie_spawn_delay`, takes far less.
    const GIVE_UP_MS: u64 = 60_000;
    const NOT_PORTED: &str = "actor_zombie_not_ported";
    let mut level = level_with(1, StringTables::default());
    level.ents = std::sync::Arc::new(MapEnts::parse(&format!(
        "{{ \"classname\" \"{NOT_PORTED}\" \"origin\" \"0 0 0\" }}"
    )));
    level.enemy_spawns = vec![0];
    level.zombie_ai_limit = ZOMBIE_AI_LIMIT;
    let mut threads = Scheduler::default();
    threads.flag_set(SPAWN_ZOMBIES);
    threads.spawn(Owner::LEVEL, RoundThink::default());
    threads.spawn(Owner::LEVEL, EndOfRoundSeen::default());
    let mut ms = 0;
    while !level.println.iter().any(|line| line == END_OF_ROUND) {
        assert!(
            ms < GIVE_UP_MS,
            "no end_of_round: {} zombies left to spawn",
            level.zombie_total
        );
        threads.run(ms, &mut level);
        ms += FRAME_MS;
    }
    assert_eq!(level.enemy_count(), 0);
    let warnings = level
        .println
        .iter()
        .filter(|line| line.contains(NOT_PORTED))
        .count();
    assert_eq!(warnings, 1, "{:?}", level.println);
}

#[test]
fn a_round_whose_zombies_all_die_ends_and_the_next_one_starts() {
    // One Nacht spawner and two players; player 1 kills every zombie (client
    // 1, so a player owner that collided with an entity number would show): odd ones in the frame they
    // are created (before the engine spawns them), even ones the frame after.
    // Round 1 ends through the real `round_think`, every kill counts, pays
    // and is notified on the level and on the killer, and round 2 begins.
    const FRAME_MS: u64 = 50;
    const GIVE_UP_MS: u64 = 120_000;
    const MOD: &str = "MOD_RIFLE_BULLET";
    const HIT: &str = "torso_upper";
    const LEVEL_KILL: &str = "level zom_kill";
    const PLAYER_KILL: &str = "player zom_kill";
    let player = ClientId(1);
    let mut level = level_with(2, StringTables::default());
    level.ents = std::sync::Arc::new(MapEnts::parse(
        "{ \"classname\" \"actor_zombie_ger_zombie\" \"origin\" \"0 0 0\" }",
    ));
    level.enemy_spawns = vec![0];
    level.zombie_ai_limit = ZOMBIE_AI_LIMIT;
    let round_one = default_max_zombie_func(true, 1, round_spawning_max(&level, 1, 2));
    let kill_points = player_points(
        &level,
        PointsEvent::Death {
            means_of_death: MOD,
            hit_location: HIT,
        },
    );
    let mut threads = Scheduler::default();
    threads.flag_set(SPAWN_ZOMBIES);
    threads.spawn(Owner::LEVEL, RoundThink::default());
    threads.spawn(Owner::LEVEL, EndOfRoundSeen::default());
    for (owner, tag) in [
        (Owner::LEVEL, LEVEL_KILL),
        (player_owner(player), PLAYER_KILL),
    ] {
        threads.spawn(
            owner,
            ZomKillSeen {
                owner,
                tag,
                waiting: false,
            },
        );
    }
    let shoot = |level: &mut Level, actor: u32| {
        let amount = level.zombie_health;
        level.pending_damage.push(ZombieDamage {
            actor,
            amount,
            attacker: Some(player),
            means_of_death: MOD,
            hit_location: HIT,
        });
    };

    let mut ms = 0;
    while level.round_number < 2 {
        assert!(
            ms < GIVE_UP_MS,
            "round 1 never ended: {} killed",
            level.total_zombies_killed
        );
        // In the mode's order: the frame's threads, the damage, then the
        // engine commands (a zombie already dead is never spawned).
        let standing: Vec<u32> = level.zombies.keys().copied().collect();
        threads.run(ms, &mut level);
        for &actor in &standing {
            shoot(&mut level, actor);
        }
        let fresh: Vec<u32> = level
            .zombies
            .keys()
            .copied()
            .filter(|actor| actor % 2 == 1 && !standing.contains(actor))
            .collect();
        for actor in fresh {
            shoot(&mut level, actor);
        }
        apply_zombie_damage(&mut level, &mut threads);
        for command in std::mem::take(&mut level.commands) {
            if let EngineCommand::SpawnActor { actor, .. } = command
                && level.zombies.contains_key(&actor)
            {
                zombie_spawn_init_threads(&mut threads, actor);
            }
        }
        ms += FRAME_MS;
    }

    assert!(level.println.iter().any(|line| line == END_OF_ROUND));
    assert_eq!(level.total_zombies_killed, round_one);
    assert_eq!(level.zombie_player_killed_count, round_one);
    assert_eq!(level.zombies_timeout_spawn, 0);
    assert_eq!(level.scores[&1].score_total, round_one * kill_points);
    let seen = |tag: &str| level.println.iter().filter(|line| *line == tag).count() as i32;
    assert_eq!(seen(LEVEL_KILL), round_one);
    assert_eq!(seen(PLAYER_KILL), round_one);
}

#[test]
fn a_zone_is_occupied_while_a_living_player_touches_one_of_its_volumes() {
    let mut level = level_with(2, StringTables::default());
    level.ents = std::sync::Arc::new(MapEnts::parse(
        "{ \"classname\" \"info_volume\" \"targetname\" \"start_zone\" \"model\" \"*3\" \"origin\" \"0 0 0\" }\n\
         { \"classname\" \"info_volume\" \"targetname\" \"start_zone\" \"model\" \"*4\" \"origin\" \"500 0 0\" }\n\
         { \"classname\" \"info_volume\" \"targetname\" \"upstairs_zone\" \"model\" \"*5\" \"origin\" \"0 0 200\" }",
    ));
    zone_init(&mut level, "start_zone");
    zone_init(&mut level, "upstairs_zone");
    enable_zone(&mut level, "start_zone");
    enable_zone(&mut level, "upstairs_zone");
    // Player 0 stands in model *4 (the start zone's second volume); player 1
    // stands in *5 upstairs, but is dead.
    level.players[1].alive = false;
    let occupied = occupied_volumes(&level, |player, volume| {
        matches!((player.entnum, volume.model), (0, 4) | (1, 5))
    });
    assert_eq!(occupied.into_iter().collect::<Vec<_>>(), [1]);
    level.occupied_volumes = [1].into();
    assert!(player_in_zone(&level, "start_zone"));
    assert!(!player_in_zone(&level, "upstairs_zone"));
    // A disabled zone is never occupied (`zone_is_enabled` comes first).
    level.zones.get_mut("start_zone").unwrap().is_enabled = false;
    assert!(!player_in_zone(&level, "start_zone"));
}
