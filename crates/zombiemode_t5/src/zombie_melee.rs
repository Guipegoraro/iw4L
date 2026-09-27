//! `animscripts\zombie_melee.gsc`: a zombie close enough to its enemy stops,
//! faces it and swings; each `fire` note of the swing is the engine's
//! `melee()`, which hurts the enemy if it is still in reach.
//!
//! Retail enters `MeleeCombat` from the combat animscript when
//! `CanMeleeAnyRange()` passes; [`CombatWatch`] stands in for that check with
//! its distance test only (no facing test, no one-melee-per-target rule).

use gsc_threads::{Cx, Thread, Value, Yield, call};
use sim::ClientId;

use crate::Level;
use crate::actor::{AnimMode, DEATH, Orient, actor_number};
use crate::level::EngineCommand;

/// `self waittill( "meleeanim", note )`: a note of the swing clip.
pub const MELEEANIM: &str = "meleeanim";
/// The swing's hit.
pub const NOTE_FIRE: &str = "fire";
/// The swing's end.
pub const NOTE_END: &str = "end";
/// Where a swing may stop early when the enemy has left reach.
pub const NOTE_STOP: &str = "stop";

/// `GetDvarFloat( "ai_meleeRange" )`: how far the engine's `melee()` reaches.
/// The dvar default, not read from the zone.
pub const AI_MELEE_RANGE: f32 = 64.0;

/// A zombie and its living enemy.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Engagement {
    origin: [f32; 3],
    enemy: ClientId,
    enemy_origin: [f32; 3],
    melee_attack_dist: f32,
}

impl Engagement {
    fn of(level: &Level, actor: u32) -> Option<Self> {
        let zombie = level.zombies.get(&actor)?;
        let enemy = zombie.favorite_enemy?;
        let player = level.player(enemy).filter(|player| player.alive)?;
        Some(Self {
            origin: zombie.motor.origin,
            enemy,
            enemy_origin: player.origin,
            melee_attack_dist: zombie.melee_attack_dist,
        })
    }

    fn distance(&self) -> f32 {
        math_iw4::vec3_distance(self.origin, self.enemy_origin)
    }

    /// Close enough to start or keep swinging (`meleeAttackDist`).
    fn in_reach(&self) -> bool {
        self.distance() <= self.melee_attack_dist
    }
}

fn in_melee_range(level: &Level, actor: u32) -> bool {
    Engagement::of(level, actor).is_some_and(|engagement| engagement.in_reach())
}

/// Every server frame: an enemy within `meleeAttackDist` starts
/// [`MeleeCombat`], which returns here when it ends.
#[derive(Clone, Debug, Default)]
pub struct CombatWatch {
    started: bool,
}

impl Thread<Level> for CombatWatch {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        let owner = cx.owner();
        if !self.started {
            self.started = true;
            cx.endon(owner, DEATH);
        }
        let actor = actor_number(owner);
        let level = &mut *cx.world;
        let idle = level
            .zombies
            .get(&actor)
            .is_some_and(|zombie| !zombie.meleeing);
        if idle && in_melee_range(level, actor) {
            if let Some(zombie) = level.zombies.get_mut(&actor) {
                zombie.meleeing = true;
            }
            cx.thread(owner, MeleeCombatThenWatch::default());
            return Yield::Done;
        }
        Yield::WaitFrame
    }
}

/// `MeleeCombat()` then back to watching, as retail's combat script loops.
#[derive(Clone, Debug, Default)]
struct MeleeCombatThenWatch {
    melee: MeleeCombat,
}

impl Thread<Level> for MeleeCombatThenWatch {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        if let Some(wait) = call(&mut self.melee, cx) {
            return wait;
        }
        let owner = cx.owner();
        cx.thread(owner, CombatWatch::default());
        Yield::Done
    }
}

/// `animscripts\zombie_melee.gsc::MeleeCombat`: swing after swing while the
/// enemy stays in reach.
#[derive(Clone, Debug, Default)]
pub struct MeleeCombat {
    pc: u8,
    started: bool,
}

impl MeleeCombat {
    fn finish(level: &mut Level, actor: u32) {
        if let Some(zombie) = level.zombies.get_mut(&actor) {
            zombie.end_melee();
        }
    }
}

impl Thread<Level> for MeleeCombat {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        let owner = cx.owner();
        let actor = actor_number(owner);
        loop {
            match self.pc {
                0 => {
                    if !self.started {
                        self.started = true;
                        cx.endon(owner, DEATH);
                    }
                    let level = &mut *cx.world;
                    let (Some(engagement), Some(speed)) = (
                        Engagement::of(level, actor),
                        level.zombies.get(&actor).map(|zombie| zombie.move_speed),
                    ) else {
                        Self::finish(level, actor);
                        return Yield::Done;
                    };
                    let clip = crate::anims::pick_zombie_melee_anim(level, speed);
                    if let Some(zombie) = level.zombies.get_mut(&actor) {
                        // `self AnimMode( "zonly_physics" )`, and
                        // `OrientMode( "face angle", VectorToAngles(
                        // enemy - self )[1] )`.
                        zombie.motor.anim_mode = AnimMode::InPlace;
                        let (from, to) = (engagement.origin, engagement.enemy_origin);
                        zombie.motor.orient =
                            Orient::Angle(math_iw4::vec_to_yaw(to[0] - from[0], to[1] - from[1]));
                        zombie.motor.play_scripted(clip);
                    }
                    self.pc = 1;
                    return Yield::waittill(owner, MELEEANIM);
                }
                1 => {
                    let note = match cx.event().and_then(|event| event.args.first()) {
                        Some(Value::Str(note)) => note.as_str().to_owned(),
                        _ => String::new(),
                    };
                    let level = &mut *cx.world;
                    // Each `break` of the note loop: the end, a hit with no
                    // enemy left, or `stop` when `CanContinueToMelee()` fails
                    // (stood in for by the reach test).
                    let engagement = Engagement::of(level, actor);
                    let leave = match note.as_str() {
                        NOTE_END => true,
                        NOTE_FIRE => engagement.is_none(),
                        NOTE_STOP => !engagement.is_some_and(|e| e.in_reach()),
                        _ => false,
                    };
                    if leave {
                        self.pc = 2;
                        continue;
                    }
                    if note == NOTE_FIRE
                        && let Some(engagement) = engagement
                        && engagement.distance() <= AI_MELEE_RANGE
                    {
                        let amount = level
                            .zombies
                            .get(&actor)
                            .map_or(0, |zombie| zombie.melee_damage);
                        level.commands.push(EngineCommand::MeleePlayer {
                            client: engagement.enemy,
                            amount,
                            from: engagement.origin,
                        });
                    }
                    return Yield::waittill(owner, MELEEANIM);
                }
                _ => {
                    // Another swing while the enemy is still in reach.
                    if in_melee_range(cx.world, actor) {
                        self.pc = 0;
                        continue;
                    }
                    Self::finish(cx.world, actor);
                    return Yield::Done;
                }
            }
        }
    }
}
