//! `maps\_zombiemode_utility.gsc`

use crate::Level;

/// A `level.zombie_vars` value. GSC keeps ints and floats apart, and the table
/// parse depends on which one the script asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZombieVar {
    Int(i32),
    Float(f32),
}

impl ZombieVar {
    pub fn as_i32(self) -> i32 {
        match self {
            Self::Int(value) => value,
            Self::Float(value) => value as i32,
        }
    }

    pub fn as_f32(self) -> f32 {
        match self {
            Self::Int(value) => value as f32,
            Self::Float(value) => value,
        }
    }
}

pub const ZOMBIEMODE_TABLE: &str = "mp/zombiemode.csv";

/// `maps\_zombiemode_utility.gsc::set_zombie_var`: the table's value in
/// `column` when it has one, else `value`.
pub fn set_zombie_var(
    level: &mut Level,
    var: &str,
    value: ZombieVar,
    is_float: bool,
    column: usize,
) -> ZombieVar {
    let cell = level.tables.lookup(ZOMBIEMODE_TABLE, 0, var, column);
    let value = if cell.is_empty() {
        value
    } else if is_float {
        ZombieVar::Float(gsc_float(cell))
    } else {
        ZombieVar::Int(gsc_int(cell))
    };
    level.zombie_vars.insert(var.to_owned(), value);
    value
}

/// GSC `int( string )`: the leading integer, `atoi`-style (`"2.0"` is 2).
pub fn gsc_int(text: &str) -> i32 {
    let text = text.trim_start();
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    digits[..end].parse::<i64>().map_or(0, |n| {
        (sign * n).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    })
}

/// GSC `float( string )`: the leading decimal number, `atof`-style.
pub fn gsc_float(text: &str) -> f32 {
    let text = text.trim_start();
    let end = text
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && i == 0)))
        .map_or(text.len(), |(i, _)| i);
    text[..end].parse().unwrap_or(0.0)
}

/// `maps\_zombiemode_utility.gsc::round_up_score`.
pub fn round_up_score(score: i32, value: i32) -> i32 {
    let mut new_score = score - score % value;
    if new_score < score {
        new_score += value;
    }
    new_score
}

/// `maps\_zombiemode_utility.gsc::round_up_to_ten`.
pub fn round_up_to_ten(score: i32) -> i32 {
    round_up_score(score, 10)
}

/// `maps\_utility.gsc::wait_network_frame`: a tenth of a second alone;
/// with remote clients, until they acknowledge a snapshot, which a listen
/// server here sends every frame, so the same tenth is kept.
pub fn wait_network_frame() -> gsc_threads::Yield {
    gsc_threads::Yield::wait_seconds(0.1)
}

/// `self.meleeDamage` from `maps\_zombiemode_spawner.gsc::zombie_spawn_init`.
pub const ZOMBIE_MELEE_DAMAGE: i32 = 60;

/// Why [`spawn_zombie`] made no zombie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnFailed {
    /// No entity at that index.
    NoSpawner,
    /// Its classname is no aitype the port knows, so it never will spawn.
    UnknownAiType,
}

/// `maps\_zombiemode_utility.gsc::spawn_zombie`: spawns from `spawner` (an
/// entity index) through its aitype and character, with the run cycle
/// `zombie_spawn_init` picks. Returns the actor number; the engine spawn is
/// queued for after the frame, and the caller starts its `zombie_think`. An
/// unknown spawner type is reported once per classname.
pub fn spawn_zombie(level: &mut crate::Level, spawner: u32) -> Result<u32, SpawnFailed> {
    let ent = level
        .ents
        .get(spawner)
        .ok_or(SpawnFailed::NoSpawner)?
        .clone();
    let Some(aitype) = crate::character::aitype_for_classname(ent.classname()) else {
        if level
            .unknown_spawner_types
            .insert(ent.classname().to_owned())
        {
            level.println(format!(
                "spawn_zombie: no aitype for spawner classname {}; its zombies count as spawned",
                ent.classname()
            ));
        }
        return Err(SpawnFailed::UnknownAiType);
    };
    let (body, head) = crate::character::pick_models(level, &aitype.character);
    let move_speed = crate::anims::set_run_speed(level);
    let move_clip = crate::anims::set_zombie_run_cycle(level, move_speed);
    let actor = level.next_actor;
    level.next_actor += 1;
    let origin = ent.origin();
    let yaw = ent.angles()[1];
    level.zombies.insert(
        actor,
        crate::level::Zombie {
            spawner,
            // `zombie_spawn_init` sets `self.health = level.zombie_health`
            // over the aitype's.
            health: level.zombie_health,
            body,
            head,
            motor: crate::actor::Motor::new(origin, yaw, move_clip),
            move_speed,
            favorite_enemy: None,
            melee_damage: ZOMBIE_MELEE_DAMAGE,
            melee_attack_dist: None,
            meleeing: false,
            // `zombie_spawn_init`: `self.ignoreall = true`.
            ignore_all: true,
            zombie_bad_path: None,
        },
    );
    level
        .commands
        .push(crate::level::EngineCommand::SpawnActor {
            actor,
            body,
            head,
            origin,
            yaw,
            anim: move_clip,
        });
    Ok(actor)
}
