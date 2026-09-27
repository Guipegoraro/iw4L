//! `maps\_zombiemode_spawner.gsc`: what a zombie does once spawned. It walks
//! to the window it was sent to (`zombie_think`, `zombie_goto_entrance`),
//! then hunts the closest player (`find_flesh`, `zombie_follow_enemy`).
//!
//! Not ported yet: rising from the ground and window traversals (ZMB-036),
//! tearing the boards off (`tear_into_building`, ZMB-037: until then a zombie
//! at its window goes straight to `find_flesh`), the timeout death at the end
//! of `zombie_assure_node` (ZMB-041), the breadcrumbs of `zombie_pathing` for an unreachable player, points of
//! interest, and the ignore list that splits a crowd between players.

use gsc_threads::{Cx, Owner, Thread, Yield, call};

use crate::Level;
use crate::actor::{self, BAD_PATH, DEATH, GOAL, Orient, actor_owner};
use crate::mapents::{SCRIPT_STRING, TARGET, TARGETNAME};
use crate::zombiemode::INTERMISSION;

/// `self notify( "zombie_acquire_enemy" )`: `find_flesh` picks again.
pub const ZOMBIE_ACQUIRE_ENEMY: &str = "zombie_acquire_enemy";
/// `self notify( "path_timer_done" )`.
pub const PATH_TIMER_DONE: &str = "path_timer_done";
/// `self notify( "stop_zombie_bad_path" )`: `zombie_bad_path()` has its answer.
pub const STOP_ZOMBIE_BAD_PATH: &str = "stop_zombie_bad_path";

/// `self.script_string == "zombie_chaser"`: skips the window.
pub const ZOMBIE_CHASER: &str = "zombie_chaser";

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
/// `get_array_of_closest( origin, level.exterior_goals, undefined, 3 )` in
/// `zombie_think`: the windows an untargeted zombie picks among.
pub const ENTRANCE_CANDIDATES: usize = 3;
/// `zombie_bad_path_timeout`: how long `zombie_bad_path()` waits for a
/// `bad_path` before it answers no, seconds.
pub const ZOMBIE_BAD_PATH_TIMEOUT: f32 = 2.0;
/// `wait( 0.05 )` in `zombie_bad_path()`'s poll, seconds.
pub const ZOMBIE_BAD_PATH_POLL: f32 = 0.05;
/// `wait( 2 )` in `zombie_assure_node` before it widens the search, seconds.
pub const ASSURE_NODE_RETRY_WAIT: f32 = 2.0;
/// `get_array_of_closest( self.origin, level.exterior_goals, undefined, 20 )`
/// in `zombie_assure_node`.
pub const ASSURE_NODE_CLOSEST: usize = 20;
/// `wait( 20 )` before `zombie_assure_node` gives up on the zombie, seconds.
pub const ASSURE_NODE_GIVE_UP: f32 = 20.0;
/// `wait( 1 )` in `find_flesh` when no player is valid, seconds.
pub const FIND_FLESH_NO_PLAYER_WAIT: f32 = 1.0;
/// `RandomFloatRange( 1, 3 )` in `find_flesh`: seconds before it picks an
/// enemy again.
pub const FIND_FLESH_REPICK: (f32, f32) = (1.0, 3.0);
/// `wait( 0.1 )` at the end of each `zombie_follow_enemy` pass, seconds.
pub const FOLLOW_ENEMY_TICK: f32 = 0.1;

/// One of `zombie_follow_enemy`'s distance tiers: an enemy further than
/// `beyond` adds `base + RandomFloat( spread )` seconds to the pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FollowTier {
    pub beyond: f32,
    pub base: f32,
    pub spread: f32,
}

/// `zombie_follow_enemy`'s tiers, furthest first (`distSq > 3200 * 3200`…).
pub const FOLLOW_ENEMY_TIERS: [FollowTier; 3] = [
    FollowTier {
        beyond: 3200.0,
        base: 2.0,
        spread: 1.0,
    },
    FollowTier {
        beyond: 2200.0,
        base: 1.0,
        spread: 0.5,
    },
    FollowTier {
        beyond: 1200.0,
        base: 0.5,
        spread: 0.5,
    },
];

/// The actor number of a zombie's script owner.
pub fn actor_number(owner: Owner) -> u32 {
    (owner.0 - actor_owner(0).0) as u32
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
    let target = level.ents.get(spawner)?.get(TARGET)?;
    let ent = *level.ents.array(target, TARGETNAME).first()?;
    level.ents.get(ent).map(|ent| ent.origin())
}

/// `common_scripts\utility.gsc::get_array_of_closest( org, array, undefined,
/// max )`: the closest `max` origins to `org`, nearest first.
pub fn get_array_of_closest(org: [f32; 3], array: &[[f32; 3]], max: usize) -> Vec<[f32; 3]> {
    let mut sorted = array.to_vec();
    sorted.sort_by(|a, b| {
        math_iw4::vec3_distance(org, *a).total_cmp(&math_iw4::vec3_distance(org, *b))
    });
    sorted.truncate(max);
    sorted
}

