//! Gamepads. Buttons are ordinary binds (`BindButton::Pad`, defaults in
//! `default_controls.cfg`); this module adds what a bind cannot express: the
//! sticks as analog move and look, and the pad driving the menus by standing
//! in for the keys they already read.

use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use ui::MenuEnabled;

/// The knobs of the sticks. Rates in degrees per second at full tilt.
pub(crate) struct PadTuning {
    pub move_deadzone: f32,
    pub look_deadzone: f32,
    pub yaw_rate: f32,
    pub pitch_rate: f32,
    /// Look response curve: tilt is raised to this power, for fine aim near
    /// the centre and full speed at the edge.
    pub look_exponent: f32,
}

pub(crate) const PAD_TUNING: PadTuning = PadTuning {
    move_deadzone: 0.2,
    look_deadzone: 0.15,
    yaw_rate: 240.0,
    pitch_rate: 170.0,
    look_exponent: 2.0,
};

/// Keys a pad button stands in for, and when.
const MENU_KEYS: &[(GamepadButton, KeyCode)] = &[
    (GamepadButton::DPadUp, KeyCode::ArrowUp),
    (GamepadButton::DPadDown, KeyCode::ArrowDown),
    (GamepadButton::DPadLeft, KeyCode::ArrowLeft),
    (GamepadButton::DPadRight, KeyCode::ArrowRight),
    (GamepadButton::South, KeyCode::Enter),
    (GamepadButton::East, KeyCode::Escape),
];
const ALWAYS_KEYS: &[(GamepadButton, KeyCode)] = &[
    // The mod menu, paused.
    (GamepadButton::Start, KeyCode::F5),
    // The pause menu.
    (GamepadButton::Select, KeyCode::Escape),
];

/// Radial deadzone, rescaled so the stick still reaches 1 at the edge.
fn deadzone(stick: Vec2, zone: f32) -> Vec2 {
    let length = stick.length();
    if length <= zone {
        return Vec2::ZERO;
    }
    stick / length * ((length - zone) / (1.0 - zone)).min(1.0)
}

/// Left stick as `[forward, right]`, summed over every pad.
pub(crate) fn pad_move<'a>(pads: impl IntoIterator<Item = &'a Gamepad>) -> [f32; 2] {
    let stick: Vec2 = pads
        .into_iter()
        .map(|pad| deadzone(pad.left_stick(), PAD_TUNING.move_deadzone))
        .sum();
    [stick.y.clamp(-1.0, 1.0), stick.x.clamp(-1.0, 1.0)]
}

/// Right stick as `[pitch, yaw]` degrees to turn over `dt`; up looks up,
/// right turns right. `scale` carries the zoom so aiming slows with it.
pub(crate) fn pad_look<'a>(
    pads: impl IntoIterator<Item = &'a Gamepad>,
    dt: f32,
    scale: f32,
    invert: bool,
) -> [f32; 2] {
    let stick: Vec2 = pads
        .into_iter()
        .map(|pad| deadzone(pad.right_stick(), PAD_TUNING.look_deadzone))
        .sum();
    let curve = |v: f32| v.signum() * v.abs().min(1.0).powf(PAD_TUNING.look_exponent);
    let pitch_sign = if invert { 1.0 } else { -1.0 };
    [
        pitch_sign * curve(stick.y) * PAD_TUNING.pitch_rate * dt * scale,
        -curve(stick.x) * PAD_TUNING.yaw_rate * dt * scale,
    ]
}

/// Press and release the keys a pad button stands in for. A key pressed for
/// the menu is released on the button's release even if the menu has closed
/// in between, so nothing stays held.
pub(crate) fn pad_menu_keys(
    pads: Query<&Gamepad>,
    menu: Res<MenuEnabled>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut held: Local<Vec<(GamepadButton, KeyCode)>>,
) {
    held.retain(|&(button, key)| {
        let still = pads.iter().any(|pad| pad.pressed(button));
        if !still {
            keys.release(key);
        }
        still
    });
    let tables: &[&[(GamepadButton, KeyCode)]] = if menu.0 {
        &[MENU_KEYS, ALWAYS_KEYS]
    } else {
        &[ALWAYS_KEYS]
    };
    for &(button, key) in tables.iter().copied().flatten() {
        if pads.iter().any(|pad| pad.just_pressed(button)) {
            keys.press(key);
            held.push((button, key));
        }
    }
}

pub(crate) fn log_pad_connections(pads: Query<(Entity, &Name), Added<Gamepad>>) {
    for (entity, name) in &pads {
        diag::info!(Console, "gamepad connected: {name} ({entity})");
    }
}

/// Skate 3 samples the stick at 60 Hz; the recogniser counts those ticks.
const FLICK_HZ: f32 = 60.0;
/// How long a fired trick stays in the command before only its sequence
/// remains, so a stale trick never replays after a respawn.
const TRICK_HOLD_SECS: f32 = 0.3;
const FLICK_DEADZONE: f32 = 0.1;
/// How fast the view swings round to the board while skating with a pad,
/// as a fraction of the gap per second.
const FOLLOW_BOARD_RATE: f32 = 4.0;

/// Flick-it state: the recogniser (from the player's own `skater.pat`,
/// found under `IW4L_SKATE3_DATA`), the trick sequence and a trick queued by
/// the `trick` console command.
#[derive(Resource, Default)]
pub(crate) struct FlickIt {
    recognizer: Option<skate_input::Recognizer>,
    loaded: bool,
    carry: f32,
    seq: u8,
    fired_at: f32,
    queued: Option<(u8, u8)>,
}

