//! `maps\_zombiemode_spawner.gsc`: what a zombie does once spawned. It walks
//! to the window it was sent to (`zombie_think`, `zombie_goto_entrance`),
//! then hunts the closest player (`find_flesh`, `zombie_follow_enemy`).
//!
//! Not ported yet: rising from the ground and window traversals (ZMB-036),
//! tearing the boards off (`tear_into_building`, ZMB-037: until then a zombie
//! at its window goes straight to `find_flesh`), `zombie_assure_node`, the
//! breadcrumbs of `zombie_pathing` for an unreachable player, points of
//! interest, and the ignore list that splits a crowd between players.

use gsc_threads::{Cx, Owner, Thread, Yield};

use crate::Level;
use crate::actor::{self, DEATH, GOAL, Orient, actor_owner};

/// `level endon( "intermission" )`.
pub const INTERMISSION: &str = "intermission";
/// `self notify( "zombie_acquire_enemy" )`: `find_flesh` picks again.
pub const ZOMBIE_ACQUIRE_ENEMY: &str = "zombie_acquire_enemy";
/// `self notify( "path_timer_done" )`.
pub const PATH_TIMER_DONE: &str = "path_timer_done";

/// `level.exterior_goals = getstructarray( "exterior_goal", "targetname" )`
/// (`maps\_zombiemode_blockers.gsc::init`).
pub const EXTERIOR_GOAL: &str = "exterior_goal";

/// `max_dist` in `zombie_think`, when the level sets no override.
pub const MAX_BARRIER_SEARCH_DIST: f32 = 500.0;
/// `self.goalradius` on the way to the window.
pub const ENTRANCE_GOAL_RADIUS: f32 = 128.0;
/// `self.goalradius` in `find_flesh`.
pub const FIND_FLESH_GOAL_RADIUS: f32 = 32.0;
/// `self.meleeAttackDist` from `zombie_setup_attack_properties`.
pub const MELEE_ATTACK_DIST: f32 = 64.0;

/// The actor number of a zombie's script owner.
pub fn actor_number(owner: Owner) -> u32 {
    (owner.0 - actor_owner(0).0) as u32
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// `self SetGoalPos( goal )` with `self.goalradius = radius`.
fn set_goal_pos(level: &mut Level, actor: u32, goal: [f32; 3], radius: f32) {
    let graph = std::sync::Arc::clone(&level.path_graph);
    if let Some(zombie) = level.zombies.get_mut(&actor) {
        zombie.motor.set_goal_pos(&graph, goal, radius);
    }
}

/// `maps\_zombiemode_spawner.gsc::get_desired_origin`: the origin of the
/// entity, struct or node the spawner targets.
pub fn get_desired_origin(level: &Level, spawner: u32) -> Option<[f32; 3]> {
    let target = level.ents.get(spawner)?.get("target")?;
    let ent = *level.ents.array(target, "targetname").first()?;
    level.ents.get(ent).map(|ent| ent.origin())
}

/// `common_scripts\utility.gsc::get_array_of_closest( org, array, undefined,
/// max )`: the closest `max` origins to `org`, nearest first.
pub fn get_array_of_closest(org: [f32; 3], array: &[[f32; 3]], max: usize) -> Vec<[f32; 3]> {
    let mut sorted = array.to_vec();
    sorted.sort_by(|a, b| distance(org, *a).total_cmp(&distance(org, *b)));
    sorted.truncate(max);
    sorted
}

/// The entrance a zombie from `spawner` standing at `origin` walks to: of the
/// three windows closest to the spawner's target, those not more than
/// `max_dist` further (from the zombie) than the previous one, then one of
/// them at random. `None` when the map has no `exterior_goal`.
pub fn pick_entrance(level: &mut Level, spawner: u32, origin: [f32; 3]) -> Option<[f32; 3]> {
    let goals: Vec<[f32; 3]> = level
        .ents
        .array(EXTERIOR_GOAL, "targetname")
        .into_iter()
        .filter_map(|ent| level.ents.get(ent).map(|ent| ent.origin()))
        .collect();
    let from = get_desired_origin(level, spawner).unwrap_or(origin);
    let nodes = get_array_of_closest(from, &goals, 3);
    let first = *nodes.first()?;
    let mut desired = vec![first];
    // Retail measures these from the zombie, not from the desired origin.
    let mut prev_dist = distance(origin, first);
    for node in &nodes[1..] {
        let dist = distance(origin, *node);
        if dist - prev_dist > MAX_BARRIER_SEARCH_DIST {
            break;
        }
        prev_dist = dist;
        desired.push(*node);
    }
    if desired.len() > 1 {
        let pick = level.random_int(desired.len() as i32) as usize;
        return Some(desired[pick]);
    }
    Some(first)
}

/// `maps\_zombiemode_utility.gsc::get_closest_valid_player`: the nearest
/// living player (the ignore list is not ported).
pub fn get_closest_valid_player(level: &Level, origin: [f32; 3]) -> Option<i32> {
    level
        .players
        .iter()
        .filter(|player| player.alive)
        .min_by(|a, b| distance(origin, a.origin).total_cmp(&distance(origin, b.origin)))
        .map(|player| player.entnum)
}

/// `maps\_zombiemode_spawner.gsc::zombie_think`, for a zombie that neither
/// rises nor has a target that skips the window.
#[derive(Clone, Debug, Default)]
pub struct ZombieThink;

impl Thread<Level> for ZombieThink {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        let owner = cx.owner();
        let actor = actor_number(owner);
        let level = &mut *cx.world;
        let Some(zombie) = level.zombies.get(&actor) else {
            return Yield::Done;
        };
        let (spawner, origin) = (zombie.spawner, zombie.motor.origin);
        match pick_entrance(level, spawner, origin) {
            Some(node) => cx.thread(owner, ZombieGotoEntrance::new(node)),
            None => {
                // A map with no windows: hunt at once.
                zombie_setup_attack_properties(level, actor);
                cx.thread(owner, FindFlesh::default());
            }
        }
        Yield::Done
    }
}

