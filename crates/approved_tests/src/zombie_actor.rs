//! A zombie actor's engine half (`zombiemode_t5::actor::Motor`): it walks its
//! route by the clip's root motion, stops on the goal radius and says so once,
//! plays a scripted swing in place with its notes, and turns at a fixed rate.

use std::sync::Arc;

use pathnodes::PathGraph;
use xmodel_runtime::{AnimClip, ClipNotify, FrameIndices, Keyed, Translation};
use zombiemode_t5::actor::{ACTOR_BAD_PATH_REPEAT, AnimMode, Motor, MotorEvent, turn_toward};
use zombiemode_t5::anims::IDLE;
use zombiemode_t5::zombie_melee::{NOTE_END, NOTE_FIRE};

use crate::path_fixtures::{link, node};

const WALK: &str = "walk";
const SWING: &str = "swing";
/// The goal radius the walking tests use (`find_flesh`'s).
const GOAL_RADIUS: f32 = 32.0;
/// How close a float position must come to the expected one.
const CLOSE: f32 = 0.01;

/// One second long, ten frames, its root moving 100 units along x.
fn clip(name: &str, looping: bool, notes: &[(&str, f32)]) -> AnimClip {
    moving(name, looping, notes, 100.0)
}

/// One second long, ten frames, its root moving `reach` units along x.
fn moving(name: &str, looping: bool, notes: &[(&str, f32)], reach: f32) -> AnimClip {
    AnimClip {
        name: name.to_owned(),
        framerate: 10.0,
        numframes: 10,
        looping,
        tracks: Vec::new(),
        notifies: notes
            .iter()
            .map(|&(name, time)| ClipNotify {
                name: name.to_owned(),
                time,
            })
            .collect(),
        delta_translation: Translation::FullKeyed(Keyed {
            values: (0..=10)
                .map(|i| [i as f32 * reach / 10.0, 0.0, 0.0])
                .collect(),
            frames: FrameIndices::Dense,
        }),
    }
}

/// Two nodes 1000 units apart along x, linked both ways.
fn line() -> PathGraph {
    let mut graph = PathGraph {
        nodes: vec![node(0.0, 0.0), node(1000.0, 0.0)],
    };
    link(&mut graph, 0, 1);
    graph
}

fn flat(_: [f32; 3]) -> Option<f32> {
    None
}

/// The motor's clip lookup over `clips`, by name.
fn lookup(clips: &[AnimClip]) -> impl Fn(&str) -> Option<Arc<AnimClip>> + use<> {
    let clips: Vec<Arc<AnimClip>> = clips.iter().cloned().map(Arc::new).collect();
    move |name| clips.iter().find(|clip| clip.name == name).cloned()
}

/// A lookup that knows no clip.
fn none(_: &str) -> Option<Arc<AnimClip>> {
    None
}

#[test]
fn a_zombie_walks_its_route_by_the_clip_root_motion() {
    let walk = lookup(&[clip(WALK, true, &[])]);
    let mut motor = Motor::new([0.0, 0.0, 0.0], 0.0, WALK);
    motor.set_goal_pos(&line(), [1000.0, 0.0, 0.0], GOAL_RADIUS);
    let events = motor.step(0.5, &walk, flat);
    assert!(events.is_empty());
    assert!((motor.origin[0] - 50.0).abs() < CLOSE, "{:?}", motor.origin);
    // Across the loop point the distance keeps adding up.
    motor.step(0.75, &walk, flat);
    assert!(
        (motor.origin[0] - 125.0).abs() < CLOSE,
        "{:?}",
        motor.origin
    );
}

#[test]
fn reaching_the_goal_radius_is_one_goal_then_the_idle_clip() {
    let clips = lookup(&[clip(WALK, true, &[]), clip(IDLE, true, &[])]);
    let mut motor = Motor::new([900.0, 0.0, 0.0], 0.0, WALK);
    motor.set_goal_pos(&line(), [1000.0, 0.0, 0.0], GOAL_RADIUS);
    let events = motor.step(1.0, &clips, flat);
    assert_eq!(events, vec![MotorEvent::Goal]);
    assert!(
        (motor.origin[0] - 968.0).abs() < CLOSE,
        "{:?}",
        motor.origin
    );
    assert!(motor.at_goal());
    assert!(motor.step(0.1, &clips, flat).is_empty());
    assert_eq!(motor.clip(), IDLE);
    assert_eq!(motor.take_restart(), Some((IDLE, true)));
    motor.step(1.0, &clips, flat);
    assert!(
        (motor.origin[0] - 968.0).abs() < CLOSE,
        "an idle zombie stays put"
    );
}