/// `level.exterior_goals`: every window's origin.
pub fn exterior_goals(level: &Level) -> Vec<[f32; 3]> {
    level
        .ents
        .array(EXTERIOR_GOAL, TARGETNAME)
        .into_iter()
        .filter_map(|ent| level.ents.get(ent).map(|ent| ent.origin()))
        .collect()
}

/// `should_skip_teardown`, for a zombie that did not rise: its spawner's
/// `script_string` is `zombie_chaser`.
pub fn should_skip_teardown(level: &Level, spawner: u32) -> bool {
    level
        .ents
        .get(spawner)
        .and_then(|ent| ent.get(SCRIPT_STRING))
        .is_some_and(|value| value == ZOMBIE_CHASER)
}

/// Where `zombie_think` sends a zombie.
#[derive(Clone, Debug, PartialEq)]
pub enum Entrance {
    /// The spawner has a target: the one window closest to it.
    Forced([f32; 3]),
    /// No target: `node`, picked at random among `entrance_nodes`, which
    /// `zombie_assure_node` falls back on.
    Picked {
        node: [f32; 3],
        entrance_nodes: Vec<[f32; 3]>,
    },
    /// `should_skip_teardown`: no window, straight to `find_flesh`.
    SkipTeardown,
}

/// `zombie_think`'s choice for a zombie from `spawner` standing at `origin`
/// (not a riser). `None` when the map has no `exterior_goal`.
pub fn pick_entrance(level: &mut Level, spawner: u32, origin: [f32; 3]) -> Option<Entrance> {
    let goals = exterior_goals(level);
    // `IsDefined( self.target ) && self.target != ""`: retail asserts the
    // target exists; a missing one falls through to the untargeted pick here.
    if let Some(target) = get_desired_origin(level, spawner) {
        return get_array_of_closest(target, &goals, 1)
            .first()
            .map(|node| Entrance::Forced(*node));
    }
    if should_skip_teardown(level, spawner) {
        return Some(Entrance::SkipTeardown);
    }
    let nodes = get_array_of_closest(origin, &goals, ENTRANCE_CANDIDATES);
    let first = *nodes.first()?;
    let mut entrance_nodes = vec![first];
    let mut prev_dist = math_iw4::vec3_distance(origin, first);
    for node in &nodes[1..] {
        let dist = math_iw4::vec3_distance(origin, *node);
        if dist - prev_dist > MAX_BARRIER_SEARCH_DIST {
            break;
        }
        prev_dist = dist;
        entrance_nodes.push(*node);
    }
    let node = if entrance_nodes.len() > 1 {
        entrance_nodes[level.random_int(entrance_nodes.len() as i32) as usize]
    } else {
        first
    };
    Some(Entrance::Picked {
        node,
        entrance_nodes,
    })
}

