//! Skateboard movement: an IW4L movement mode with no retail counterpart.
//!
//! The board rolls along its own heading (`ps.skate_yaw`), independent of the
//! view. Forward pushes, back brakes, strafe carves, and jump winds up an ollie
//! that pops on release. A landing across the board, a hard impact or running
//! into a wall bails, which costs control for `bail_ms`.
//!
//! Everything reads the tick's `pml.frametime` only, never frame time, so the
//! authority and prediction step the same.

use playerstate_iw4::{ENTITYNUM_NONE, PlayerState, UserCmd, buttons};

use crate::{
    CollisionBackend, MoveBounds, Pml, complete_ground_trace, pm_drop_timers, pm_end_tick_velocity,
    pm_project_velocity, pm_step_slide_move,
};

/// The knobs of the board. Speeds in units/s, lengths in units, angles in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkateTuning {
    /// Acceleration while pushing.
    pub push_accel: f32,
    /// Pushing stops adding speed above this; slopes can still go faster.
    pub push_max_speed: f32,
    /// Hard cap on speed along the board.
    pub max_speed: f32,
    /// Deceleration at full brake.
    pub brake_decel: f32,
    /// Constant rolling resistance.
    pub rolling_decel: f32,
    /// Air drag: deceleration = `drag * speed²`.
    pub drag: f32,
    /// Carve radius at standstill.
    pub min_turn_radius: f32,
    /// Carve radius added per unit/s of speed: faster means wider.
    pub turn_radius_per_speed: f32,
    /// Turn rate at standstill (degrees/s), for aiming the board before a push.
    pub pivot_rate: f32,
    /// Below this speed the pivot rate blends in.
    pub pivot_speed: f32,
    /// How fast sideways slip dies out, per second.
    pub grip: f32,
    /// Ollie height from a tap.
    pub ollie_min_height: f32,
    /// Ollie height from a full wind-up.
    pub ollie_max_height: f32,
    /// Wind-up time to reach the full height.
    pub ollie_windup_ms: i32,
    /// Board spin in the air at full strafe (degrees/s).
    pub air_spin_rate: f32,
    /// A landing further than this off the direction of travel bails.
    pub land_max_angle: f32,
    /// A landing falling faster than this bails.
    pub bail_impact_speed: f32,
    /// Losing this much speed in one tick (a wall) bails.
    pub wall_bail_speed_loss: f32,
    /// Deceleration while bailed.
    pub bail_decel: f32,
    /// How long a bail lasts.
    pub bail_ms: i32,
}

impl SkateTuning {
    pub const DEFAULT: Self = Self {
        push_accel: 420.0,
        push_max_speed: 330.0,
        max_speed: 900.0,
        brake_decel: 500.0,
        rolling_decel: 15.0,
        drag: 0.000_4,
        min_turn_radius: 48.0,
        turn_radius_per_speed: 0.35,
        pivot_rate: 180.0,
        pivot_speed: 40.0,
        grip: 12.0,
        ollie_min_height: 24.0,
        ollie_max_height: 56.0,
        ollie_windup_ms: 400,
        air_spin_rate: 360.0,
        land_max_angle: 40.0,
        bail_impact_speed: 700.0,
        wall_bail_speed_loss: 250.0,
        bail_decel: 1200.0,
        bail_ms: 1500,
    };
}

impl Default for SkateTuning {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What one skate tick did, for the caller's events and for tests.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SkateOutcome {
    pub popped: bool,
    pub landed: bool,
    pub bailed: bool,
}

