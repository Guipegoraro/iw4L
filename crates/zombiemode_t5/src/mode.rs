//! The zombies mode as the simulation runs it: one [`Scheduler`] of ported
//! threads over the [`Level`], stepped once per server frame.

use gsc_threads::{Owner, Scheduler, Value};
use sim::{ModeEngine, ModeScript};

use crate::actor::{self, MotorEvent, actor_owner};
use crate::level::EngineCommand;
use crate::zombie_melee::{MELEEANIM, NOTE_END};
use crate::zombiemode::{ALL_PLAYERS_CONNECTED, Main};
use crate::{Level, MapEnts, StringTables};

#[derive(Clone, Debug)]
pub struct ZombiesMode {
    threads: Scheduler<Level>,
    level: Level,
    /// `GetTime()` of the previous frame, for the actors' frame time.
    last_ms: Option<u64>,
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
            last_ms: None,
        }
    }

    pub fn level(&self) -> &Level {
        &self.level
    }

    /// The engine's actor pass: each zombie's motor moves it by root motion,
    /// its pose goes to the engine, and what it reports wakes its threads next
    /// frame.
    fn step_actors(&mut self, engine: &mut ModeEngine<'_, '_>, dt: f32) {
        for (&actor, zombie) in &mut self.level.zombies {
            let events = zombie.motor.step(
                dt,
                |clip| engine.actor_clip(clip),
                |point| engine.ground_z(point),
            );
            if let Some((clip, looping)) = zombie.motor.take_restart() {
                engine.actor_play_anim(actor, clip, looping, actor::ANIM_RATE);
            }
            engine.set_actor_origin(actor, zombie.motor.origin, zombie.motor.yaw);
            let owner = actor_owner(actor);
            for event in events {
                let (name, args) = match event {
                    MotorEvent::Goal => (actor::GOAL, Vec::new()),
                    MotorEvent::BadPath => (actor::BAD_PATH, Vec::new()),
                    MotorEvent::Note(note) => (MELEEANIM, vec![Value::Str(note.into())]),
                    MotorEvent::End => (MELEEANIM, vec![Value::Str(NOTE_END.into())]),
                };
                self.threads.notify(owner, name, args);
            }
        }
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
                            engine.actor_play_anim(actor, anim, true, actor::ANIM_RATE);
                            // `spawner add_spawn_function( zombie_spawn_init )`
                            // ends in `self thread zombie_think()`.
                            self.threads
                                .spawn(actor_owner(actor), crate::spawner::ZombieThink);
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
                EngineCommand::MeleePlayer {
                    client,
                    amount,
                    from,
                } => {
                    engine.melee_player(client, amount, from);
                }
            }
        }
        let dt = self.last_ms.map_or(0.0, |last| {
            self.level.now_ms.saturating_sub(last) as f32 / 1000.0
        });
        self.last_ms = Some(self.level.now_ms);
        self.step_actors(engine, dt);
        for line in self.level.println.drain(..) {
            diag::info!(Sim, "zombies: {line}");
        }
    }
}