/// `maps\_zombiemode_utility.gsc::get_closest_valid_player`: the nearest
/// living player (the ignore list is not ported).
pub fn get_closest_valid_player(level: &Level, origin: [f32; 3]) -> Option<i32> {
    level
        .players
        .iter()
        .filter(|player| player.alive)
        .min_by(|a, b| {
            math_iw4::vec3_distance(origin, a.origin)
                .total_cmp(&math_iw4::vec3_distance(origin, b.origin))
        })
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
            Some(Entrance::Forced(node)) => cx.thread(owner, ZombieGotoEntrance::new(node)),
            Some(Entrance::Picked {
                node,
                entrance_nodes,
            }) => {
                cx.thread(owner, ZombieAssureNode::new(entrance_nodes));
                cx.thread(owner, ZombieGotoEntrance::new(node));
            }
            // `should_skip_teardown`, or a map with no windows (retail
            // asserts): hunt at once.
            Some(Entrance::SkipTeardown) | None => {
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

/// `maps\_zombiemode_spawner.gsc::zombie_assure_node`: while the zombie has
/// not reached its window, each `bad_path` sends it to the next of its
/// entrance nodes, then to the 20 windows closest to where it stands.
#[derive(Clone, Debug)]
pub struct ZombieAssureNode {
    entrance_nodes: Vec<[f32; 3]>,
    next: usize,
    widened: bool,
    bad_path: ZombieBadPath,
    pc: u8,
}

impl ZombieAssureNode {
    pub fn new(entrance_nodes: Vec<[f32; 3]>) -> Self {
        Self {
            entrance_nodes,
            next: 0,
            widened: false,
            bad_path: ZombieBadPath::default(),
            pc: 0,
        }
    }
}

impl Thread<Level> for ZombieAssureNode {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        let owner = cx.owner();
        let actor = actor_number(owner);
        loop {
            match self.pc {
                0 => {
                    cx.endon(owner, DEATH);
                    cx.endon(owner, GOAL);
                    cx.endon(Owner::LEVEL, INTERMISSION);
                    self.pc = 1;
                }
                1 => {
                    let Some(&node) = self.entrance_nodes.get(self.next) else {
                        if self.widened {
                            self.pc = 3;
                            return Yield::wait_seconds(ASSURE_NODE_GIVE_UP);
                        }
                        self.pc = 2;
                        return Yield::wait_seconds(ASSURE_NODE_RETRY_WAIT);
                    };
                    if let Some(wait) = call(&mut self.bad_path, cx) {
                        return wait;
                    }
                    self.bad_path = ZombieBadPath::default();
                    let bad = cx
                        .world
                        .zombies
                        .get(&actor)
                        .and_then(|zombie| zombie.zombie_bad_path);
                    if bad != Some(true) {
                        return Yield::Done;
                    }
                    // `self SetGoalPos( node )`: `goalradius` is still the
                    // one `zombie_goto_entrance` set.
                    set_goal_pos(cx.world, actor, node, ENTRANCE_GOAL_RADIUS);
                    self.next += 1;
                }
                2 => {
                    let level = &*cx.world;
                    let Some(origin) = level.zombies.get(&actor).map(|z| z.motor.origin) else {
                        return Yield::Done;
                    };
                    self.entrance_nodes =
                        get_array_of_closest(origin, &exterior_goals(level), ASSURE_NODE_CLOSEST);
                    self.next = 0;
                    self.widened = true;
                    self.pc = 1;
                }
                _ => {
                    // `self DoDamage( self.health + 10, self.origin )` and
                    // `level.zombies_timeout_spawn++` wait for zombie death
                    // (ZMB-041): the zombie stays where it is.
                    return Yield::Done;
                }
            }
        }
    }
}

/// `maps\_zombiemode_spawner.gsc::zombie_bad_path`, called in the caller's
/// thread: waits for a `bad_path` or its timeout and leaves the answer in
/// `self.zombie_bad_path`.
#[derive(Clone, Debug, Default)]
pub struct ZombieBadPath {
    started: bool,
}

impl Thread<Level> for ZombieBadPath {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        let owner = cx.owner();
        let actor = actor_number(owner);
        if !std::mem::replace(&mut self.started, true) {
            cx.endon(owner, DEATH);
            cx.endon(owner, GOAL);
            cx.thread(owner, ZombieBadPathNotify::default());
            cx.thread(owner, ZombieBadPathTimeout::default());
            if let Some(zombie) = cx.world.zombies.get_mut(&actor) {
                zombie.zombie_bad_path = None;
            }
        }
        let answered = cx
            .world
            .zombies
            .get(&actor)
            .is_none_or(|zombie| zombie.zombie_bad_path.is_some());
        if !answered {
            return Yield::wait_seconds(ZOMBIE_BAD_PATH_POLL);
        }
        cx.notify(owner, STOP_ZOMBIE_BAD_PATH, Vec::new());
        Yield::Done
    }
}

/// `zombie_bad_path_notify`: a `bad_path` answers yes.
#[derive(Clone, Debug, Default)]
struct ZombieBadPathNotify {
    started: bool,
}

impl Thread<Level> for ZombieBadPathNotify {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        let owner = cx.owner();
        if !std::mem::replace(&mut self.started, true) {
            cx.endon(owner, DEATH);
            cx.endon(owner, STOP_ZOMBIE_BAD_PATH);
            return Yield::waittill(owner, BAD_PATH);
        }
        if let Some(zombie) = cx.world.zombies.get_mut(&actor_number(owner)) {
            zombie.zombie_bad_path = Some(true);
        }
        Yield::Done
    }
}

/// `zombie_bad_path_timeout`: two seconds with no `bad_path` answer no.
#[derive(Clone, Debug, Default)]
struct ZombieBadPathTimeout {
    started: bool,
}

impl Thread<Level> for ZombieBadPathTimeout {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        let owner = cx.owner();
        if !std::mem::replace(&mut self.started, true) {
            cx.endon(owner, DEATH);
            cx.endon(owner, STOP_ZOMBIE_BAD_PATH);
            return Yield::wait_seconds(ZOMBIE_BAD_PATH_TIMEOUT);
        }
        if let Some(zombie) = cx.world.zombies.get_mut(&actor_number(owner)) {
            zombie.zombie_bad_path = Some(false);
        }
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
                        return Yield::wait_seconds(FIND_FLESH_NO_PLAYER_WAIT);
                    };
                    if let Some(zombie) = level.zombies.get_mut(&actor) {
                        zombie.favorite_enemy = Some(player);
                    }
                    // `self thread zombie_pathing()`, which with an enemy and
                    // no point of interest is `zombie_follow_enemy()`.
                    let delay = level.random_float_range(FIND_FLESH_REPICK.0, FIND_FLESH_REPICK.1);
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
            let dist = math_iw4::vec3_distance(origin, enemy);
            if let Some(tier) = FOLLOW_ENEMY_TIERS.iter().find(|tier| dist > tier.beyond) {
                extra_wait = tier.base + level.random_float_range(0.0, tier.spread);
            }
        }
        Yield::wait_seconds(extra_wait + FOLLOW_ENEMY_TICK)
    }
}
