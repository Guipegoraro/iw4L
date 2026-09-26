//! Skate physics, stepped exactly as `pm_move` steps it (`pm_skate_tick`), on
//! analytic ground: infinite planes for floors, slopes and walls. Deterministic
//! and game-free, so a scenario is a few lines on a `Rider`.

use movement_iw4::{
    CollisionBackend, GroundTraceInput, MoveBounds, MoveMode, Pml, SkateOutcome, SkateTuning,
    pm_set_move_mode, pm_skate_tick,
};
use playerstate_iw4::{ENTITYNUM_NONE, PlayerState, UserCmd, buttons};
use trace_iw4::Trace;

const TICK_MS: i32 = 50;
const TICKS_PER_SECOND: u32 = 20;
/// The gap a trace keeps from the surface it stops at.
const CLIP_EPSILON: f32 = 0.125;
const BOUNDS: MoveBounds = MoveBounds {
    mins: [-15.0, -15.0, 0.0],
    maxs: [15.0, 15.0, 70.0],
    tracemask: 1,
};

/// Solid half-spaces `normal · p < dist`; the box may not enter any of them.
struct Planes(Vec<([f32; 3], f32)>);

impl Planes {
    fn flat() -> Self {
        Self(vec![([0.0, 0.0, 1.0], 0.0)])
    }

    /// Ground through the origin that falls away along +x by `degrees`.
    fn downhill(degrees: f32) -> Self {
        let a = degrees.to_radians();
        Self(vec![([a.sin(), 0.0, a.cos()], 0.0)])
    }

    /// Ground through the origin that rises along +x by `degrees`.
    fn uphill(degrees: f32) -> Self {
        let a = degrees.to_radians();
        Self(vec![([-a.sin(), 0.0, a.cos()], 0.0)])
    }

    /// A wall facing -x whose face stands at `x`.
    fn with_wall(mut self, x: f32) -> Self {
        self.0.push(([-1.0, 0.0, 0.0], -x));
        self
    }

    /// Where a box resting on the first plane sits, above `(x, y)`.
    fn rest_origin(&self, x: f32, y: f32) -> [f32; 3] {
        let (n, dist) = self.0[0];
        let offset = support(n, BOUNDS.mins, BOUNDS.maxs);
        let z = (dist + CLIP_EPSILON - n[0] * x - n[1] * y - offset) / n[2];
        [x, y, z]
    }
}

