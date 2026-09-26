//! `savepos [slot]` / `loadpos [slot]`: remember where the local player
//! stands and looks, and teleport back through `move`, which already carries
//! the cheat and alive checks.

use std::collections::BTreeMap;

use bevy::prelude::*;
use net::{LocalPresentClient, PresentedSnapshot};

use crate::{ConsoleCommand, ConsoleSettings, ConsoleState};

/// Saved `(origin, viewangles)` by slot; kept for the whole session.
#[derive(Resource, Default)]
pub(crate) struct SavedPositions(BTreeMap<u8, ([f32; 3], [f32; 3])>);

pub(crate) fn route_savepos_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut saved: ResMut<SavedPositions>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    mut lines: ResMut<session::PendingConsoleLines>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
) {
    for cmd in events.read() {
        if !matches!(cmd.name.as_str(), "savepos" | "loadpos") {
            continue;
        }
        let slot = match cmd.args.first().map(|a| a.parse::<u8>()) {
            None => Ok(0),
            Some(Ok(slot)) => Ok(slot),
            Some(Err(_)) => Err(format!("usage: {} [slot 0-255]", cmd.name)),
        };
        let msg = match (cmd.name.as_str(), slot) {
            (_, Err(msg)) => msg,
            ("savepos", Ok(slot)) => match presented.alive_player(local.0) {
                Some(ps) => {
                    saved.0.insert(slot, (ps.origin, ps.viewangles));
                    let o = ps.origin;
                    format!("savepos {slot}: {:.0} {:.0} {:.0}", o[0], o[1], o[2])
                }
                None => "savepos: not Alive".to_owned(),
            },
            (_, Ok(slot)) => match saved.0.get(&slot) {
                Some((o, a)) => {
                    let line = format!("move {} {} {} {} {}", o[0], o[1], o[2], a[1], a[0]);
                    lines.0.push(line);
                    format!("loadpos {slot}")
                }
                None => format!("loadpos: slot {slot} is empty — savepos {slot} first"),
            },
        };
        diag::info!(Console, "{msg}");
        console.echo(msg, settings.log_capacity);
    }
}