/// A whole skate tick as `pm_move` runs it: timers, ground trace, the move,
/// the ground trace again, and the landing judgement on touchdown.
pub fn pm_skate_tick<C: CollisionBackend>(
    ps: &mut PlayerState,
    pml: &mut Pml,
    cmd: &UserCmd,
    old_buttons: u32,
    tuning: &SkateTuning,
    bounds: MoveBounds,
    collision: &C,
) -> SkateOutcome {
    pm_drop_timers(ps, pml);
    let impact_speed = (ps.gravity as f32) * pml.frametime - ps.velocity[2];
    let mut outcome = SkateOutcome::default();
    ground_trace(
        ps,
        pml,
        impact_speed,
        tuning,
        bounds,
        collision,
        &mut outcome,
    );
    let moved = pm_skate_move(ps, pml, cmd, old_buttons, tuning, bounds, collision);
    outcome.popped |= moved.popped;
    outcome.bailed |= moved.bailed;
    ground_trace(
        ps,
        pml,
        impact_speed,
        tuning,
        bounds,
        collision,
        &mut outcome,
    );
    pm_end_tick_velocity(ps, pml);
    outcome
}

/// The retail ground trace, plus the skater's touchdown: the knees take the
/// impact, so retail's hard-landing slowdown is undone (its landing sound is
/// kept), and the landing is judged.
fn ground_trace<C: CollisionBackend>(
    ps: &mut PlayerState,
    pml: &mut Pml,
    impact_speed: f32,
    tuning: &SkateTuning,
    bounds: MoveBounds,
    collision: &C,
    outcome: &mut SkateOutcome,
) {
    let was_airborne = ps.ground_entity_num == ENTITYNUM_NONE;
    let rolling = [ps.velocity[0], ps.velocity[1]];
    complete_ground_trace(ps, pml, bounds, collision);
    if was_airborne && pml.walking != 0 {
        ps.velocity[0] = rolling[0];
        ps.velocity[1] = rolling[1];
        let landing = pm_skate_land(ps, impact_speed, tuning);
        outcome.landed = true;
        outcome.bailed |= landing.bailed;
    }
}

