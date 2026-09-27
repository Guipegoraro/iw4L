//! The choices the ported zombie scripts make (`zombiemode_t5`), pinned to the
//! original scripts' numbers and lists: which window `zombie_think` sends a
//! zombie to, how fast it moves (`set_run_speed`), its run cycle
//! (`set_zombie_run_cycle`), its swing (`pick_zombie_melee_anim`) and whom it
//! hunts (`get_closest_valid_player`). A choice that rolls is sampled many
//! times: every result must be allowed, and every allowed one must turn up.

use std::collections::BTreeSet;

use gsc_threads::Scheduler;
use sim::{ClientId, ScriptPlayer};
use zombiemode_t5::actor::{self, ACTOR_BAD_PATH_REPEAT, MotorEvent, actor_owner};
use zombiemode_t5::anims::{
    MoveSpeed, pick_zombie_melee_anim, set_run_speed, set_zombie_run_cycle,
};
use zombiemode_t5::spawner::{Entrance, ZombieAssureNode, get_closest_valid_player, pick_entrance};
use zombiemode_t5::utility::spawn_zombie;
use zombiemode_t5::{Level, MapEnts, StringTables};

const ROLLS: usize = 500;

fn level(ents: &str) -> Level {
    Level::new(
        "zombie_test",
        StringTables::default(),
        Default::default(),
        MapEnts::parse(ents),
    )
}

fn window(origin: [f32; 3]) -> String {
    format!(
        "{{ \"classname\" \"script_struct\" \"targetname\" \"exterior_goal\" \"origin\" \"{} {} {}\" }}\n",
        origin[0], origin[1], origin[2]
    )
}

/// Everything `roll` returned over [`ROLLS`] calls.
fn sample<T: Ord>(level: &mut Level, mut roll: impl FnMut(&mut Level) -> T) -> BTreeSet<T> {
    (0..ROLLS).map(|_| roll(level)).collect()
}

