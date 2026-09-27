//! `maps\zombie_cod5_prototype.gsc`: Nacht der Untoten.

use gsc_threads::{Cx, Owner, Thread, Yield, call};

use crate::Level;
use crate::zone_manager::{ManageZones, add_adjacent_zone};

pub const MAP: &str = "zombie_cod5_prototype";

/// `maps\zombie_cod5_prototype.gsc::main`, the parts that run zombies:
/// `_zombiemode::main()`, then the zone manager on `start_zone`.
#[derive(Clone, Debug, Default)]
pub struct Main {
    zombiemode: crate::zombiemode::Main,
}

impl Thread<Level> for Main {
    fn resume(&mut self, cx: &mut Cx<'_, Level>) -> Yield {
        if let Some(wait) = call(&mut self.zombiemode, cx) {
            return wait;
        }
        cx.thread(
            Owner::LEVEL,
            ManageZones::new(vec!["start_zone".into()], prototype_zone_init),
        );
        Yield::Done
    }
}

/// `maps\zombie_cod5_prototype.gsc::prototype_zone_init`.
fn prototype_zone_init(cx: &mut Cx<'_, Level>) {
    cx.flag_set("always_on");
    add_adjacent_zone(cx.world, "start_zone", "box_zone", "start_2_box", false);
    add_adjacent_zone(
        cx.world,
        "start_zone",
        "upstairs_zone",
        "start_2_upstairs",
        false,
    );
    add_adjacent_zone(
        cx.world,
        "box_zone",
        "upstairs_zone",
        "box_2_upstairs",
        false,
    );
}