/// One skate move. `pml` must hold this tick's ground trace; the caller runs
/// the ground trace again afterwards, then [`pm_skate_land`].
pub fn pm_skate_move<C: CollisionBackend>(
    ps: &mut PlayerState,
    pml: &mut Pml,
    cmd: &UserCmd,
    old_buttons: u32,
    tuning: &SkateTuning,
    bounds: MoveBounds,
    collision: &C,
) -> SkateOutcome {
    let mut outcome = SkateOutcome::default();
    let dt = pml.frametime;
    let steer = f32::from(cmd.rightmove) / 127.0;
    let throttle = f32::from(cmd.forwardmove) / 127.0;

    // A flick-it trick arrives once per sequence change; a stale sequence
    // (say after a respawn) only resyncs, because its id is cleared.
    let [trick_seq, trick_id, trick_strength] = cmd.skate_trick;
    let flicked = i32::from(trick_seq) != ps.skate_trick_seq
        && crate::skate_trick(i32::from(trick_id)).is_some();
    ps.skate_trick_seq = i32::from(trick_seq);

    if ps.skate_bail_ms > 0 {
        ps.skate_bail_ms = (ps.skate_bail_ms - pml.msec).max(0);
        ps.skate_pop_ms = 0;
        if ps.skate_bail_ms == 0 {
            ps.skate_yaw = ps.viewangles[1];
        }
        if pml.walking != 0 {
            let horizontal = [ps.velocity[0], ps.velocity[1]];
            let speed = libm::sqrtf(horizontal[0] * horizontal[0] + horizontal[1] * horizontal[1]);
            let kept = if speed > 0.0 {
                (speed - tuning.bail_decel * dt).max(0.0) / speed
            } else {
                0.0
            };
            ps.velocity[0] *= kept;
            ps.velocity[1] *= kept;
            ground_step(ps, pml, bounds, collision);
        } else {
            air_step(ps, pml, bounds, collision);
        }
        return outcome;
    }

    if pml.walking == 0 {
        ps.skate_pop_ms = 0;
        if ps.skate_trick != 0 {
            ps.skate_trick_ms = ps.skate_trick_ms.saturating_add(pml.msec);
        }
        ps.skate_yaw = wrap_degrees(ps.skate_yaw - steer * tuning.air_spin_rate * dt);
        air_step(ps, pml, bounds, collision);
        return outcome;
    }

    let jump_held = cmd.buttons & buttons::JUMP != 0;
    let jump_was_held = old_buttons & buttons::JUMP != 0;
    if jump_held {
        ps.skate_pop_ms = (ps.skate_pop_ms + pml.msec).min(tuning.ollie_windup_ms);
    }

    // Speed along the board before the carve: the wheels carry it round.
    // Speed along the board on the ground plane, before the carve: the wheels
    // carry it round. What is left over in the plane is sideways slip.
    let normal = ground_normal(pml);
    let along = surface_heading(ps.skate_yaw, normal);
    let mut speed = dot(ps.velocity, along);
    let into_ground = dot(ps.velocity, normal);
    let lateral = [
        ps.velocity[0] - speed * along[0] - into_ground * normal[0],
        ps.velocity[1] - speed * along[1] - into_ground * normal[1],
        ps.velocity[2] - speed * along[2] - into_ground * normal[2],
    ];

    let abs_speed = speed.abs();
    let carve = abs_speed / (tuning.min_turn_radius + tuning.turn_radius_per_speed * abs_speed);
    let pivot = tuning.pivot_rate.to_radians() * (1.0 - abs_speed / tuning.pivot_speed).max(0.0);
    let turn_rate = carve.max(pivot).to_degrees();
    ps.skate_yaw = wrap_degrees(ps.skate_yaw - steer * turn_rate * dt);
    let along = surface_heading(ps.skate_yaw, normal);

    // Gravity along the board: -g·along_z, which is g·sin(slope) straight down it.
    let gravity = ps.gravity as f32;
    speed -= gravity * along[2] * dt;

    if throttle > 0.0 && speed < tuning.push_max_speed {
        speed = (speed + tuning.push_accel * throttle * dt).min(tuning.push_max_speed);
    }
    let mut resist = tuning.rolling_decel + tuning.drag * speed * speed;
    if throttle < 0.0 {
        resist += tuning.brake_decel * -throttle;
    }
    speed = toward_zero(speed, resist * dt);
    speed = speed.clamp(-tuning.max_speed, tuning.max_speed);

    let slip = (1.0 - tuning.grip * dt).max(0.0);
    for i in 0..3 {
        ps.velocity[i] = speed * along[i] + lateral[i] * slip;
    }

    let jump_released = !jump_held && jump_was_held && ps.skate_pop_ms > 0;
    if flicked || jump_released {
        // A flick's strength stands in for the wind-up.
        let wound = if flicked {
            f32::from(trick_strength) / 255.0
        } else {
            ps.skate_pop_ms as f32 / tuning.ollie_windup_ms as f32
        };
        ps.skate_trick = if flicked { i32::from(trick_id) } else { 0 };
        ps.skate_trick_ms = 0;
        let height =
            tuning.ollie_min_height + (tuning.ollie_max_height - tuning.ollie_min_height) * wound;
        ps.skate_pop_ms = 0;
        ps.velocity[2] = libm::sqrtf(2.0 * gravity * height);
        ps.ground_entity_num = ENTITYNUM_NONE;
        ps.jump_origin_z = ps.origin[2];
        ps.jump_time = cmd.server_time;
        pml.walking = 0;
        pml.ground_plane = 0;
        pml.almost_ground_plane = 0;
        outcome.popped = true;
        air_step(ps, pml, bounds, collision);
        return outcome;
    }

    ground_step(ps, pml, bounds, collision);

    let after = dot(ps.velocity, along);
    if speed.abs() - after.abs() > tuning.wall_bail_speed_loss {
        bail(ps, tuning);
        outcome.bailed = true;
    }
    outcome
}

