//! The engine half of a Black Ops actor, which the zombie scripts drive:
//! `SetGoalPos` and `goalradius`, the `goal` and `bad_path` notifies,
//! `OrientMode`, `AnimMode`, and movement by the playing clip's root motion.
//!
//! Retail runs this in the engine's actor code; here it runs in the mode
//! script, after the frame's threads, so it is cloned with the simulation
//! for replay and prediction. [`Motor::step`] is pure: the clips and the
//! ground come in as arguments.

use std::sync::Arc;

use gsc_threads::Owner;
use pathnodes::{PathGraph, Route};

use crate::anims::IDLE;
use xmodel_runtime::AnimClip;

/// `self waittill( "goal" )`: within `goalradius` of the goal.
pub const GOAL: &str = "goal";
/// `self waittill( "bad_path" )`: no route to the goal.
pub const BAD_PATH: &str = "bad_path";
/// `self waittill( "death" )`.
pub const DEATH: &str = "death";

/// How fast an actor turns toward where it faces, degrees per second.
/// Not measured against retail yet (ZMB-029).
pub const ACTOR_TURN_RATE: f32 = 360.0;

/// How often an actor with no route to its goal says `bad_path` again. Retail
/// keeps repathing while the path fails, and each failure is a `bad_path`
/// (`zombie_assure_node` counts on hearing one after each `SetGoalPos`); the
/// interval is not measured against retail (ZMB-029).
pub const ACTOR_BAD_PATH_REPEAT: f32 = 0.5;

/// The playback rate every actor clip runs at: `SetAnimKnob…( clip, 1, 0.2,
/// 1 )`, the last argument, in every zombie script that starts one.
pub const ANIM_RATE: f32 = 1.0;

/// A zombie actor's script owner: actors sit above every entity number.
pub fn actor_owner(actor: u32) -> Owner {
    Owner(ACTOR_OWNER_BASE + u64::from(actor))
}

const ACTOR_OWNER_BASE: u64 = 1 << 32;

/// The actor number of a zombie's script owner: [`actor_owner`] undone.
pub fn actor_number(owner: Owner) -> u32 {
    (owner.0 - ACTOR_OWNER_BASE) as u32
}

/// `OrientMode`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Orient {
    /// `"face motion"` / `"face default"`: toward where it walks.
    #[default]
    Motion,
    /// `"face angle", yaw`.
    Angle(f32),
}

/// `AnimMode`: whether the clip's root motion moves the actor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimMode {
    /// `"none"`: the clip's root motion carries it along the route to the
    /// goal.
    #[default]
    Walk,
    /// `"zonly_physics"`: in place (melee).
    InPlace,
}

/// What a [`Motor::step`] tells the scripts.
#[derive(Clone, Debug, PartialEq)]
pub enum MotorEvent {
    Goal,
    BadPath,
    /// A note of a clip played with [`Motor::play_scripted`] (`"fire"`).
    Note(String),
    /// That clip reached its end.
    End,
}

#[derive(Clone, Debug, PartialEq)]
struct Goal {
    pos: [f32; 3],
    route: Option<Route>,
    radius: f32,
    reached: bool,
    /// With no route: seconds until the next `bad_path`.
    bad_path_in: f32,
}

/// One playing clip: its name and normalized time.
#[derive(Clone, Debug, PartialEq)]
struct Playing {
    clip: &'static str,
    time: f32,
    looping: bool,
    /// The engine has not started it on the actor's model yet.
    restart: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Motor {
    pub origin: [f32; 3],
    pub yaw: f32,
    /// The run cycle it walks with; standing, it plays [`IDLE`].
    pub move_clip: &'static str,
    pub orient: Orient,
    pub anim_mode: AnimMode,
    goal: Option<Goal>,
    playing: Playing,
    /// A clip the script plays over the walk (a melee swing).
    scripted: bool,
    pending: Vec<MotorEvent>,
}

impl Motor {
    /// An actor standing at `origin`, already playing `move_clip` (the spawn
    /// starts it).
    pub fn new(origin: [f32; 3], yaw: f32, move_clip: &'static str) -> Self {
        Self {
            origin,
            yaw,
            move_clip,
            orient: Orient::Motion,
            anim_mode: AnimMode::Walk,
            goal: None,
            playing: Playing {
                clip: move_clip,
                time: 0.0,
                looping: true,
                restart: false,
            },
            scripted: false,
            pending: Vec::new(),
        }
    }

    pub fn clip(&self) -> &'static str {
        self.playing.clip
    }

    /// The clip the engine must start on the model, once: name and looping.
    pub fn take_restart(&mut self) -> Option<(&'static str, bool)> {
        std::mem::take(&mut self.playing.restart)
            .then_some((self.playing.clip, self.playing.looping))
    }

    /// `self.goalradius = radius; self SetGoalPos( goal )`. No route is a
    /// `bad_path` on the next step.
    pub fn set_goal_pos(&mut self, graph: &PathGraph, goal: [f32; 3], radius: f32) {
        let route = graph.route(self.origin, goal);
        if route.is_none() {
            self.pending.push(MotorEvent::BadPath);
        }
        self.goal = Some(Goal {
            pos: goal,
            route,
            radius,
            reached: false,
            bad_path_in: ACTOR_BAD_PATH_REPEAT,
        });
    }

