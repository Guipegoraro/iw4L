//! `maps\_loadout.gsc`

/// What every player spawns with. Each slot lists weapon names in order of
/// preference: the zombies weapon from the zombies zones first, then the Black
/// Ops multiplayer weapon that stands in for it while those zones are not
/// loaded with the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartLoadout {
    pub primary: &'static [&'static str],

    pub lethal: &'static [&'static str],
}

/// `maps\_loadout.gsc::init_models_and_variables_loadout` (zombiemode branch: `knife_zm`,
/// `m1911_zm`) and `maps\_zombiemode_utility.gsc::register_offhand_weapons_for_level_defaults`
/// (`level.zombie_lethal_grenade_player_init = "frag_grenade_zm"`). The knife is
/// the player's melee, which every loadout already has.
pub const fn start_loadout() -> StartLoadout {
    StartLoadout {
        primary: &["t5:weapon/m1911_zm", "t5:weapon/m1911_mp"],
        lethal: &["t5:weapon/frag_grenade_zm", "t5:weapon/frag_grenade_mp"],
    }
}