fn names(list: &[&'static str]) -> BTreeSet<&'static str> {
    list.iter().copied().collect()
}

#[test]
fn a_targeted_spawner_sends_its_zombie_to_the_window_closest_to_the_target() {
    // Spawner 0 targets a struct beside window B; window A is closer to the
    // zombie, and retail still sends it to B, with no roll.
    let ents = format!(
        "{{ \"classname\" \"actor_zombie\" \"target\" \"auto1\" \"origin\" \"0 0 0\" }}\n\
         {{ \"classname\" \"script_struct\" \"targetname\" \"auto1\" \"origin\" \"900 0 0\" }}\n{}{}{}",
        window([100.0, 0.0, 0.0]),
        window([1000.0, 0.0, 0.0]),
        window([1000.0, 600.0, 0.0]),
    );
    let mut level = level(&ents);
    let picks = sample(&mut level, |level| {
        format!("{:?}", pick_entrance(level, 0, [0.0; 3]))
    });
    assert_eq!(
        picks.into_iter().collect::<Vec<_>>(),
        [format!("{:?}", Some(Entrance::Forced([1000.0, 0.0, 0.0])))]
    );
}

#[test]
fn an_untargeted_zombie_picks_among_the_close_windows_cut_at_max_dist() {
    // The three closest are 100, 400 and 1200 away: the third is more than
    // 500 (`max_dist`) further than the second, so the pick is between the
    // first two.
    let ents = format!(
        "{{ \"classname\" \"actor_zombie\" \"origin\" \"0 0 0\" }}\n{}{}{}{}",
        window([100.0, 0.0, 0.0]),
        window([0.0, 400.0, 0.0]),
        window([-1200.0, 0.0, 0.0]),
        window([0.0, -3000.0, 0.0]),
    );
    let mut level = level(&ents);
    let near = vec![[100.0, 0.0, 0.0], [0.0, 400.0, 0.0]];
    let picks = sample(&mut level, |level| {
        match pick_entrance(level, 0, [0.0; 3]) {
            Some(Entrance::Picked {
                node,
                entrance_nodes,
            }) => {
                assert_eq!(entrance_nodes, near);
                format!("{node:?}")
            }
            other => panic!("{other:?}"),
        }
    });
    assert_eq!(
        picks,
        near.iter().map(|node| format!("{node:?}")).collect(),
        "both kept windows get picked"
    );
}

#[test]
fn a_single_window_is_the_pick_and_a_chaser_skips_the_window() {
    let mut one = level(&format!(
        "{{ \"classname\" \"actor_zombie\" \"origin\" \"0 0 0\" }}\n{}",
        window([300.0, 0.0, 0.0])
    ));
    assert_eq!(
        pick_entrance(&mut one, 0, [0.0; 3]),
        Some(Entrance::Picked {
            node: [300.0, 0.0, 0.0],
            entrance_nodes: vec![[300.0, 0.0, 0.0]],
        })
    );
    let mut chaser = level(&format!(
        "{{ \"classname\" \"actor_zombie\" \"script_string\" \"zombie_chaser\" \"origin\" \"0 0 0\" }}\n{}",
        window([300.0, 0.0, 0.0])
    ));
    assert_eq!(
        pick_entrance(&mut chaser, 0, [0.0; 3]),
        Some(Entrance::SkipTeardown)
    );
    let mut none = level("{ \"classname\" \"actor_zombie\" \"origin\" \"0 0 0\" }");
    assert_eq!(pick_entrance(&mut none, 0, [0.0; 3]), None);
}

#[test]
fn run_speed_walks_to_35_runs_to_70_and_sprints_above() {
    // `RandomIntRange( speed, speed + 35 )` rolls `speed ..= speed + 34`.
    let cases = [
        (1, vec![MoveSpeed::Walk]),
        (35, vec![MoveSpeed::Walk, MoveSpeed::Run]),
        (36, vec![MoveSpeed::Run]),
        (70, vec![MoveSpeed::Run, MoveSpeed::Sprint]),
        (71, vec![MoveSpeed::Sprint]),
    ];
    for (speed, expected) in cases {
        let mut level = level("");
        level.zombie_move_speed = speed;
        let seen = sample(&mut level, |level| format!("{:?}", set_run_speed(level)));
        let expected: BTreeSet<String> = expected.iter().map(|s| format!("{s:?}")).collect();
        assert_eq!(seen, expected, "zombie_move_speed {speed}");
    }
}

#[test]
fn a_run_cycle_comes_from_its_speed_list_only() {
    // `RandomIntRange( 1, 8 )` picks walk1..walk7, `( 1, 6 )` run1..run5 and
    // `( 1, 4 )` sprint1..sprint3, from the slots `init_anims` fills. Slots
    // repeat clips (sprint3 is sprint_v1 again), so each clip's share of many
    // rolls pins how many slots the pick spans, not just which names appear.
    let cases = [
        (
            MoveSpeed::Walk,
            vec![
                "ai_zombie_walk_v1",
                "ai_zombie_walk_v2",
                "ai_zombie_walk_v3",
                "ai_zombie_walk_v4",
                "ai_zombie_walk_v6",
                "ai_zombie_walk_v7",
                "ai_zombie_walk_v9",
            ],
        ),
        (
            MoveSpeed::Run,
            vec![
                "ai_zombie_walk_fast_v1",
                "ai_zombie_walk_fast_v2",
                "ai_zombie_walk_fast_v3",
                "ai_zombie_run_v2",
                "ai_zombie_run_v4",
            ],
        ),
        (
            MoveSpeed::Sprint,
            vec![
                "ai_zombie_sprint_v1",
                "ai_zombie_sprint_v2",
                "ai_zombie_sprint_v1",
            ],
        ),
    ];
    const SHARE_ROLLS: usize = 20_000;
    const SHARE_TOLERANCE: f64 = 0.02;
    for (speed, slots) in cases {
        let mut level = level("");
        let mut counts = std::collections::BTreeMap::<&str, usize>::new();
        for _ in 0..SHARE_ROLLS {
            *counts
                .entry(set_zombie_run_cycle(&mut level, speed))
                .or_default() += 1;
        }
        assert_eq!(
            counts.keys().copied().collect::<BTreeSet<_>>(),
            names(&slots),
            "{speed:?}"
        );
        for (clip, count) in counts {
            let expected =
                slots.iter().filter(|slot| **slot == clip).count() as f64 / slots.len() as f64;
            let share = count as f64 / SHARE_ROLLS as f64;
            assert!(
                (share - expected).abs() < SHARE_TOLERANCE,
                "{speed:?} {clip}: {share} of the rolls, expected {expected}"
            );
        }
    }
}

#[test]
fn a_swing_is_a_standing_swipe_or_one_for_the_move_speed() {
    let standing = [
        "ai_zombie_attack_v2",
        "ai_zombie_attack_v4",
        "ai_zombie_attack_v6",
        "ai_zombie_attack_v1",
        "ai_zombie_attack_forward_v1",
        "ai_zombie_attack_forward_v2",
    ];
    let walking = [
        "ai_zombie_walk_attack_v1",
        "ai_zombie_walk_attack_v2",
        "ai_zombie_walk_attack_v3",
        "ai_zombie_walk_attack_v4",
    ];
    let running = [
        "ai_zombie_run_attack_v1",
        "ai_zombie_run_attack_v2",
        "ai_zombie_run_attack_v3",
    ];
    let cases = [
        (MoveSpeed::Walk, &walking[..]),
        (MoveSpeed::Run, &running[..]),
        (MoveSpeed::Sprint, &running[..]),
    ];
    for (speed, moving) in cases {
        let mut level = level("");
        let seen = sample(&mut level, |level| pick_zombie_melee_anim(level, speed));
        let expected: BTreeSet<_> = standing.iter().chain(moving).copied().collect();
        assert_eq!(seen, expected, "{speed:?}");
    }
}

#[test]
fn the_closest_valid_player_skips_the_dead() {
    let player = |entnum: i32, alive: bool, x: f32| ScriptPlayer {
        client: ClientId(entnum as u32),
        entnum,
        alive,
        origin: [x, 0.0, 0.0],
        team: 2,
        lethal: None,
    };
    let mut level = level("");
    level.players = vec![
        player(0, true, 900.0),
        player(1, false, 50.0),
        player(2, true, 300.0),
    ];
    assert_eq!(
        get_closest_valid_player(&level, [0.0; 3]),
        Some(ClientId(2))
    );
    level
        .players
        .iter_mut()
        .for_each(|player| player.alive = false);
    assert_eq!(get_closest_valid_player(&level, [0.0; 3]), None);
}

#[test]
fn a_zombie_with_no_way_to_any_window_walks_its_entrances_then_the_closest_then_gives_up() {
    // No path nodes, so every goal is a `bad_path`. `zombie_assure_node`
    // sends it to each entrance node, waits 2 s, sends it to the (up to) 20
    // windows closest to it, nearest first, then waits 20 s and gives up.
    const FRAME_MS: u64 = 50;
    const RETRY_WAIT: f32 = 2.0;
    const GIVE_UP: f32 = 20.0;
    // `zombie_goto_entrance`'s goalradius.
    const ENTRANCE_RADIUS: f32 = 128.0;
    let windows: Vec<[f32; 3]> = (1..=5).map(|i| [i as f32 * 300.0, 0.0, 0.0]).collect();
    let ents = format!(
        "{{ \"classname\" \"actor_zombie_ger_zombie\" \"origin\" \"0 0 0\" }}\n{}",
        windows.iter().map(|&w| window(w)).collect::<String>()
    );
    let mut level = level(&ents);
    let actor = spawn_zombie(&mut level, 0).expect("the Nacht zombie aitype");
    let entrance_nodes = windows[..2].to_vec();
    level.set_goal_pos(actor, entrance_nodes[0], ENTRANCE_RADIUS);
    let mut threads = Scheduler::default();
    threads.spawn(
        actor_owner(actor),
        ZombieAssureNode::new(entrance_nodes.clone()),
    );

    let mut goals: Vec<[f32; 3]> = Vec::new();
    let mut ended_ms = None;
    let mut ms = 0;
    while ended_ms.is_none() && ms < 60_000 {
        threads.run(ms, &mut level);
        let motor = &mut level.zombies.get_mut(&actor).unwrap().motor;
        if let Some(goal) = motor.goal_pos()
            && goals.last() != Some(&goal)
        {
            goals.push(goal);
        }
        for event in motor.step(FRAME_MS as f32 / 1000.0, |_| None, |_| None) {
            if event == MotorEvent::BadPath {
                threads.notify(actor_owner(actor), actor::BAD_PATH, Vec::new());
            }
        }
        if threads.thread_count() == 0 {
            ended_ms = Some(ms);
        }
        ms += FRAME_MS;
    }

    // Each change of goal: the first entrance (already its goal), the second,
    // then every window from the nearest out.
    let mut expected = entrance_nodes;
    expected.extend(&windows);
    assert_eq!(goals, expected);
    // Every `SetGoalPos` waits for its `bad_path`, at most one repeat away.
    let tries = 2 + windows.len();
    let ended = ended_ms.expect("it gives up") as f32 / 1000.0;
    let earliest = RETRY_WAIT + GIVE_UP;
    let latest = earliest + (tries + 1) as f32 * ACTOR_BAD_PATH_REPEAT;
    assert!(
        (earliest..=latest).contains(&ended),
        "gave up at {ended} s, expected {earliest}..={latest}"
    );
}