    /// `self.goalpos`: where the last `SetGoalPos` sent it.
    pub fn goal_pos(&self) -> Option<[f32; 3]> {
        self.goal.as_ref().map(|goal| goal.pos)
    }

    pub fn at_goal(&self) -> bool {
        self.goal.as_ref().is_some_and(|goal| goal.reached)
    }

    /// `SetFlaggedAnimKnobAllRestart( flag, clip, ... )` once: plays `clip`
    /// from its start; its notes and its end come back as events.
    pub fn play_scripted(&mut self, clip: &'static str) {
        self.scripted = true;
        self.play(clip, false);
    }

    /// Back to the run cycle after a scripted clip.
    pub fn stop_scripted(&mut self) {
        self.scripted = false;
    }

    fn play(&mut self, clip: &'static str, looping: bool) {
        self.playing = Playing {
            clip,
            time: 0.0,
            looping,
            restart: true,
        };
    }

    fn walking(&self) -> bool {
        self.anim_mode == AnimMode::Walk
            && self
                .goal
                .as_ref()
                .is_some_and(|goal| goal.route.is_some() && !goal.reached)
    }

    /// One frame of `dt` seconds. `clips` looks a clip's data up by name, and
    /// is asked only once this frame's clip is chosen (a clip it does not know
    /// leaves the actor where it is); `ground` gives the floor height under a
    /// point.
    pub fn step(
        &mut self,
        dt: f32,
        clips: impl Fn(&str) -> Option<Arc<AnimClip>>,
        ground: impl Fn([f32; 3]) -> Option<f32>,
    ) -> Vec<MotorEvent> {
        let mut events = std::mem::take(&mut self.pending);
        if let Some(goal) = self.goal.as_mut()
            && goal.route.is_none()
        {
            goal.bad_path_in -= dt;
            if goal.bad_path_in <= 0.0 {
                goal.bad_path_in += ACTOR_BAD_PATH_REPEAT;
                events.push(MotorEvent::BadPath);
            }
        }
        if !self.scripted {
            let want = if self.walking() { self.move_clip } else { IDLE };
            if self.playing.clip != want {
                self.play(want, true);
            }
        }
        let clip = clips(self.playing.clip);
        let Some(clip) = clip.as_deref().filter(|clip| clip.duration() > 0.0) else {
            return events;
        };
        let old = self.playing.time;
        let advance = dt * clip.frequency();
        let distance = root_motion_distance(clip, old, advance, self.playing.looping);
        let mut new = old + advance;
        let mut ended = false;
        if new >= 1.0 {
            if self.playing.looping {
                new = new.fract();
            } else {
                new = 1.0;
                ended = old < 1.0;
            }
        }
        self.playing.time = new;
        if self.scripted {
            let duration = clip.duration();
            events.extend(
                clip.crossed_notify_records(old * duration, new * duration)
                    .into_iter()
                    .map(|note| MotorEvent::Note(note.name)),
            );
            if ended {
                events.push(MotorEvent::End);
            }
        }

        let mut heading = None;
        let walking = self.walking();
        if let Some(goal) = self.goal.as_mut()
            && !goal.reached
            && let Some(route) = goal.route.as_mut()
        {
            if walking {
                let step = route.advance(self.origin, distance, goal.radius);
                self.origin = step.origin;
                heading = step.heading;
                goal.reached = step.arrived;
            } else {
                goal.reached = route.at_goal(self.origin, goal.radius);
            }
            if goal.reached {
                events.push(MotorEvent::Goal);
            }
        }
        if let Some(z) = ground(self.origin) {
            self.origin[2] = z;
        }

        let target = match self.orient {
            Orient::Angle(yaw) => Some(yaw),
            Orient::Motion => heading.map(|dir| math_iw4::vec_to_yaw(dir[0], dir[1])),
        };
        if let Some(target) = target {
            self.yaw = turn_toward(self.yaw, target, ACTOR_TURN_RATE * dt);
        }
        events
    }
}

/// How far the clip's root moves in the ground plane from normalized time
/// `old` over `advance` of the clip: a loop wraps through its end (whole
/// cycles included), a clip that does not loop stops at its end.
pub fn root_motion_distance(clip: &AnimClip, old: f32, advance: f32, looping: bool) -> f32 {
    let span = |from: f32, to: f32| {
        math_iw4::vec3_distance_2d(clip.abs_delta_trans(from), clip.abs_delta_trans(to))
    };
    let end = old + advance.max(0.0);
    if !looping || end < 1.0 {
        return span(old, end.min(1.0));
    }
    let cycles = end.floor();
    span(old, 1.0) + (cycles - 1.0) * span(0.0, 1.0) + span(0.0, end - cycles)
}

/// `yaw` turned toward `target` by at most `max_step` degrees.
pub fn turn_toward(yaw: f32, target: f32, max_step: f32) -> f32 {
    let delta = math_iw4::angle_subtract(target, yaw);
    if delta.abs() <= max_step {
        target
    } else {
        math_iw4::angle_normalize_360(yaw + max_step.copysign(delta))
    }
}
