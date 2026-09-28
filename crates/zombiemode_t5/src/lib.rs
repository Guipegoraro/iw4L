//! Black Ops zombies, ported from the retail GSC.
//!
//! One module per script file, named after it, so a line of GSC maps to one
//! place here and a custom map or mod written against the GSC finds its
//! counterpart. Each ported item names its origin as `file::function`.
//!
//! The ported functions run as [`gsc_threads`] threads over [`Level`], the
//! zombies' `level` and the frame's view of the engine. [`ZombiesMode`] is what
//! the simulation runs each server frame.

#![forbid(unsafe_code)]

pub mod actor;
pub mod anims;
pub mod character;
pub mod damage;
pub mod level;
pub mod loadout;
pub mod mapents;
pub mod mode;
pub mod powerups;
pub mod prototype;
pub mod score;
pub mod spawner;
pub mod table;
pub mod utility;
pub mod zombie_melee;
pub mod zombiemode;
pub mod zone_manager;

pub use anims::actor_clips;
pub use character::actor_models;
pub use level::Level;
pub use loadout::{StartLoadout, start_loadout};
pub use mapents::MapEnts;
pub use mode::ZombiesMode;
pub use table::{StringTable, StringTables};
