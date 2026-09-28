//! Rules a mode runs as scripts: Black Ops zombies is the first. A mode script
//! is installed once per match and gets one frame per authoritative step,
//! after entities have thought, as the retail server runs script threads after
//! `G_RunFrame`'s entity pass.
//!
//! [`ModeEngine`] is the script's view of the engine, the Rust side of the GSC
//! builtins it calls. It grows one builtin at a time, as ported scripts need
//! them.

use core::fmt;

use crate::frame::FrameWorld;
use crate::match_state::ClientLifecycle;
use crate::world::{ClientId, Tick};

/// How far above an actor the ground trace starts: the height it may step up,
/// the same step a player takes.
pub const ACTOR_STEP_HEIGHT: f32 = movement_iw4::STEP_SIZE;

/// Every contents bit: a volume is touched whatever its brushes are made of.
const ALL_CONTENTS: u32 = u32::MAX;

/// How far below an actor the ground trace looks: the drop it may step down.
pub const ACTOR_GROUND_PROBE: f32 = 64.0;

pub trait ModeScript: fmt::Debug + Send + Sync {
    fn clone_box(&self) -> Box<dyn ModeScript>;

    /// One server frame.
    fn frame(&mut self, engine: &mut ModeEngine<'_, '_>);
}

/// Where the installed script lives in the simulation state; cloned with it.
#[derive(Default)]
pub struct ModeScriptSlot(Option<Box<dyn ModeScript>>);

impl ModeScriptSlot {
    pub(crate) fn take(&mut self) -> Option<Box<dyn ModeScript>> {
        self.0.take()
    }

    pub(crate) fn put(&mut self, script: Box<dyn ModeScript>) {
        self.0 = Some(script);
    }

    pub(crate) fn set(&mut self, script: Option<Box<dyn ModeScript>>) {
        self.0 = script;
    }

    pub fn is_installed(&self) -> bool {
        self.0.is_some()
    }
}

impl Clone for ModeScriptSlot {
    fn clone(&self) -> Self {
        Self(self.0.as_ref().map(|script| script.clone_box()))
    }
}

impl fmt::Debug for ModeScriptSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(script) => fmt::Debug::fmt(script, f),
            None => f.write_str("None"),
        }
    }
}

/// A player as `GetPlayers()` returns it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptPlayer {
    pub client: ClientId,
    /// The player's entity number, which is also its script `self`.
    pub entnum: i32,
    pub alive: bool,
    pub origin: [f32; 3],
    pub team: i32,
    /// The lethal grenade slot: weapon, ammo held, and its max ammo.
    pub lethal: Option<ScriptAmmo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScriptAmmo {
    pub weapon: u32,
    pub clip: i32,
    pub stock: i32,
    pub max_ammo: i32,
}

impl ScriptAmmo {
    /// `GetFractionMaxAmmo`: everything held over the weapon's max ammo.
    pub fn fraction_max_ammo(self) -> f32 {
        (self.clip + self.stock) as f32 / self.max_ammo.max(1) as f32
    }
}

pub struct ModeEngine<'a, 'w> {
    world: &'a mut FrameWorld<'w>,
    tick: Tick,
}

impl<'a, 'w> ModeEngine<'a, 'w> {
    pub(crate) fn new(world: &'a mut FrameWorld<'w>, tick: Tick) -> Self {
        Self { world, tick }
    }

    /// `GetTime()`: milliseconds since the match started.
    pub fn now_ms(&self) -> u64 {
        u64::from(self.tick.0) * u64::from(crate::MATCH_TICK_MS)
    }

    /// `GetPlayers()`: connected players in client order.
    pub fn players(&self) -> Vec<ScriptPlayer> {
        self.world
            .client_ids_sorted()
            .into_iter()
            .filter_map(|client| {
                let meta = self.world.client_meta(client)?;
                let alive = meta.lifecycle == ClientLifecycle::Alive;
                let origin = self
                    .world
                    .player(client)
                    .map_or([0.0; 3], |player| player.origin);
                let lethal = meta
                    .loadout
                    .as_ref()
                    .map(|loadout| loadout.lethal)
                    .filter(|weapon| *weapon != 0)
                    .map(|weapon| {
                        let (clip, stock) = meta.ammo_for(weapon);
                        let max_ammo = self
                            .world
                            .combat_facts_for(weapon)
                            .map_or(0, |facts| facts.max_ammo);
                        ScriptAmmo {
                            weapon,
                            clip,
                            stock,
                            max_ammo,
                        }
                    });
                Some(ScriptPlayer {
                    client,
                    entnum: client.0 as i32,
                    alive,
                    origin,
                    team: meta.client_state_team,
                    lethal,
                })
            })
            .collect()
    }