fn load_recognizer() -> Option<skate_input::Recognizer> {
    let Some(root) = std::env::var_os("IW4L_SKATE3_DATA") else {
        diag::warn!(
            Console,
            "flick-it: IW4L_SKATE3_DATA is not set (the folder holding data/joystick/skater.pat); right-stick tricks are off"
        );
        return None;
    };
    let path = std::path::Path::new(&root).join("data/joystick/skater.pat");
    match skate_input::load_pat(&path).and_then(skate_input::Recognizer::new) {
        Ok(recognizer) => {
            diag::info!(
                Console,
                "flick-it: {} gestures from {}",
                recognizer.patterns().len(),
                path.display()
            );
            Some(recognizer)
        }
        Err(error) => {
            diag::warn!(Console, "flick-it: {error}");
            None
        }
    }
}

fn fire(state: &mut FlickIt, out: &mut net::ClientActionInput, id: u8, strength: u8, now: f32) {
    state.seq = state.seq.wrapping_add(1).max(1);
    state.fired_at = now;
    out.skate_trick = [state.seq, id, strength];
}

/// While skating (and not aiming with LT), the right stick is sampled into
/// the recogniser instead of looking; a trick goes out as
/// `[sequence, id, strength]` in the command.
pub(crate) fn sample_flick_it(
    time: Res<Time>,
    pads: Query<&Gamepad>,
    (presented, local): (Res<net::PresentedSnapshot>, Res<net::LocalPresentClient>),
    menu: Res<MenuEnabled>,
    mut out: ResMut<net::ClientActionInput>,
    mut look: ResMut<net::LookState>,
    mut state: ResMut<FlickIt>,
) {
    let now = time.elapsed_secs();
    if now - state.fired_at > TRICK_HOLD_SECS {
        out.skate_trick = [state.seq, 0, 0];
    }
    if let Some((id, strength)) = state.queued.take() {
        fire(&mut state, &mut out, id, strength, now);
    }
    let skater = presented
        .alive_player(local.0)
        .filter(|ps| movement_iw4::MoveMode::of(ps) == movement_iw4::MoveMode::Skate);
    let skating = skater.is_some();
    let aiming = pads
        .iter()
        .any(|pad| pad.pressed(GamepadButton::LeftTrigger2));
    if !skating || aiming || menu.0 {
        state.carry = 0.0;
        return;
    }
    out.pad_look = [0.0; 2];
    // The right stick is busy with tricks, so the view follows the board.
    if let Some(ps) = skater.filter(|_| !pads.is_empty()) {
        let gap = (ps.skate_yaw - ps.viewangles[1] + 540.0).rem_euclid(360.0) - 180.0;
        let turn = gap * (FOLLOW_BOARD_RATE * time.delta_secs()).min(1.0);
        look.angles[1] = look.angles[1].wrapping_add((turn * input_iw4::ANGLE2SHORT) as i32);
    }
    if !state.loaded {
        state.loaded = true;
        state.recognizer = load_recognizer();
    }
    let stick = pads
        .iter()
        .map(Gamepad::right_stick)
        .find(|s| *s != Vec2::ZERO)
        .unwrap_or(Vec2::ZERO);
    let axis = |v: f32| if v.abs() < FLICK_DEADZONE { 0.0 } else { v };
    // Skate 3's gesture space has y pointing down.
    let sample = [axis(stick.x), -axis(stick.y)];
    state.carry += time.delta_secs() * FLICK_HZ;
    while state.carry >= 1.0 {
        state.carry -= 1.0;
        let Some(recognizer) = state.recognizer.as_mut() else {
            return;
        };
        let Some(hit) = recognizer.sample(sample, skate_input::Settings::STOCK) else {
            continue;
        };
        let name = recognizer.patterns()[hit.pattern].name.clone();
        let Some(id) = movement_iw4::skate_trick_id(&name) else {
            continue;
        };
        diag::info!(Console, "flick-it: {name} strength {:.2}", hit.strength);
        fire(&mut state, &mut out, id, (hit.strength * 255.0) as u8, now);
    }
}

/// `trick <name> [strength 0..1]`: fire a flip trick as if flicked, for
/// keyboards and scripts.
pub(crate) fn route_trick_commands(
    mut events: MessageReader<crate::ConsoleCommand>,
    mut state: ResMut<FlickIt>,
    mut console: ResMut<crate::ConsoleState>,
    settings: Res<crate::ConsoleSettings>,
) {
    for cmd in events.read() {
        if cmd.name != "trick" {
            continue;
        }
        let names = || {
            movement_iw4::SKATE_TRICKS
                .iter()
                .map(|t| t.name)
                .collect::<Vec<_>>()
                .join(" ")
        };
        let found = cmd.args.first().and_then(|wanted| {
            movement_iw4::SKATE_TRICKS
                .iter()
                .position(|t| t.name.eq_ignore_ascii_case(wanted))
        });
        let msg = match found {
            Some(index) => {
                let strength = cmd
                    .args
                    .get(1)
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(1.0)
                    .clamp(0.0, 1.0);
                let id = u8::try_from(index + 1).unwrap_or(0);
                state.queued = Some((id, (strength * 255.0) as u8));
                format!(
                    "trick {} (strength {strength:.2})",
                    movement_iw4::SKATE_TRICKS[index].name
                )
            }
            None => format!("usage: trick <name> [strength]; names: {}", names()),
        };
        diag::info!(Console, "{msg}");
        console.echo(msg, settings.log_capacity);
    }
}
