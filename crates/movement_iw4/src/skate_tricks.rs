//! Flip tricks as the sim sees them: a pop plus board rotation. A trick is
//! kickflip turns (+ kickflip, - heelflip) and a shuv in degrees (+
//! backside, - frontside). Its id travels in `UserCmd::skate_trick`; names
//! match Skate 3's gesture files (an `N_` prefix is the nollie version).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkateTrick {
    pub name: &'static str,
    pub flips: i8,
    pub shuv: i16,
}

/// Index + 1 is the trick id; 0 means no trick.
pub const SKATE_TRICKS: &[SkateTrick] = &[
    trick("Ollie", 0, 0),
    trick("Nollie", 0, 0),
    trick("Kickflip", 1, 0),
    trick("Heelflip", -1, 0),
    trick("PopShuvit", 0, 180),
    trick("FSPopShuvit", 0, -180),
    trick("360PopShuvit", 0, 360),
    trick("FS360PopShuvit", 0, -360),
    trick("VarialKickflip", 1, 180),
    trick("VarialHeelflip", -1, -180),
    trick("Hardflip", 1, -180),
    trick("InwardHeelflip", -1, 180),
    trick("360Flip", 1, 360),
    trick("Laserflip", -1, -360),
    trick("360Hardflip", 1, -360),
    trick("360InwardHeelflip", -1, 360),
];

const fn trick(name: &'static str, flips: i8, shuv: i16) -> SkateTrick {
    SkateTrick { name, flips, shuv }
}

/// The id for a gesture name; nollie variants (`N_Kickflip`) share the
/// regular trick's rotation.
pub fn skate_trick_id(name: &str) -> Option<u8> {
    let base = name.strip_prefix("N_").unwrap_or(name);
    SKATE_TRICKS
        .iter()
        .position(|t| t.name == base)
        .and_then(|i| u8::try_from(i + 1).ok())
}

pub fn skate_trick(id: i32) -> Option<SkateTrick> {
    usize::try_from(id - 1)
        .ok()
        .and_then(|i| SKATE_TRICKS.get(i).copied())
}

impl SkateTrick {
    /// How long the board takes to come round: land before this and you bail.
    pub fn rotation_ms(self) -> i32 {
        let flips = i32::from(self.flips.unsigned_abs());
        let half_shuvs = i32::from(self.shuv.unsigned_abs()) / 180;
        if flips == 0 && half_shuvs == 0 {
            return 0;
        }
        200 + 150 * flips + 110 * half_shuvs
    }

    /// An odd number of half shuvs lands the board the other way round.
    pub fn turns_board(self) -> bool {
        (self.shuv.unsigned_abs() / 180) % 2 == 1
    }
}
