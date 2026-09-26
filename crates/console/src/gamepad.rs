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
