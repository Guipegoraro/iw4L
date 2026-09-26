//! Noclip: an IW4L movement mode. Fly along the view through everything.
//! Jump rises, crouch sinks, sprint doubles the speed.

use playerstate_iw4::{ENTITYNUM_NONE, PlayerState, UserCmd, buttons};

use crate::Pml;

pub const NOCLIP_SPEED: f32 = 400.0;

pub fn pm_noclip_move(ps: &mut PlayerState, pml: &Pml, cmd: &UserCmd) {
    let forward = f32::from(cmd.forwardmove) / 127.0;
    let right = f32::from(cmd.rightmove) / 127.0;
    let mut up = 0.0;
    if cmd.buttons & buttons::JUMP != 0 {
        up += 1.0;
    }
    if cmd.buttons & (buttons::CROUCH | buttons::PRONE) != 0 {
        up -= 1.0;
    }
    let speed = if cmd.buttons & buttons::SPRINT != 0 {
        NOCLIP_SPEED * 2.0
    } else {
        NOCLIP_SPEED
    };
    for i in 0..3 {
        ps.velocity[i] = (pml.forward[i] * forward + pml.right[i] * right) * speed;
    }
    ps.velocity[2] += up * speed;
    for i in 0..3 {
        ps.origin[i] += ps.velocity[i] * pml.frametime;
    }
    ps.ground_entity_num = ENTITYNUM_NONE;
}