    /// `player SetWeaponAmmoClip(weapon, clip)`: the reserve stays as it is.
    pub fn set_weapon_ammo_clip(&mut self, client: ClientId, weapon: u32, clip: i32) {
        let Some(facts) = self.world.combat_facts_for(weapon) else {
            return;
        };
        let Some(meta) = self.world.client_meta(client) else {
            return;
        };
        let (_, stock) = meta.ammo_for(weapon);
        let Some(ps) = self.world.player_mut(client) else {
            return;
        };
        crate::step::seed_ps_ammo_tables(ps, weapon, &facts, clip, 0, false, stock);
        self.world
            .client_meta_mut(client)
            .set_ammo(weapon, clip, stock);
    }

    /// `Spawner DoSpawn()` plus the character's `SetModel`/`Attach`: an actor
    /// `actor` (the script's own number) wearing `body` and `attachments`.
    /// Returns its entity number.
    pub fn spawn_actor(
        &mut self,
        actor: u32,
        body: &str,
        attachments: &[crate::actors::ActorAttachment],
        origin: [f32; 3],
        yaw: f32,
    ) -> Result<i32, crate::actors::ActorSpawnError> {
        crate::actors::spawn(self.world, actor, body, attachments, origin, yaw)
    }

    /// Plays one clip on an actor from its first frame.
    pub fn actor_play_anim(&mut self, actor: u32, clip: &str, looping: bool, rate: f32) -> bool {
        crate::actors::play_anim(self.world, actor, clip, looping, rate)
    }

    /// Puts an actor at `origin` facing `yaw` degrees.
    pub fn set_actor_origin(&mut self, actor: u32, origin: [f32; 3], yaw: f32) -> bool {
        crate::actors::set_origin(self.world, actor, origin, yaw)
    }

    /// `Delete()`.
    pub fn delete_actor(&mut self, actor: u32) -> bool {
        crate::actors::delete(self.world, actor)
    }

    pub fn has_actor_model(&self, model: &str) -> bool {
        self.world.actor_models().contains(model)
    }

    /// `player IsTouching( volume )` for a brush-model volume (`"model"
    /// "*N"`, `model` = N) placed at `origin` and `angles`: the player's
    /// linked box against the model's brushes, whatever their contents. False
    /// for a player not linked in the world or a model the map does not have.
    pub fn player_touches_brush_model(
        &self,
        client: ClientId,
        model: u32,
        origin: [f32; 3],
        angles: [f32; 3],
    ) -> bool {
        let Some(bounds) = self.world.player_area_bounds(client) else {
            return false;
        };
        let Some(cmodel) =
            clipmap_iw4::clip_handle_to_model(&self.world.clip_cmodels().models, model)
        else {
            return false;
        };
        let (mid, half) = (bounds.mid(), bounds.half());
        let trace = clipmap_iw4::transformed_capsule_trace(
            cmodel,
            &self.world.clip_bsp().leafbrushes,
            self.world.clip_brushes(),
            mid,
            mid,
            [-half[0], -half[1], -half[2]],
            half,
            origin,
            angles,
            ALL_CONTENTS,
        );
        trace.startsolid != 0
    }

    /// An installed actor animation: its length, root motion and notes.
    pub fn actor_clip(&self, clip: &str) -> Option<std::sync::Arc<xmodel_runtime::AnimClip>> {
        self.world.actor_clips().get(clip).cloned()
    }

    /// The ground under `origin`: a point trace from a step above it to
    /// `ACTOR_GROUND_PROBE` below. `None` when nothing is there.
    pub fn ground_z(&self, origin: [f32; 3]) -> Option<f32> {
        let start = [origin[0], origin[1], origin[2] + ACTOR_STEP_HEIGHT];
        let end = [origin[0], origin[1], origin[2] - ACTOR_GROUND_PROBE];
        let trace = self.world.trace_world(
            start,
            end,
            [0.0; 3],
            [0.0; 3],
            crate::bullet_collision::MASK_PLAYER_SOLID,
        );
        (trace.fraction < 1.0 && trace.startsolid == 0)
            .then(|| start[2] + (end[2] - start[2]) * trace.fraction)
    }

    /// `player DoDamage( amount, origin, attacker, 0, "MOD_MELEE" )` from an
    /// actor at `from`: the host's melee damage on the player. An actor is not
    /// a client, so the player is its own attacker, as world damage is.
    pub fn melee_player(&mut self, client: ClientId, amount: i32, from: [f32; 3]) -> bool {
        let Some(meta) = self.world.client_meta(client) else {
            return false;
        };
        let life = meta.life_sequence;
        let attempt = crate::DamageAttempt {
            source: crate::DamageSource::Melee,
            pellet: crate::PelletId(0),
            attacker: client,
            attacker_life: life,
            target: client,
            target_life: life,
            weapon: 0,
            amount,
            killcam_entity_start_time: 0,
            inflictor_origin: Some(from),
            hitloc: 0,
        };
        !matches!(
            crate::damage::apply_damage_attempt(self.world, self.tick, &attempt),
            crate::damage::DamageOutcome::Refused(_)
        )
    }
}
