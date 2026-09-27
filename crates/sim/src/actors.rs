//! Script-driven actors: a Black Ops zombie is the first. An actor is a
//! script mover (its entity number, origin and angles ride the snapshot and
//! are interpolated on clients like any mover) plus an animated DObj: a body
//! with attached models (a head) playing one clip, which also gives bullets
//! its bones to hit.
//!
//! The mode script decides everything an actor does; this module only holds
//! the engine half: spawn, play an animation, move, delete.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::bullet_collision::{
    AuthorityDObjState, AuthorityModelOwner, EntityCollisionCapabilities,
};
use crate::frame::FrameWorld;
use crate::identities::ScriptModelId;

/// The model source kind of an actor's mover (see `killstreak_model_source`).
pub const ACTOR_MODEL_KIND: u32 = 5;

/// The xmodels actors may wear, installed with the map: name → the retained
/// capability the server poses for bone hits (`None` when the model has no
/// capability; it is still drawn).
#[derive(Clone, Debug, Default)]
pub struct ActorModels(BTreeMap<String, Option<Arc<xmodel_runtime::RetainedModelCapability>>>);

impl ActorModels {
    pub fn new(
        models: impl IntoIterator<Item = (String, Option<Arc<xmodel_runtime::RetainedModelCapability>>)>,
    ) -> Self {
        Self(models.into_iter().collect())
    }

    pub fn contains(&self, model: &str) -> bool {
        self.0.contains_key(model)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn capability(&self, model: &str) -> Option<Arc<xmodel_runtime::RetainedModelCapability>> {
        self.0.get(model).cloned().flatten()
    }
}

/// The animations actors play, installed with the map: name → the decoded
/// clip, whose length, root motion and notes the mode script reads.
#[derive(Clone, Debug, Default)]
pub struct ActorClips(BTreeMap<String, Arc<xmodel_runtime::AnimClip>>);

impl ActorClips {
    pub fn new(clips: impl IntoIterator<Item = (String, Arc<xmodel_runtime::AnimClip>)>) -> Self {
        Self(clips.into_iter().collect())
    }

    pub fn get(&self, name: &str) -> Option<&Arc<xmodel_runtime::AnimClip>> {
        self.0.get(name)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The leaf frequency (cycles per second) that plays `clip` at `rate` times
/// its authored speed, as `SetAnim`'s rate does. A clip with no length stays
/// still.
pub fn clip_frequency(clip: &xmodel_runtime::AnimClip, rate: f32) -> f32 {
    rate * clip.frequency()
}

/// A model attached to an actor's body, like `self Attach(head)` in GSC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorAttachment {
    pub model: String,
    /// The body bone it hangs from; `None` binds its root bone to the body's
    /// bone of the same name, as `Attach` does with an empty tag.
    pub tag: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorSpawnError {
    UnknownModel,
    NoEntity,
    AlreadySpawned,
    /// An attachment with no tag whose root bone the body lacks.
    NoAttachBone,
}

/// `Attach( model, "" )`: the attached model's root bone binds to the
/// body's bone of the same name (a head's `j_spine4` or `j_neck`).
fn root_bone_tag(models: &ActorModels, body: &str, attached: &str) -> Option<String> {
    let root = models
        .capability(attached)?
        .pose
        .bone_names
        .first()?
        .clone();
    models
        .capability(body)?
        .pose
        .bone_names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(&root))
        .then_some(root)
}

pub const fn actor_model_id(actor: u32) -> ScriptModelId {
    ScriptModelId::from_wire(crate::killstreaks::model_source(ACTOR_MODEL_KIND, actor))
}

/// Whether a mover source ordinal is an actor's, and which.
pub const fn actor_of_model_source(source: u32) -> Option<u32> {
    if source & 0x8000_0000 != 0 && (source >> 28) & 0x7 == ACTOR_MODEL_KIND {
        Some(source & 0x0fff_ffff)
    } else {
        None
    }
}

fn world_from_model(origin: [f32; 3], yaw: f32) -> glam::Mat4 {
    glam::Mat4::from_rotation_translation(
        glam::Quat::from_rotation_z(yaw.to_radians()),
        glam::Vec3::from_array(origin),
    )
}

fn capabilities_mut<'a>(
    world: &'a mut FrameWorld<'_>,
    id: ScriptModelId,
) -> Option<&'a mut EntityCollisionCapabilities> {
    world
        .entity_collision_capabilities_mut()
        .iter_mut()
        .find(|capabilities| capabilities.owner == AuthorityModelOwner::ScriptModel(id))
}

