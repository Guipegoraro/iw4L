//! `animscripts\zombie_melee.gsc`: a zombie close enough to its enemy stops,
//! faces it and swings; each `fire` note of the swing is the engine's
//! `melee()`, which hurts the enemy if it is still in reach.
//!
//! Retail enters `MeleeCombat` from the combat animscript when
//! `CanMeleeAnyRange()` passes; [`CombatWatch`] stands in for that check with
//! its distance test only (no facing test, no one-melee-per-target rule).

use gsc_threads::{Cx, Thread, Value, Yield, call};

use crate::Level;
use crate::actor::{AnimMode, DEATH, Orient};
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

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The zombie's origin and its living enemy's, when it has one.
fn zombie_and_enemy(level: &Level, actor: u32) -> Option<([f32; 3], i32, [f32; 3])> {
    let zombie = level.zombies.get(&actor)?;
    let entnum = zombie.favorite_enemy?;
    let player = level.player(entnum).filter(|player| player.alive)?;
    Some((zombie.motor.origin, entnum, player.origin))
}

fn in_melee_range(level: &Level, actor: u32) -> bool {
    let Some(zombie) = level.zombies.get(&actor) else {
        return false;
    };
    zombie_and_enemy(level, actor)
        .is_some_and(|(origin, _, enemy)| distance(origin, enemy) <= zombie.melee_attack_dist)
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
        let actor = crate::spawner::actor_number(owner);
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
            zombie.meleeing = false;
            zombie.motor.stop_scripted();
            zombie.motor.anim_mode = AnimMode::Walk;
            // `self OrientMode( "face default" )`.
            zombie.motor.orient = Orient::Motion;
        }
    }
}

impl Thread<Level> for MeleeCombat {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        let owner = cx.owner();
        let actor = crate::spawner::actor_number(owner);
        loop {
            match self.pc {
                0 => {
                    if !self.started {
                        self.started = true;
                        cx.endon(owner, DEATH);
                    }
                    let level = &mut *cx.world;
                    let (Some((origin, _, enemy)), Some(speed)) = (
                        zombie_and_enemy(level, actor),
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
                        zombie.motor.orient = Orient::Angle(math_iw4::vec_to_yaw(
                            enemy[0] - origin[0],
                            enemy[1] - origin[1],
                        ));
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
                    let enemy = zombie_and_enemy(level, actor);
                    let leave = match note.as_str() {
                        NOTE_END => true,
                        NOTE_FIRE => enemy.is_none(),
                        NOTE_STOP => !in_melee_range(level, actor),
                        _ => false,
                    };
                    if leave {
                        self.pc = 2;
                        continue;
                    }
                    if note == NOTE_FIRE
                        && let Some((origin, entnum, enemy)) = enemy
                        && distance(origin, enemy) <= AI_MELEE_RANGE
                    {
                        let amount = level
                            .zombies
                            .get(&actor)
                            .map_or(0, |zombie| zombie.melee_damage);
                        level.commands.push(EngineCommand::MeleePlayer {
                            entnum,
                            amount,
                            from: origin,
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
