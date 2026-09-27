//! The characters zombies wear, from the zones' `aitype\*.gsc` and
//! `character\*.gsc` (both generated from the aitype and character assets).
//! A spawner's classname names its aitype (`actor_<aitype>`); the aitype's
//! `main()` runs its character's `main()`, which picks a body and a head.

use crate::Level;

/// `character\<name>.gsc`: the models `main()` chooses between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Character {
    /// `xmodelalias\<name>_bodyalias.gsc`, for `setModelFromArray`.
    pub bodies: &'static [&'static str],
    /// `xmodelalias\<name>_headalias.gsc`, for `randomElement`.
    pub heads: &'static [&'static str],
}

/// `character\c_ger_honorguard_zt.gsc` (zombie_cod5_prototype.ff).
pub const C_GER_HONORGUARD_ZT: Character = Character {
    bodies: &[
        "c_ger_honorguard_body1",
        "c_ger_honorguard_body2",
        "c_ger_honorguard_body2",
        "c_ger_honorguard_body2",
    ],
    heads: &[
        "c_ger_zombie_head1",
        "c_ger_zombie_head2",
        "c_ger_zombie_head3",
        "c_ger_zombie_head4",
    ],
};

/// The aitype a spawner's classname names, and what its `main()` sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiType {
    pub classname: &'static str,
    /// `self.health`.
    pub health: i32,
    pub character: Character,
}

/// `aitype\zombie_ger_zombie.gsc`.
pub const ZOMBIE_GER_ZOMBIE: AiType = AiType {
    classname: "actor_zombie_ger_zombie",
    health: 150,
    character: C_GER_HONORGUARD_ZT,
};

pub const AITYPES: &[AiType] = &[ZOMBIE_GER_ZOMBIE];

pub fn aitype_for_classname(classname: &str) -> Option<&'static AiType> {
    AITYPES.iter().find(|aitype| aitype.classname == classname)
}

/// Every model an aitype can put on an actor, for the engine to prepare.
pub fn actor_models() -> Vec<&'static str> {
    let mut models: Vec<&'static str> = AITYPES
        .iter()
        .flat_map(|aitype| {
            aitype
                .character
                .bodies
                .iter()
                .chain(aitype.character.heads)
                .copied()
        })
        .collect();
    models.sort_unstable();
    models.dedup();
    models
}

/// The body and head a character's `main()` picks:
/// `codescripts\character::setModelFromArray` and `randomElement` are both
/// `a[ RandomInt( a.size ) ]`.
pub fn pick_models(level: &mut Level, character: &Character) -> (&'static str, &'static str) {
    let body = character.bodies[level.random_int(character.bodies.len() as i32) as usize];
    let head = character.heads[level.random_int(character.heads.len() as i32) as usize];
    (body, head)
}
