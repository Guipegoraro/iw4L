//! `timescale [f]`: scales the virtual clock that drives the fixed sim tick,
//! so the whole game slows or speeds up while each tick stays the same.

use bevy::prelude::*;

use crate::{ConsoleCommand, ConsoleSettings, ConsoleState};

pub(crate) const TIMESCALE_MIN: f32 = 0.05;
pub(crate) const TIMESCALE_MAX: f32 = 4.0;

pub(crate) fn route_timescale_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut time: ResMut<Time<Virtual>>,
    authority: Option<Res<net::AuthorityWorld>>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
) {
    for cmd in events.read() {
        if cmd.name != "timescale" {
            continue;
        }
        let msg = match cmd.args.first().map(|a| a.parse::<f32>()) {
            None => format!("timescale {}", time.relative_speed()),
            Some(Err(_)) => "usage: timescale [0.05..4]".to_owned(),
            // A client of a remote host cannot slow the host's clock.
            Some(Ok(_)) if authority.is_none() => {
                "timescale: only the host of the match can change time".to_owned()
            }
            Some(Ok(scale)) => {
                let scale = scale.clamp(TIMESCALE_MIN, TIMESCALE_MAX);
                time.set_relative_speed(scale);
                format!("timescale {scale}")
            }
        };
        diag::info!(Console, "{msg}");
        console.echo(msg, settings.log_capacity);
    }
}
