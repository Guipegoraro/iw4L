//! Match-wide movement tuning (retail `g_gravity` / `g_speed`), changed live
//! by the host. The values land in every player's `PlayerState`, which is
//! replicated, so prediction follows without a second copy.

use playerstate_iw4::PlayerState;

pub const GRAVITY_DEFAULT: i32 = 800;
pub const SPEED_DEFAULT: i32 = 190;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchTuning {
    pub gravity: i32,
    pub speed: i32,
}

impl Default for MatchTuning {
    fn default() -> Self {
        Self {
            gravity: GRAVITY_DEFAULT,
            speed: SPEED_DEFAULT,
        }
    }
}

/// Which knob a `ClientAction::SetTuning` turns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TuningKey {
    Gravity = 0,
    Speed = 1,
}

impl TuningKey {
    pub fn from_u8(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Gravity),
            1 => Some(Self::Speed),
            _ => None,
        }
    }
}

impl MatchTuning {
    pub fn set(&mut self, key: TuningKey, value: i32) {
        match key {
            TuningKey::Gravity => self.gravity = value.clamp(0, 10_000),
            TuningKey::Speed => self.speed = value.clamp(1, 2_000),
        }
    }

    pub fn apply(self, ps: &mut PlayerState) {
        ps.gravity = self.gravity;
        ps.speed = self.speed;
    }
}
