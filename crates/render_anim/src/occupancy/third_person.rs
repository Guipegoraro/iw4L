use assets::ClipCollision;
use net::PresentedSnapshot;
use playerstate_iw4::{
    CG_CAMERA_PULLBACK_BOX_HALF, CG_CAMERA_PULLBACK_CLIPMASK, CG_THIRD_PERSON_RANGE_DEFAULT,
    CgIsThirdPersonViewInputs, KillCamMode, OffsetThirdPersonViewInputs, cg_is_third_person_view,
    offset_third_person_view,
};
use sim::ClientId;

use render_scene::WorldCameraPose;

pub const CG_THIRD_PERSON_ANGLE_MP: f32 = 356.0;

pub fn presented_is_third_person(
    presented: &PresentedSnapshot,
    local: ClientId,
    in_killcam: bool,
) -> bool {
    let Some(ps) = presented.player(local) else {
        return false;
    };
    if in_killcam && ps.kill_cam_entity != playerstate_iw4::ENTITYNUM_NONE {
        return true;
    }
    if remote_missile_camera(presented, local, 0).is_some() {
        return true;
    }
    cg_is_third_person_view(CgIsThirdPersonViewInputs {
        pm_type: ps.pm_type,
        other_flags: ps.other_flags,
        link_flags: ps.link_flags,
        cg_third_person: presented.cg_third_person(),
        in_killcam,
        killcam_mode: KillCamMode::Mode0,
    })
}

pub fn remote_missile_camera(
    presented: &PresentedSnapshot,
    local: ClientId,
    at_time: i32,
) -> Option<WorldCameraPose> {
    let link = presented
        .snapshot()?
        .meta
        .for_client(local)?
        .remote_missile
        .filter(|link| link.unlink_at_ms.is_none())?;
    let missile = presented
        .presented_projectiles()
        .iter()
        .find(|p| p.authoritative_id() == Some(link.projectile))?;
    Some(WorldCameraPose {
        origin: missile.origin_at(at_time),
        angles: link.angles,
    })
}

pub fn death_watch_camera(
    presented: &PresentedSnapshot,
    local: ClientId,
    clip: Option<&ClipCollision>,
) -> Option<WorldCameraPose> {
    let ps = presented.player(local)?;
    let clip = clip?;
    let offset = presented.view_offset();
    let yaw = presented
        .snapshot()?
        .meta
        .for_client(local)
        .map(|meta| meta.look_at_killer_yaw as f32)
        .unwrap_or(ps.viewangles[1]);

    let half = CG_CAMERA_PULLBACK_BOX_HALF;
    let trace = |start: [f32; 3], end: [f32; 3]| {
        clip.sweep_box(
            start,
            end,
            [-half, -half, -half],
            [half, half, half],
            CG_CAMERA_PULLBACK_CLIPMASK,
        )
        .fraction
    };
    let view = offset_third_person_view(
        OffsetThirdPersonViewInputs {
            origin: [
                ps.origin[0] + offset[0],
                ps.origin[1] + offset[1],
                ps.origin[2] + offset[2],
            ],
            view_height_current: ps.view_height_current,
            viewangles: ps.viewangles,
            pm_type: ps.pm_type,
            look_at_killer_yaw: yaw,

            corpse_j_mainroot: None,
            other_flags: ps.other_flags,
            delta_time: ps.delta_time,
            cg_third_person_angle: CG_THIRD_PERSON_ANGLE_MP,
            cg_third_person_range: CG_THIRD_PERSON_RANGE_DEFAULT,
        },
        trace,
    );
    Some(WorldCameraPose {
        origin: view.origin,
        angles: view.angles,
    })
}

/// Skate 3's ground chase shot, in IW units (1 m ≈ 39.37): about 2 m behind
/// the board, low, aimed a little ahead of it.
const CHASE_DISTANCE: f32 = 80.0;
const CHASE_HEIGHT: f32 = 22.0;
const CHASE_FOCUS_Z: f32 = 30.0;
const CHASE_AIM_AHEAD: f32 = 60.0;
/// How fast the shot swings round behind the board (fraction of the gap per second).
const CHASE_YAW_RATE: f32 = 5.0;

/// The chase camera's smoothed heading, carried between frames.
#[derive(Default)]
pub struct SkateChase {
    yaw: Option<f32>,
}

/// The skate chase camera while the local player skates, or `None`.
pub fn skate_chase_camera(
    presented: &PresentedSnapshot,
    local: ClientId,
    clip: Option<&ClipCollision>,
    chase: &mut SkateChase,
    dt: f32,
) -> Option<WorldCameraPose> {
    let ps = presented.player(local)?;
    if movement_iw4::MoveMode::of(ps) != movement_iw4::MoveMode::Skate {
        chase.yaw = None;
        return None;
    }
    let target = ps.skate_yaw;
    let yaw = match chase.yaw {
        Some(yaw) => {
            let gap = (target - yaw + 540.0).rem_euclid(360.0) - 180.0;
            yaw + gap * (CHASE_YAW_RATE * dt).min(1.0)
        }
        None => target,
    };
    chase.yaw = Some(yaw);

    let offset = presented.view_offset();
    let focus = [
        ps.origin[0] + offset[0],
        ps.origin[1] + offset[1],
        ps.origin[2] + offset[2] + CHASE_FOCUS_Z,
    ];
    let (sin, cos) = yaw.to_radians().sin_cos();
    let wanted = [
        focus[0] - cos * CHASE_DISTANCE,
        focus[1] - sin * CHASE_DISTANCE,
        focus[2] + CHASE_HEIGHT,
    ];
    let origin = match clip {
        Some(clip) => {
            let half = CG_CAMERA_PULLBACK_BOX_HALF;
            let hit = clip.sweep_box(
                focus,
                wanted,
                [-half, -half, -half],
                [half, half, half],
                CG_CAMERA_PULLBACK_CLIPMASK,
            );
            let f = hit.fraction;
            [
                focus[0] + (wanted[0] - focus[0]) * f,
                focus[1] + (wanted[1] - focus[1]) * f,
                focus[2] + (wanted[2] - focus[2]) * f,
            ]
        }
        None => wanted,
    };
    let aim = [
        focus[0] + cos * CHASE_AIM_AHEAD,
        focus[1] + sin * CHASE_AIM_AHEAD,
        focus[2],
    ];
    let flat = (aim[0] - origin[0]).hypot(aim[1] - origin[1]).max(1.0);
    // IW pitch is positive looking down.
    let pitch = (origin[2] - aim[2]).atan2(flat).to_degrees();
    let aim_yaw = (aim[1] - origin[1]).atan2(aim[0] - origin[0]).to_degrees();
    Some(WorldCameraPose {
        origin,
        angles: [pitch, aim_yaw, 0.0],
    })
}
