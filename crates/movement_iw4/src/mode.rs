//! Movement modes: IW4L's hook for movement that retail never had.
//!
//! `pm_move` reads `ps.move_mode` once per tick. `Normal` is the retail path
//! untouched; every other mode replaces walk/air/ladder/mantle with its own
//! step, keeping the view, stance and timers that surround them. A new mode is
//! a variant here, a step function, and one arm in `pm_move`.

use playerstate_iw4::PlayerState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum MoveMode {
    #[default]
    Normal = 0,
    Skate = 1,
}

impl MoveMode {
    pub const ALL: [Self; 2] = [Self::Normal, Self::Skate];

    pub fn of(ps: &PlayerState) -> Self {
        Self::ALL
            .into_iter()
            .find(|mode| *mode as u32 == ps.move_mode)
            .unwrap_or(Self::Normal)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Skate => "skate",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.name() == name)
    }
}

/// Switch mode and reset the state the new mode owns.
pub fn pm_set_move_mode(ps: &mut PlayerState, mode: MoveMode) {
    ps.move_mode = mode as u32;
    ps.skate_pop_ms = 0;
    ps.skate_bail_ms = 0;
    ps.skate_yaw = ps.viewangles[1];
}
