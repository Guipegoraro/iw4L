//! `maps\_zombiemode_zone_manager.gsc`: which parts of the map spawn zombies.
//! A zone is a set of `info_volume`s named after it that target its
//! spawners; a zone spawns while a player stands in it or in an enabled,
//! connected neighbour.

use std::collections::BTreeMap;

use gsc_threads::{Cx, Thread, Yield};

use crate::Level;
use crate::mapents::{TARGET, TARGETNAME};

/// `spawner.classname == "actor_zombie_dog"`: a dog spawner, kept apart.
pub const ACTOR_ZOMBIE_DOG: &str = "actor_zombie_dog";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Zone {
    pub is_enabled: bool,
    pub is_occupied: bool,
    pub is_active: bool,
    /// Entity indices of the zone's `info_volume`s.
    pub volumes: Vec<u32>,
    /// Entity indices of the zone's zombie spawners.
    pub spawners: Vec<u32>,
    pub dog_spawners: Vec<u32>,
    pub adjacent_zones: BTreeMap<String, AdjacentZone>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AdjacentZone {
    pub is_connected: bool,
    pub flags: Vec<String>,
}

/// `maps\_zombiemode_zone_manager.gsc::zone_init`.
pub fn zone_init(level: &mut Level, zone_name: &str) {
    if level.zones.contains_key(zone_name) {
        return;
    }
    let mut zone = Zone {
        volumes: level
            .ents
            .array(zone_name, TARGETNAME)
            .into_iter()
            .filter(|&i| {
                level
                    .ents
                    .get(i)
                    .is_some_and(|e| e.classname() == "info_volume")
            })
            .collect(),
        ..Zone::default()
    };
    if zone.volumes.is_empty() {
        level.println(format!(
            "ASSERTMSG: zone_init: No volumes found for zone: {zone_name}"
        ));
    }
    let target = zone
        .volumes
        .first()
        .and_then(|&i| level.ents.get(i))
        .and_then(|volume| volume.get(TARGET))
        .map(str::to_owned);
    if let Some(target) = target {
        for spawner in level.ents.array(&target, TARGETNAME) {
            // `spawner.zone_name` and `is_enabled` are kept by the level's
            // spawner table; `level.ignore_spawner_func` is unset on Nacht.
            if level.ents.get(spawner).map(|e| e.classname()) == Some(ACTOR_ZOMBIE_DOG) {
                zone.dog_spawners.push(spawner);
            } else {
                zone.spawners.push(spawner);
            }
        }
    }
    level.zones.insert(zone_name.to_owned(), zone);
}

/// `maps\_zombiemode_zone_manager.gsc::enable_zone`, spawner half: respawn
/// points and barrier goals are ZMB-037.
pub fn enable_zone(level: &mut Level, zone_name: &str) {
    let Some(zone) = level.zones.get_mut(zone_name) else {
        level.println(format!(
            "ASSERTMSG: enable_zone: zone has not been initialized ({zone_name})"
        ));
        return;
    };
    zone.is_enabled = true;
}

/// `maps\_zombiemode_zone_manager.gsc::make_zone_adjacent`.
fn make_zone_adjacent(level: &mut Level, main_zone: &str, adj_zone: &str, flag_name: &str) {
    let Some(zone) = level.zones.get_mut(main_zone) else {
        return;
    };
    let adjacent = zone.adjacent_zones.entry(adj_zone.to_owned()).or_default();
    if !adjacent.flags.iter().any(|flag| flag == flag_name) {
        adjacent.flags.push(flag_name.to_owned());
    }
}

/// `maps\_zombiemode_zone_manager.gsc::add_adjacent_zone`; the zone flags are
/// initialised by the caller's scheduler (`flag_init`).
pub fn add_adjacent_zone(
    level: &mut Level,
    zone_a: &str,
    zone_b: &str,
    flag_name: &str,
    one_way: bool,
) {
    zone_init(level, zone_a);
    zone_init(level, zone_b);
    make_zone_adjacent(level, zone_a, zone_b, flag_name);
    if !one_way {
        make_zone_adjacent(level, zone_b, zone_a, flag_name);
    }
}

/// `maps\_zombiemode_zone_manager.gsc::player_in_zone`. Touching a zone's
/// volumes needs the volumes' brushes, which the zone data does not reach
/// yet (ZMB-036), so no zone counts as occupied and `manage_zones` falls back
/// to its first initial zone, as retail does when no zone is occupied.
pub fn player_in_zone(_level: &Level, _zone_name: &str) -> bool {
    false
}

/// `maps\_zombiemode_zone_manager.gsc::create_spawner_list`, zombie spawners
/// (dog and riser locations come with those AI).
pub fn create_spawner_list(level: &mut Level) {
    level.enemy_spawns = level
        .zones
        .values()
        .filter(|zone| zone.is_enabled && zone.is_active)
        .flat_map(|zone| zone.spawners.iter().copied())
        .collect();
}

/// `maps\_zombiemode_zone_manager.gsc::manage_zones`.
#[derive(Clone, Debug)]
pub struct ManageZones {
    pub initial_zones: Vec<String>,
    /// `level.zone_manager_init_func`.
    pub init_func: fn(&mut Cx<'_, Level>),
    pc: u8,
}

impl ManageZones {
    pub fn new(initial_zones: Vec<String>, init_func: fn(&mut Cx<'_, Level>)) -> Self {
        Self {
            initial_zones,
            init_func,
            pc: 0,
        }
    }
}

impl Thread<Level> for ManageZones {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        match self.pc {
            0 => {
                // Setup zone connections, then the initial zones.
                (self.init_func)(cx);
                for zone in self.initial_zones.clone() {
                    zone_init(cx.world, &zone);
                    enable_zone(cx.world, &zone);
                }
                cx.flag_set("zones_initialized");
                self.pc = 1;
                Yield::FlagWait("begin_spawning".into())
            }
            1 => {
                let level = &mut *cx.world;
                let names: Vec<String> = level.zones.keys().cloned().collect();
                for zone in level.zones.values_mut() {
                    zone.is_active = false;
                    zone.is_occupied = false;
                }
                let mut a_zone_is_active = false;
                for name in &names {
                    if !level.zones[name].is_enabled {
                        continue;
                    }
                    let occupied = player_in_zone(level, name);
                    let zone = level.zones.get_mut(name).expect("zone listed above");
                    zone.is_occupied = occupied;
                    if !occupied {
                        continue;
                    }
                    zone.is_active = true;
                    a_zone_is_active = true;
                    let open: Vec<String> = zone
                        .adjacent_zones
                        .iter()
                        .filter(|(_, adjacent)| adjacent.is_connected)
                        .map(|(name, _)| name.clone())
                        .collect();
                    for name in open {
                        if let Some(adjacent) = level.zones.get_mut(&name)
                            && adjacent.is_enabled
                        {
                            adjacent.is_active = true;
                        }
                    }
                }
                if !a_zone_is_active
                    && let Some(first) = self.initial_zones.first()
                    && let Some(zone) = level.zones.get_mut(first)
                {
                    zone.is_active = true;
                    zone.is_occupied = true;
                }
                create_spawner_list(level);
                Yield::wait_seconds(1.0)
            }
            _ => unreachable!("manage_zones has two steps"),
        }
    }
}
