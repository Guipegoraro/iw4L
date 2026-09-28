//! `maps\_zombiemode_score.gsc`

use crate::Level;
use crate::damage::{MOD_BURNED, MOD_MELEE, MOD_UNKNOWN};
use crate::utility::round_up_score;

/// What earned the points, as `player_add_points( event, … )` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointsEvent<'a> {
    /// A kill: the damage's means of death and where it hit.
    Death {
        means_of_death: &'a str,
        hit_location: &'a str,
    },
    BallisticKnifeDeath,
    DamageLight,
    Damage,
    DamageAds,
    /// `rebuild_board`, `carpenter_powerup`, `bonus_points_powerup`,
    /// `thundergun_fling`, `hacker_transfer`, `reviver`: the points come with
    /// the event.
    Flat(i32),
}

/// `maps\_zombiemode_score.gsc::get_zombie_death_player_points`.
pub fn get_zombie_death_player_points(level: &Level) -> i32 {
    let var = match level.players.len() {
        1 => "zombie_score_kill_1player",
        2 => "zombie_score_kill_2player",
        3 => "zombie_score_kill_3player",
        _ => "zombie_score_kill_4player",
    };
    level.zombie_var(var).as_i32()
}

/// `maps\_zombiemode_score.gsc::player_add_points_kill_bonus`.
pub fn player_add_points_kill_bonus(
    level: &Level,
    means_of_death: &str,
    hit_location: &str,
) -> i32 {
    if means_of_death == MOD_MELEE {
        return level.zombie_var("zombie_score_bonus_melee").as_i32();
    }
    if means_of_death == MOD_BURNED {
        return level.zombie_var("zombie_score_bonus_burn").as_i32();
    }
    let var = match hit_location {
        "head" | "helmet" => "zombie_score_bonus_head",
        "neck" => "zombie_score_bonus_neck",
        "torso_upper" | "torso_lower" => "zombie_score_bonus_torso",
        _ => return 0,
    };
    level.zombie_var(var).as_i32()
}

/// `maps\_zombiemode_score.gsc::get_points_multiplier` (no mutators).
pub fn get_points_multiplier(level: &Level) -> i32 {
    level.zombie_var("zombie_point_scalar").as_i32()
}

/// The points `player_add_points` gives the player for `event`, before
/// `add_to_player_score`. Team points are not returned: retail's
/// `add_to_team_score` is commented out (MM 3/10/10).
pub fn player_points(level: &Level, event: PointsEvent<'_>) -> i32 {
    let points = match event {
        PointsEvent::Death {
            means_of_death,
            hit_location,
        } => {
            let bonus = player_add_points_kill_bonus(level, means_of_death, hit_location);
            let bonus = if level.zombie_var("zombie_powerup_insta_kill_on").as_i32() == 1
                && means_of_death == MOD_UNKNOWN
            {
                bonus * 2
            } else {
                bonus
            };
            get_zombie_death_player_points(level) + bonus
        }
        PointsEvent::BallisticKnifeDeath => {
            get_zombie_death_player_points(level)
                + level.zombie_var("zombie_score_bonus_melee").as_i32()
        }
        PointsEvent::DamageLight => level.zombie_var("zombie_score_damage_light").as_i32(),
        PointsEvent::Damage => level.zombie_var("zombie_score_damage_normal").as_i32(),
        PointsEvent::DamageAds => {
            (level.zombie_var("zombie_score_damage_normal").as_f32() * 1.25) as i32
        }
        PointsEvent::Flat(points) => points,
    };
    get_points_multiplier(level) * round_up_score(points, 5)
}

/// `self player_add_points( event, … )` for the player at `entnum`.
pub fn player_add_points(level: &mut Level, entnum: i32, event: PointsEvent<'_>) {
    if level.intermission || !level.players.iter().any(|p| p.entnum == entnum && p.alive) {
        return;
    }
    let points = player_points(level, event);
    add_to_player_score(level, entnum, points, true);
}

/// `maps\_zombiemode_score.gsc::add_to_player_score`.
pub fn add_to_player_score(level: &mut Level, entnum: i32, points: i32, add_to_total: bool) {
    if level.intermission {
        return;
    }
    let score = level.scores.entry(entnum).or_default();
    score.score += points;
    if add_to_total {
        score.score_total += points;
    }
}

/// `maps\_zombiemode_score.gsc::minus_to_player_score`.
pub fn minus_to_player_score(level: &mut Level, entnum: i32, points: i32) {
    if level.intermission {
        return;
    }
    level.scores.entry(entnum).or_default().score -= points;
}