#[test]
fn the_frame_that_switches_clips_moves_by_the_new_clip() {
    // The idle clip's root stands still: stepping the switch frame with its
    // data would not move the zombie at all.
    let clips = lookup(&[clip(WALK, true, &[]), moving(IDLE, true, &[], 0.0)]);
    let mut motor = Motor::new([900.0, 0.0, 0.0], 0.0, WALK);
    motor.set_goal_pos(&line(), [920.0, 0.0, 0.0], GOAL_RADIUS);
    assert_eq!(motor.step(0.1, &clips, flat), vec![MotorEvent::Goal]);
    motor.step(0.1, &clips, flat);
    assert_eq!(motor.clip(), IDLE);
    let before = motor.origin[0];
    motor.set_goal_pos(&line(), [1000.0, 0.0, 0.0], GOAL_RADIUS);
    motor.step(0.1, &clips, flat);
    assert_eq!(motor.clip(), WALK);
    assert!(
        (motor.origin[0] - before - 10.0).abs() < CLOSE,
        "{before} -> {:?}",
        motor.origin
    );
}

#[test]
fn a_goal_with_no_route_is_a_bad_path() {
    let mut motor = Motor::new([0.0, 0.0, 0.0], 0.0, WALK);
    motor.set_goal_pos(&line(), [90_000.0, 0.0, 0.0], GOAL_RADIUS);
    assert_eq!(motor.step(0.05, none, flat), vec![MotorEvent::BadPath]);
    // While the path keeps failing it keeps saying so, as retail's repath
    // does; `zombie_assure_node` listens again after each new goal.
    let mut repeats = 0;
    // 41 frames is 2.05 s: the last repeat is not on the frame boundary.
    const FRAMES: usize = 41;
    for _ in 0..FRAMES {
        repeats += motor
            .step(0.05, none, flat)
            .iter()
            .filter(|event| **event == MotorEvent::BadPath)
            .count();
    }
    let expected = (FRAMES as f32 * 0.05 / ACTOR_BAD_PATH_REPEAT).floor() as usize;
    assert_eq!(repeats, expected);
}

#[test]
fn a_scripted_swing_plays_in_place_with_its_notes_and_end() {
    let swing = lookup(&[clip(SWING, false, &[(NOTE_FIRE, 0.4), (NOTE_END, 1.0)])]);
    let mut motor = Motor::new([0.0, 0.0, 0.0], 0.0, WALK);
    motor.set_goal_pos(&line(), [1000.0, 0.0, 0.0], GOAL_RADIUS);
    motor.anim_mode = AnimMode::InPlace;
    motor.play_scripted(SWING);
    assert_eq!(motor.take_restart(), Some((SWING, false)));
    assert!(motor.step(0.3, &swing, flat).is_empty());
    assert_eq!(
        motor.step(0.3, &swing, flat),
        vec![MotorEvent::Note(NOTE_FIRE.into())]
    );
    assert_eq!(motor.step(0.5, &swing, flat), vec![MotorEvent::End]);
    assert_eq!(motor.origin, [0.0, 0.0, 0.0]);
    assert!(
        motor.step(0.5, &swing, flat).is_empty(),
        "the end fires once"
    );
}

#[test]
fn the_ground_sets_the_height() {
    let clips = lookup(&[clip(WALK, true, &[]), clip(IDLE, true, &[])]);
    let mut motor = Motor::new([0.0, 0.0, 40.0], 0.0, WALK);
    motor.step(0.1, &clips, |_| Some(12.5));
    assert_eq!(motor.origin[2], 12.5);
}

#[test]
fn turning_is_limited_and_takes_the_short_way_round() {
    assert_eq!(turn_toward(350.0, 10.0, 90.0), 10.0);
    assert!((turn_toward(0.0, 90.0, 18.0) - 18.0).abs() < CLOSE);
    assert!((turn_toward(0.0, 270.0, 18.0) - 342.0).abs() < CLOSE);
}