pub(crate) fn spawn(
    world: &mut FrameWorld<'_>,
    actor: u32,
    body: &str,
    attachments: &[ActorAttachment],
    origin: [f32; 3],
    yaw: f32,
) -> Result<i32, ActorSpawnError> {
    let id = actor_model_id(actor);
    if world.gentity_number(id).is_some() {
        return Err(ActorSpawnError::AlreadySpawned);
    }
    if !world.actor_models().contains(body)
        || attachments
            .iter()
            .any(|attachment| !world.actor_models().contains(&attachment.model))
    {
        return Err(ActorSpawnError::UnknownModel);
    }
    let number = world
        .spawn_script_mover(id, origin, [0.0, yaw, 0.0])
        .map_err(|_| ActorSpawnError::NoEntity)?;
    let capability = world.actor_models().capability(body);
    let mut dobj =
        AuthorityDObjState::new_dirty(body.to_owned(), capability, world_from_model(origin, yaw));
    for attachment in attachments {
        let Some(tag) = attachment
            .tag
            .clone()
            .or_else(|| root_bone_tag(world.actor_models(), body, &attachment.model))
        else {
            world.remove_script_mover_by_number(number);
            return Err(ActorSpawnError::NoAttachBone);
        };
        dobj.semantic_state
            .composition
            .models
            .push(xmodel_runtime::DObjModelDescriptor {
                model: attachment.model.clone(),
                parent_model: Some(0),
                attach_tag: Some(tag),
                ignore_collision: true,
            });
    }
    let row = EntityCollisionCapabilities::current_tick(
        AuthorityModelOwner::ScriptModel(id),
        Some(dobj),
        Vec::new(),
    );
    let rows = world.entity_collision_capabilities_vec_mut();
    let at = rows.partition_point(|have| have.owner < row.owner);
    rows.insert(at, row);
    Ok(number)
}

/// `AnimScripted` / `SetAnim` on one clip: restart it from the first frame.
pub(crate) fn play_anim(
    world: &mut FrameWorld<'_>,
    actor: u32,
    clip: &str,
    looping: bool,
    rate: f32,
) -> bool {
    let Some(frequency) = world
        .actor_clips()
        .get(clip)
        .map(|facts| clip_frequency(facts, rate))
    else {
        return false;
    };
    let Some(dobj) = capabilities_mut(world, actor_model_id(actor)).and_then(|c| c.dobj.as_mut())
    else {
        return false;
    };
    // The attached models survive a new clip: only the tree is replaced.
    let composition = dobj.semantic_state.composition.clone();
    dobj.begin_script_model_play_anim(clip, looping, frequency);
    dobj.semantic_state.composition = composition;
    true
}

pub(crate) fn set_origin(
    world: &mut FrameWorld<'_>,
    actor: u32,
    origin: [f32; 3],
    yaw: f32,
) -> bool {
    let id = actor_model_id(actor);
    let Some(number) = world.gentity_number(id) else {
        return false;
    };
    world.set_script_mover_pose(number, origin, [0.0, yaw, 0.0]);
    if let Some(dobj) = capabilities_mut(world, id).and_then(|c| c.dobj.as_mut()) {
        dobj.world_from_model = world_from_model(origin, yaw);
        dobj.current_collision = None;
        dobj.materialized_pose_revision = None;
    }
    true
}

pub(crate) fn delete(world: &mut FrameWorld<'_>, actor: u32) -> bool {
    let id = actor_model_id(actor);
    let Some(number) = world.gentity_number(id) else {
        return false;
    };
    world.remove_script_mover_by_number(number);
    world
        .entity_collision_capabilities_vec_mut()
        .retain(|capabilities| capabilities.owner != AuthorityModelOwner::ScriptModel(id));
    true
}
