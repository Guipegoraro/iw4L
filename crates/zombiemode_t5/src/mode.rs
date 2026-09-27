//! The zombies mode as the simulation runs it: one [`Scheduler`] of ported
//! threads over the [`Level`], stepped once per server frame.

use gsc_threads::{Owner, Scheduler};
use sim::{ModeEngine, ModeScript};

use crate::level::EngineCommand;
use crate::zombiemode::{ALL_PLAYERS_CONNECTED, Main};
use crate::{Level, MapEnts, StringTables};

#[derive(Clone, Debug)]
pub struct ZombiesMode {
    threads: Scheduler<Level>,
    level: Level,
}

impl ZombiesMode {
    /// A zombies match on `map`: `_zombiemode::main()` starts on the first
    /// frame.
    pub fn new(
        map: &str,
        tables: StringTables,
        path_graph: std::sync::Arc<pathnodes::PathGraph>,
        map_entities: &str,
    ) -> Self {
        let mut threads = Scheduler::default();
        threads.flag_init(ALL_PLAYERS_CONNECTED);
        // The map's own `main()` runs `_zombiemode::main()` and its zones.
        if map == crate::prototype::MAP {
            threads.spawn(Owner::LEVEL, crate::prototype::Main::default());
        } else {
            threads.spawn(Owner::LEVEL, Main);
        }
        let ents = MapEnts::parse(map_entities);
        diag::info!(
            Sim,
            "zombies: {} map entities, {} path nodes, {} string tables",
            ents.ents.len(),
            path_graph.len(),
            tables.len()
        );
        Self {
            threads,
            level: Level::new(map, tables, path_graph, ents),
        }
    }

    pub fn level(&self) -> &Level {
        &self.level
    }
}

impl ModeScript for ZombiesMode {
    fn clone_box(&self) -> Box<dyn ModeScript> {
        Box::new(self.clone())
    }

    fn frame(&mut self, engine: &mut ModeEngine<'_, '_>) {
        self.level.now_ms = engine.now_ms();
        self.level.players = engine.players();
        // Retail's players are all in once the connected count reaches the
        // expected one; here that is everyone who joined having spawned.
        if !self.threads.flag(ALL_PLAYERS_CONNECTED)
            && !self.level.players.is_empty()
            && self.level.players.iter().all(|player| player.alive)
        {
            self.threads.flag_set(ALL_PLAYERS_CONNECTED);
        }
        let report = self.threads.run(self.level.now_ms, &mut self.level);
        if report.runaway {
            diag::warn!(
                Sim,
                "zombies: a script thread ran {} times in one frame without waiting; stopped",
                report.resumed
            );
        }
        for command in self.level.commands.drain(..) {
            match command {
                EngineCommand::SetWeaponAmmoClip {
                    entnum,
                    weapon,
                    clip,
                } => engine.set_weapon_ammo_clip(sim::ClientId(entnum as u32), weapon, clip),
                EngineCommand::SpawnActor {
                    actor,
                    body,
                    head,
                    origin,
                    yaw,
                    anim,
                } => {
                    let head = sim::actors::ActorAttachment {
                        model: head.to_owned(),
                        tag: None,
                    };
                    match engine.spawn_actor(actor, body, &[head], origin, yaw) {
                        Ok(_) => {
                            engine.actor_play_anim(actor, anim, true, 1.0);
                        }
                        Err(error) => {
                            // Retail's `spawn_failed`: the zombie never was.
                            self.level.zombies.remove(&actor);
                            self.level.zombie_total += 1;
                            diag::warn!(
                                Sim,
                                "zombies: spawning actor {actor} ({body}) failed: {error:?}"
                            );
                        }
                    }
                }
            }
        }
        for line in self.level.println.drain(..) {
            diag::info!(Sim, "zombies: {line}");
        }
    }
}
