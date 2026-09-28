//! The engine half of hurting a zombie: `DoDamage` lowers its health, and at
//! none the engine runs its `deathFunction`, removes it and notifies `death`.
//!
//! Damage is queued on the level (a script's `DoDamage`, later a player's
//! bullets) and applied after the frame's threads run, by
//! [`apply_zombie_damage`].

use gsc_threads::{Owner, Scheduler};
use sim::ClientId;

use crate::Level;
use crate::actor::{DEATH, actor_owner};
use crate::level::EngineCommand;
use crate::spawner::ZombieDeathEvent;

/// `"MOD_UNKNOWN"`: `DoDamage` with no means of death.
pub const MOD_UNKNOWN: &str = "MOD_UNKNOWN";
/// `"MOD_MELEE"`: a knife or a melee hit.
pub const MOD_MELEE: &str = "MOD_MELEE";
/// `"MOD_BURNED"`: fire.
pub const MOD_BURNED: &str = "MOD_BURNED";
/// `"none"`: damage that hit no part of the body.
pub const HIT_NONE: &str = "none";

/// One `DoDamage` on a zombie.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZombieDamage {
    pub actor: u32,
    pub amount: i32,
    /// The player who did it; `None` for the level's own damage.
    pub attacker: Option<ClientId>,
    pub means_of_death: &'static str,
    pub hit_location: &'static str,
}

/// What `zombie_death_event` reads off the dead zombie: which one it was,
/// `self.attacker` and `self.ignoreall`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Corpse {
    pub actor: u32,
    pub attacker: Option<ClientId>,
    pub ignore_all: bool,
}

/// Applies the damage queued this frame. A zombie brought to no health dies:
/// its `deathFunction` runs (`zombie_death_animscript`), it leaves the level
/// and the engine, `death` is notified on it and every thread it owns ends,
/// as freeing the entity does. `zombie_death_event` then counts it.
pub fn apply_zombie_damage(level: &mut Level, threads: &mut Scheduler<Level>) {
    for damage in std::mem::take(&mut level.pending_damage) {
        let Some(zombie) = level.zombies.get_mut(&damage.actor) else {
            continue;
        };
        zombie.health -= damage.amount;
        if zombie.health > 0 {
            continue;
        }
        let Some(zombie) = level.zombies.remove(&damage.actor) else {
            continue;
        };
        crate::spawner::zombie_death_animscript(level, &damage);
        level.commands.push(EngineCommand::DeleteActor {
            actor: damage.actor,
        });
        let owner = actor_owner(damage.actor);
        threads.notify(owner, DEATH, Vec::new());
        threads.kill_owner(owner);
        // Retail's `level thread zombie_death_event( self )` waits from the
        // spawn; started here with the corpse, it cannot miss a death that
        // lands before it would have been waiting. It runs with the next
        // frame's threads, one frame after retail counts the kill.
        threads.spawn(
            Owner::LEVEL,
            ZombieDeathEvent::new(Corpse {
                actor: damage.actor,
                attacker: damage.attacker,
                ignore_all: zombie.ignore_all,
            }),
        );
    }
}
