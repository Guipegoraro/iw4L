//! What the zombies threads work on: GSC's `level` variables plus the frame's
//! view of the engine.
//!
//! The engine view is read once at the start of the frame; a thread does not
//! see what another thread changed in the engine earlier in the same frame,
//! where retail would. Script-side state (`level.*`) is shared and immediate.
//! What a script asks of the engine is queued in [`Level::commands`] and applied
//! after the frame's threads have run.

use std::collections::BTreeMap;
use std::sync::Arc;

use sim::ScriptPlayer;

use crate::mapents::MapEnts;
use crate::table::StringTables;
use crate::utility::ZombieVar;
use crate::zone_manager::Zone;

/// A player's points (`self.score`, `self.score_total`, `self.old_score`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerScore {
    pub score: i32,
    pub score_total: i32,
    pub old_score: i32,
}

/// A builtin that changes the engine, queued for after the frame.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineCommand {
    /// `player SetWeaponAmmoClip( weapon, clip )`.
    SetWeaponAmmoClip { entnum: i32, weapon: u32, clip: i32 },
    /// `spawner StalingradSpawn()` / `DoSpawn()` with the character's models,
    /// then its first animation.
    SpawnActor {
        actor: u32,
        body: &'static str,
        head: &'static str,
        origin: [f32; 3],
        yaw: f32,
        anim: &'static str,
    },
    /// `player DoDamage( amount, origin, zombie, 0, "MOD_MELEE" )` from the
    /// engine's `melee()`.
    MeleePlayer {
        entnum: i32,
        amount: i32,
        from: [f32; 3],
    },
}

/// A zombie the script spawned, keyed by its actor number.
#[derive(Clone, Debug, PartialEq)]
pub struct Zombie {
    /// The spawner's entity index.
    pub spawner: u32,
    pub health: i32,
    pub body: &'static str,
    pub head: &'static str,
    /// Where it is and what it walks to: the engine half of the actor.
    pub motor: crate::actor::Motor,
    /// `self.zombie_move_speed`.
    pub move_speed: crate::anims::MoveSpeed,
    /// `self.favoriteenemy`: a player's entity number.
    pub favorite_enemy: Option<i32>,
    /// `self.meleeDamage`.
    pub melee_damage: i32,
    /// `self.meleeAttackDist`, set by `zombie_setup_attack_properties`.
    pub melee_attack_dist: f32,
    /// In `MeleeCombat`.
    pub meleeing: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Level {
    /// `level.script`: the map's name.
    pub script: String,

    /// The string tables the map loaded, for `TableLookUp`.
    pub tables: Arc<StringTables>,

    /// The map's path nodes, for AI routes.
    pub path_graph: Arc<pathnodes::PathGraph>,

    /// `GetTime()` at the start of this frame.
    pub now_ms: u64,

    /// `GetPlayers()` at the start of this frame.
    pub players: Vec<ScriptPlayer>,

    /// `level.zombie_vars`.
    pub zombie_vars: BTreeMap<String, ZombieVar>,

    pub round_number: i32,
    pub first_round: bool,
    pub intermission: bool,
    pub zombie_total: i32,
    pub zombie_health: i32,
    pub zombie_move_speed: i32,
    pub zombie_ai_limit: i32,
    pub total_zombies_killed: i32,

    /// `level.enemy_spawns`: the active zombie spawners' entity indices,
    /// rebuilt by the zone manager every second.
    pub enemy_spawns: Vec<u32>,

    /// The map's entities (spawners, volumes, structs).
    pub ents: Arc<MapEnts>,

    /// `level.zones`.
    pub zones: BTreeMap<String, Zone>,

    /// Zombies alive, by actor number.
    pub zombies: BTreeMap<u32, Zombie>,
    pub next_actor: u32,

    /// `RandomInt` state.
    pub rng: u64,

    /// Each player's points, by entity number.
    pub scores: BTreeMap<i32, PlayerScore>,

    /// `println(...)` lines, written to the log after the frame.
    pub println: Vec<String>,

    pub commands: Vec<EngineCommand>,
}

impl Level {
    pub fn new(
        script: impl Into<String>,
        tables: StringTables,
        path_graph: Arc<pathnodes::PathGraph>,
        ents: MapEnts,
    ) -> Self {
        Self {
            script: script.into(),
            tables: Arc::new(tables),
            path_graph,
            ents: Arc::new(ents),
            rng: 0x9e37_79b9_7f4a_7c15,
            ..Self::default()
        }
    }

    /// `RandomInt( max )`: `0..max`, 0 when `max` is not positive. The
    /// sequence is the match's own (seeded the same every match), not
    /// retail's.
    pub fn random_int(&mut self, max: i32) -> i32 {
        if max <= 0 {
            return 0;
        }
        // xorshift64*
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        (x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 33) as i32 % max
    }

    /// `RandomFloatRange( min, max )`: `min..max`, from the same sequence as
    /// [`Level::random_int`].
    pub fn random_float_range(&mut self, min: f32, max: f32) -> f32 {
        const STEPS: i32 = 1 << 24;
        let unit = self.random_int(STEPS) as f32 / STEPS as f32;
        min + (max - min) * unit
    }

    /// A player from this frame's `GetPlayers()` by entity number.
    pub fn player(&self, entnum: i32) -> Option<&ScriptPlayer> {
        self.players.iter().find(|player| player.entnum == entnum)
    }

    /// `get_enemy_count()`: zombies alive.
    pub fn enemy_count(&self) -> i32 {
        self.zombies.len() as i32
    }

    /// `println(...)`.
    pub fn println(&mut self, line: impl Into<String>) {
        self.println.push(line.into());
    }

    /// `level.zombie_vars[ var ]`; an unset var reads as 0, as undefined does
    /// in arithmetic.
    pub fn zombie_var(&self, var: &str) -> ZombieVar {
        self.zombie_vars
            .get(var)
            .copied()
            .unwrap_or(ZombieVar::Int(0))
    }
}
