//! `maps\_zombiemode_powerups.gsc`

use crate::Level;
use crate::utility::{ZombieVar, set_zombie_var};

/// The `set_zombie_var` block of `maps\_zombiemode_powerups.gsc::init`; the
/// powerups themselves are not ported yet. No column is given, so the table's
/// column 1 applies.
pub fn init_vars(level: &mut Level) {
    use ZombieVar::Int;
    for (var, value) in [
        ("zombie_insta_kill", 0),
        ("zombie_point_scalar", 1),
        ("zombie_drop_item", 0),
        ("zombie_timer_offset", 350),
        ("zombie_timer_offset_interval", 30),
        ("zombie_powerup_fire_sale_on", 0),
        ("zombie_powerup_fire_sale_time", 30),
        ("zombie_powerup_bonfire_sale_on", 0),
        ("zombie_powerup_bonfire_sale_time", 30),
        ("zombie_powerup_insta_kill_on", 0),
        ("zombie_powerup_insta_kill_time", 30),
        ("zombie_powerup_point_doubler_on", 0),
        ("zombie_powerup_point_doubler_time", 30),
        ("zombie_powerup_drop_increment", 2000),
        ("zombie_powerup_drop_max_per_round", 4),
    ] {
        set_zombie_var(level, var, Int(value), false, 1);
    }
}