/// `maps\_zombiemode_spawner.gsc::zombie_setup_attack_properties`.
pub fn zombie_setup_attack_properties(level: &mut Level, actor: u32) {
    if let Some(zombie) = level.zombies.get_mut(&actor) {
        zombie.melee_attack_dist = MELEE_ATTACK_DIST;
    }
}

/// `maps\_zombiemode_spawner.gsc::zombie_goto_entrance`.
#[derive(Clone, Debug)]
pub struct ZombieGotoEntrance {
    node: [f32; 3],
    pc: u8,
}

impl ZombieGotoEntrance {
    pub fn new(node: [f32; 3]) -> Self {
        Self { node, pc: 0 }
    }
}

impl Thread<Level> for ZombieGotoEntrance {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        let owner = cx.owner();
        let actor = actor_number(owner);
        if self.pc == 0 {
            self.pc = 1;
            cx.endon(owner, DEATH);
            cx.endon(Owner::LEVEL, INTERMISSION);
            set_goal_pos(cx.world, actor, self.node, ENTRANCE_GOAL_RADIUS);
            return Yield::waittill(owner, GOAL);
        }
        // `tear_into_building()` and the window traversal come with ZMB-037
        // and ZMB-036.
        zombie_setup_attack_properties(cx.world, actor);
        cx.thread(owner, FindFlesh::default());
        Yield::Done
    }
}

/// `maps\_zombiemode_spawner.gsc::find_flesh`: every 1–3 s, the closest
/// valid player becomes the favourite enemy and `zombie_follow_enemy`
/// restarts on it.
#[derive(Clone, Debug, Default)]
pub struct FindFlesh {
    pc: u8,
}

impl Thread<Level> for FindFlesh {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        let owner = cx.owner();
        let actor = actor_number(owner);
        loop {
            match self.pc {
                0 => {
                    cx.endon(owner, DEATH);
                    cx.endon(Owner::LEVEL, INTERMISSION);
                    if cx.world.intermission {
                        return Yield::Done;
                    }
                    cx.thread(owner, crate::zombie_melee::CombatWatch::default());
                    self.pc = 1;
                }
                1 => {
                    let level = &mut *cx.world;
                    let Some(origin) = level.zombies.get(&actor).map(|z| z.motor.origin) else {
                        return Yield::Done;
                    };
                    let Some(player) = get_closest_valid_player(level, origin) else {
                        return Yield::wait_seconds(1.0);
                    };
                    if let Some(zombie) = level.zombies.get_mut(&actor) {
                        zombie.favorite_enemy = Some(player);
                    }
                    // `self thread zombie_pathing()`, which with an enemy and
                    // no point of interest is `zombie_follow_enemy()`.
                    let delay = level.random_float_range(1.0, 3.0);
                    cx.thread(owner, ZombieFollowEnemy::default());
                    self.pc = 2;
                    return Yield::wait_seconds(delay);
                }
                _ => {
                    cx.notify(owner, PATH_TIMER_DONE, Vec::new());
                    cx.notify(owner, ZOMBIE_ACQUIRE_ENEMY, Vec::new());
                    self.pc = 1;
                }
            }
        }
    }
}

/// `maps\_zombiemode_spawner.gsc::zombie_follow_enemy`: every tenth of a
/// second (plus a pause when far away), the goal is the enemy's origin.
#[derive(Clone, Debug, Default)]
pub struct ZombieFollowEnemy {
    started: bool,
}

impl Thread<Level> for ZombieFollowEnemy {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        let owner = cx.owner();
        let actor = actor_number(owner);
        if !self.started {
            self.started = true;
            cx.endon(owner, DEATH);
            cx.endon(owner, ZOMBIE_ACQUIRE_ENEMY);
            cx.endon(owner, actor::BAD_PATH);
            cx.endon(Owner::LEVEL, INTERMISSION);
        }
        let level = &mut *cx.world;
        let Some(zombie) = level.zombies.get(&actor) else {
            return Yield::Done;
        };
        let origin = zombie.motor.origin;
        let enemy = zombie
            .favorite_enemy
            .and_then(|entnum| level.player(entnum))
            .map(|player| player.origin);
        let mut extra_wait = 0.0;
        if let Some(enemy) = enemy {
            if let Some(zombie) = level.zombies.get_mut(&actor)
                && !zombie.meleeing
            {
                zombie.motor.orient = Orient::Motion;
            }
            set_goal_pos(level, actor, enemy, FIND_FLESH_GOAL_RADIUS);
            let dist = distance(origin, enemy);
            extra_wait = if dist > 3200.0 {
                2.0 + level.random_float_range(0.0, 1.0)
            } else if dist > 2200.0 {
                1.0 + level.random_float_range(0.0, 0.5)
            } else if dist > 1200.0 {
                0.5 + level.random_float_range(0.0, 0.5)
            } else {
                0.0
            };
        }
        Yield::wait_seconds(extra_wait + 0.1)
    }
}