/// Judge a touchdown: `impact_speed` is how fast the board was falling.
/// Rolling on means travel lines up with the board (either way round: fakie
/// is fine); otherwise, or on a hard impact, the rider bails.
pub fn pm_skate_land(
    ps: &mut PlayerState,
    impact_speed: f32,
    tuning: &SkateTuning,
) -> SkateOutcome {
    let mut outcome = SkateOutcome {
        landed: true,
        ..SkateOutcome::default()
    };
    if ps.skate_bail_ms > 0 {
        return outcome;
    }
    // A flip trick is caught only once the board has come round.
    if let Some(trick) = crate::skate_trick(ps.skate_trick) {
        let caught = ps.skate_trick_ms >= trick.rotation_ms();
        ps.skate_trick = 0;
        ps.skate_trick_ms = 0;
        if !caught {
            bail(ps, tuning);
            outcome.bailed = true;
            return outcome;
        }
        if trick.turns_board() {
            ps.skate_yaw = wrap_degrees(ps.skate_yaw + 180.0);
        }
    }
    let heading = heading(ps.skate_yaw);
    let horizontal = [ps.velocity[0], ps.velocity[1]];
    let speed = libm::sqrtf(horizontal[0] * horizontal[0] + horizontal[1] * horizontal[1]);
    let along = horizontal[0] * heading[0] + horizontal[1] * heading[1];
    let off_axis = if speed > 1.0 {
        libm::acosf((along.abs() / speed).min(1.0)).to_degrees()
    } else {
        0.0
    };
    if impact_speed > tuning.bail_impact_speed || off_axis > tuning.land_max_angle {
        bail(ps, tuning);
        outcome.bailed = true;
        return outcome;
    }
    // The wheels take the part of the travel that lines up with them.
    ps.velocity[0] = along * heading[0];
    ps.velocity[1] = along * heading[1];
    outcome
}

fn bail(ps: &mut PlayerState, tuning: &SkateTuning) {
    ps.skate_bail_ms = tuning.bail_ms;
    ps.skate_pop_ms = 0;
    ps.skate_trick = 0;
    ps.skate_trick_ms = 0;
    ps.velocity[0] *= 0.3;
    ps.velocity[1] *= 0.3;
}

fn ground_step<C: CollisionBackend>(
    ps: &mut PlayerState,
    pml: &Pml,
    bounds: MoveBounds,
    collision: &C,
) {
    let normal = ground_normal(pml);
    pm_project_velocity(&mut ps.velocity, &normal);
    if ps.velocity[0] != 0.0 || ps.velocity[1] != 0.0 {
        pm_step_slide_move(
            ps,
            pml,
            collision,
            bounds.mins,
            bounds.maxs,
            bounds.tracemask,
            None,
        );
    }
}

fn air_step<C: CollisionBackend>(
    ps: &mut PlayerState,
    pml: &Pml,
    bounds: MoveBounds,
    collision: &C,
) {
    let gravity = ps.gravity as f32;
    pm_step_slide_move(
        ps,
        pml,
        collision,
        bounds.mins,
        bounds.maxs,
        bounds.tracemask,
        Some(gravity),
    );
}

fn ground_normal(pml: &Pml) -> [f32; 3] {
    if pml.ground_plane == 0 {
        return [0.0, 0.0, 1.0];
    }
    [
        f32::from_bits(pml.ground_trace[1]),
        f32::from_bits(pml.ground_trace[2]),
        f32::from_bits(pml.ground_trace[3]),
    ]
}

/// The board's heading laid onto the ground plane, unit length.
fn surface_heading(yaw_degrees: f32, normal: [f32; 3]) -> [f32; 3] {
    let flat = heading(yaw_degrees);
    let flat = [flat[0], flat[1], 0.0];
    let into = dot(flat, normal);
    let mut along = [
        flat[0] - into * normal[0],
        flat[1] - into * normal[1],
        flat[2] - into * normal[2],
    ];
    let length = libm::sqrtf(dot(along, along));
    if length > 0.0 {
        for value in &mut along {
            *value /= length;
        }
    }
    along
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn heading(yaw_degrees: f32) -> [f32; 2] {
    let yaw = yaw_degrees.to_radians();
    [libm::cosf(yaw), libm::sinf(yaw)]
}

fn toward_zero(value: f32, amount: f32) -> f32 {
    if value > 0.0 {
        (value - amount).max(0.0)
    } else {
        (value + amount).min(0.0)
    }
}

fn wrap_degrees(angle: f32) -> f32 {
    let wrapped = angle % 360.0;
    if wrapped < 0.0 {
        wrapped + 360.0
    } else {
        wrapped
    }
}