/// `n · corner` for the box corner deepest along `-n`.
fn support(n: [f32; 3], mins: [f32; 3], maxs: [f32; 3]) -> f32 {
    (0..3)
        .map(|i| n[i] * if n[i] < 0.0 { maxs[i] } else { mins[i] })
        .sum()
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl CollisionBackend for Planes {
    fn trace(&self, input: GroundTraceInput) -> Trace {
        let mut out = Trace {
            fraction: 1.0,
            ..Trace::default()
        };
        for &(n, dist) in &self.0 {
            let offset = support(n, input.mins, input.maxs);
            let d0 = dot(n, input.start) + offset - dist;
            let d1 = dot(n, input.end) + offset - dist;
            if d0 < 0.0 {
                out.startsolid = 1;
                if d1 < 0.0 {
                    out.allsolid = 1;
                    out.fraction = 0.0;
                }
                continue;
            }
            if d1 >= CLIP_EPSILON || d1 >= d0 {
                continue;
            }
            let fraction = ((d0 - CLIP_EPSILON) / (d0 - d1)).clamp(0.0, 1.0);
            if fraction < out.fraction {
                out.fraction = fraction;
                out.normal = n;
                out.contents = 1;
                out.walkable = u8::from(n[2] >= 0.7);
                out.hit_type = trace_iw4::HITTYPE_ENTITY;
                out.hit_id = trace_iw4::ENTITYNUM_WORLD;
            }
        }
        for i in 0..3 {
            out.endpos[i] = input.start[i] + (input.end[i] - input.start[i]) * out.fraction;
        }
        out
    }
}

#[derive(Clone, Copy)]
struct Input {
    forward: i8,
    right: i8,
    jump: bool,
}

const IDLE: Input = Input {
    forward: 0,
    right: 0,
    jump: false,
};
const PUSH: Input = Input {
    forward: 127,
    ..IDLE
};
const BRAKE: Input = Input {
    forward: -127,
    ..IDLE
};
const CARVE_RIGHT: Input = Input { right: 127, ..IDLE };
const WIND_UP: Input = Input { jump: true, ..IDLE };

struct Rider {
    ps: PlayerState,
    world: Planes,
    tuning: SkateTuning,
    time: i32,
    buttons: u32,
}

impl Rider {
    /// On the board at `x = 0`, facing +x, at rest.
    fn on(world: Planes) -> Self {
        let mut ps = PlayerState::ZERO;
        ps.gravity = 800;
        ps.speed = 190;
        ps.origin = world.rest_origin(0.0, 0.0);
        ps.ground_entity_num = i32::from(trace_iw4::ENTITYNUM_WORLD);
        pm_set_move_mode(&mut ps, MoveMode::Skate);
        Self {
            ps,
            world,
            tuning: SkateTuning::DEFAULT,
            time: 0,
            buttons: 0,
        }
    }

    fn rolling(mut self, speed: f32) -> Self {
        self.ps.velocity = [speed, 0.0, 0.0];
        self
    }

    fn tick(&mut self, input: Input) -> SkateOutcome {
        self.time += TICK_MS;
        let cmd = UserCmd {
            server_time: self.time,
            buttons: if input.jump { buttons::JUMP } else { 0 },
            forwardmove: input.forward,
            rightmove: input.right,
            ..UserCmd::default()
        };
        let mut pml = Pml {
            forward: [0.0; 3],
            right: [0.0; 3],
            up: [0.0; 3],
            frametime: TICK_MS as f32 * 0.001,
            msec: TICK_MS,
            walking: 0,
            ground_plane: 0,
            almost_ground_plane: 0,
            ground_trace: [0; 11],
            previous_origin: self.ps.origin,
            previous_velocity: self.ps.velocity,
            holdrand: 0,
        };
        let old = self.buttons;
        self.buttons = cmd.buttons;
        pm_skate_tick(
            &mut self.ps,
            &mut pml,
            &cmd,
            old,
            &self.tuning,
            BOUNDS,
            &self.world,
        )
    }

    /// Hold `input` for `ticks`, returning every tick's outcome.
    fn hold(&mut self, input: Input, ticks: u32) -> Vec<SkateOutcome> {
        (0..ticks).map(|_| self.tick(input)).collect()
    }

    fn seconds(&mut self, input: Input, seconds: f32) -> Vec<SkateOutcome> {
        self.hold(input, (seconds * TICKS_PER_SECOND as f32).round() as u32)
    }

    /// Ground speed.
    fn speed(&self) -> f32 {
        self.ps.velocity[0].hypot(self.ps.velocity[1])
    }

    /// Speed along the board, on a slope too: negative when rolling backwards.
    fn board_speed(&self) -> f32 {
        let yaw = self.ps.skate_yaw.to_radians();
        let flat = self.ps.velocity[0] * yaw.cos() + self.ps.velocity[1] * yaw.sin();
        let v = self.ps.velocity;
        flat.signum() * (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
    }

    fn on_ground(&self) -> bool {
        self.ps.ground_entity_num != ENTITYNUM_NONE
    }

    /// Ollie from the ground after `windup` ticks of holding jump; returns the
    /// peak height above take-off and every outcome up to the landing.
    fn ollie(&mut self, windup: u32, in_air: Input) -> (f32, Vec<SkateOutcome>) {
        let mut outcomes = self.hold(WIND_UP, windup);
        let start = self.ps.origin[2];
        let mut peak = start;
        outcomes.push(self.tick(in_air));
        for _ in 0..(4 * TICKS_PER_SECOND) {
            peak = peak.max(self.ps.origin[2]);
            if self.on_ground() {
                break;
            }
            outcomes.push(self.tick(in_air));
        }
        (peak - start, outcomes)
    }
}

fn any(outcomes: &[SkateOutcome], pick: impl Fn(&SkateOutcome) -> bool) -> bool {
    outcomes.iter().any(pick)
}

#[test]
fn skate_flat_coast() {
    let mut rider = Rider::on(Planes::flat());
    let push_max = rider.tuning.push_max_speed;

    let mut top = 0.0_f32;
    for _ in 0..(3 * TICKS_PER_SECOND) {
        rider.tick(PUSH);
        top = top.max(rider.speed());
    }
    assert!(
        top <= push_max + 0.01,
        "pushing reached {top}, cap {push_max}"
    );
    // Rolling resistance still acts at the cap, so pushing tops out just under it.
    assert!(
        rider.speed() > 0.98 * push_max,
        "3 s of pushing: {}",
        rider.speed()
    );
    assert!(rider.on_ground());

    let start = rider.speed();
    let mut last = start;
    for _ in 0..(2 * TICKS_PER_SECOND) {
        rider.tick(IDLE);
        assert!(
            rider.speed() <= last,
            "coasting sped up: {last} -> {}",
            rider.speed()
        );
        last = rider.speed();
    }
    assert!(last > 0.7 * start, "2 s coast kept {last} of {start}");
    assert!(
        rider.ps.origin[1].abs() < 0.01,
        "drifted sideways: {:?}",
        rider.ps.origin
    );

    rider.seconds(BRAKE, 2.0);
    assert!(rider.speed() < 1.0, "2 s of braking left {}", rider.speed());
}

#[test]
fn skate_slope_gain() {
    let mut down = Rider::on(Planes::downhill(8.0));
    let mut last = 0.0;
    for _ in 0..(2 * TICKS_PER_SECOND) {
        down.tick(IDLE);
        assert!(down.board_speed() > last, "downhill did not gain speed");
        last = down.board_speed();
    }
    assert!(down.on_ground(), "left the slope");
    // Down the slope gravity gives g·sin(8°); rolling resistance and drag take
    // some back, drag at most what it takes at the final speed.
    let t = down.tuning;
    let gain = 800.0 * 8.0_f32.to_radians().sin() - t.rolling_decel;
    let (most, least) = (gain * 2.0, (gain - t.drag * last * last) * 2.0);
    assert!(
        last <= most && last >= least,
        "2 s downhill at 8 degrees: {last}, expected {least}..{most}"
    );

    let mut up = Rider::on(Planes::uphill(8.0)).rolling(300.0);
    up.seconds(IDLE, 1.0);
    let slowed = up.board_speed();
    assert!(
        slowed < 250.0 && slowed > 0.0,
        "1 s uphill from 300: {slowed}"
    );
    up.seconds(IDLE, 4.0);
    assert!(
        up.board_speed() < -20.0,
        "never rolled back: {}",
        up.board_speed()
    );
}

#[test]
fn skate_carve_radius() {
    let radius_at = |speed: f32| {
        let mut rider = Rider::on(Planes::flat()).rolling(speed);
        let yaw0 = rider.ps.skate_yaw;
        let mut path = 0.0;
        let mut v = 0.0;
        for _ in 0..6 {
            let from = rider.ps.origin;
            rider.tick(CARVE_RIGHT);
            path += (rider.ps.origin[0] - from[0]).hypot(rider.ps.origin[1] - from[1]);
            v += rider.board_speed() / 6.0;
        }
        let turned = (yaw0 - rider.ps.skate_yaw).rem_euclid(360.0);
        assert!(rider.ps.origin[1] < 0.0, "carving right went left");
        (path / turned.to_radians(), v, rider.tuning)
    };
    let (slow, slow_v, tuning) = radius_at(120.0);
    let (fast, fast_v, _) = radius_at(320.0);
    assert!(
        fast > slow * 1.5,
        "radius {slow} at {slow_v}, {fast} at {fast_v}"
    );
    for (radius, v) in [(slow, slow_v), (fast, fast_v)] {
        let expected = tuning.min_turn_radius + tuning.turn_radius_per_speed * v;
        assert!(
            (radius - expected).abs() < 0.15 * expected,
            "radius {radius} at {v}, expected {expected}"
        );
    }
}

#[test]
fn skate_ollie_height() {
    let tuning = SkateTuning::DEFAULT;
    let mut rider = Rider::on(Planes::flat()).rolling(200.0);
    let (tap, outcomes) = rider.ollie(1, IDLE);
    assert!(any(&outcomes, |o| o.popped), "a tap did not pop");
    assert!(any(&outcomes, |o| o.landed), "never landed");
    assert!(!any(&outcomes, |o| o.bailed), "a straight tap bailed");
    let wound = tuning.ollie_min_height
        + (tuning.ollie_max_height - tuning.ollie_min_height) * TICK_MS as f32
            / tuning.ollie_windup_ms as f32;
    assert!(
        (tap - wound).abs() < 3.0,
        "tap rose {tap}, expected {wound}"
    );

    let before = rider.board_speed();
    let full_windup = (tuning.ollie_windup_ms / TICK_MS) as u32;
    let (full, outcomes) = rider.ollie(full_windup, IDLE);
    assert!(!any(&outcomes, |o| o.bailed), "a straight ollie bailed");
    let max = tuning.ollie_max_height;
    assert!(
        (full - max).abs() < 3.0,
        "full wind-up rose {full}, expected {max}"
    );
    assert!(rider.on_ground());
    assert!(
        rider.board_speed() > 0.85 * before,
        "landing lost speed: {before} -> {}",
        rider.board_speed()
    );
}

#[test]
fn skate_bail() {
    // Spun a quarter turn in the air: lands across the board.
    let mut rider = Rider::on(Planes::flat()).rolling(250.0);
    rider.hold(WIND_UP, 8);
    rider.tick(IDLE);
    rider.hold(CARVE_RIGHT, 5);
    let mut outcomes = Vec::new();
    while !rider.on_ground() {
        outcomes.push(rider.tick(IDLE));
    }
    assert!(
        any(&outcomes, |o| o.bailed),
        "landed sideways without a bail"
    );
    assert!(rider.ps.skate_bail_ms > 0);

    // Bailed: pushing does nothing until the bail runs out.
    let bail_ticks = (rider.tuning.bail_ms / TICK_MS) as u32;
    rider.hold(PUSH, bail_ticks - 2);
    assert!(rider.speed() < 1.0, "moved while bailed: {}", rider.speed());
    rider.hold(PUSH, 2 + TICKS_PER_SECOND);
    assert_eq!(rider.ps.skate_bail_ms, 0);
    assert!(rider.speed() > 100.0, "control never came back");

    // Straight into a wall.
    let mut rider = Rider::on(Planes::flat().with_wall(200.0)).rolling(300.0);
    let outcomes = rider.seconds(IDLE, 1.0);
    assert!(
        any(&outcomes, |o| o.bailed),
        "hit a wall at 300 without a bail"
    );
}

#[test]
fn skate_deterministic() {
    let run = || {
        let mut rider = Rider::on(Planes::downhill(5.0).with_wall(3000.0));
        rider.seconds(PUSH, 1.5);
        rider.seconds(CARVE_RIGHT, 0.5);
        rider.ollie(6, CARVE_RIGHT);
        rider.seconds(PUSH, 3.0);
        rider.ps
    };
    assert_eq!(run(), run());
}
