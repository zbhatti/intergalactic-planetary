use crate::camera::FlyCam;
use crate::coords::{WorldPos, view_transform};
use crate::search::Search;
use bevy::{prelude::*, window::PrimaryWindow};

// Only used by draw_offscreen_arrow for 3D placement of the arrow gizmo.
const ARROW_DEPTH: f32 = 1e-4;

const RETICLE_RADIUS: f32 = 30.0; // pixels
const RETICLE_BORDER: f32 = 2.0; // pixels

#[derive(Component)]
pub struct TargetReticle;

/// Returns (yaw_deg, pitch_deg) for a normalised direction vector using the
/// same YXZ Euler convention used by the camera.
pub fn dir_to_yaw_pitch(dir: Vec3) -> (f32, f32) {
    let pitch = dir.y.asin().to_degrees();
    let yaw = (-dir.x).atan2(-dir.z).to_degrees();
    (yaw, pitch)
}

pub fn orient_camera_to_target(
    mut search: ResMut<Search>,
    mut cam_query: Query<(&WorldPos, &mut Transform, &mut FlyCam)>,
    targets: Query<&WorldPos, Without<FlyCam>>,
) {
    if !search.should_orient {
        return;
    }
    search.should_orient = false;
    let Some(target) = search.target.and_then(|e| targets.get(e).ok()) else {
        return;
    };
    let Ok((cam_pos, mut transform, mut cam)) = cam_query.single_mut() else {
        return;
    };

    let offset = target.0 - cam_pos.0;
    if offset.length_squared() == 0.0 {
        return;
    }
    let (yaw, pitch) = dir_to_yaw_pitch(offset.normalize().as_vec3());
    cam.pitch = pitch.clamp(-89.0, 89.0);
    cam.yaw = yaw;
    transform.rotation = cam.rotation();
}

/// Target position relative to the camera (f32, render-independent), if there is a target.
fn target_offset(
    search: &Search,
    cam_pos: &WorldPos,
    targets: &Query<&WorldPos, Without<FlyCam>>,
) -> Option<Vec3> {
    let target = targets.get(search.target?).ok()?;
    Some((target.0 - cam_pos.0).as_vec3())
}

pub fn setup_target_reticle(mut commands: Commands) {
    let diameter = RETICLE_RADIUS * 2.0;
    commands.spawn((
        TargetReticle,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(diameter),
            height: Val::Px(diameter),
            border: UiRect::all(Val::Px(RETICLE_BORDER)),
            ..default()
        },
        BorderColor(Color::WHITE),
        BorderRadius::MAX,
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        Visibility::Hidden,
    ));
}

pub fn update_target_reticle(
    search: Res<Search>,
    cam_query: Query<(&Camera, &WorldPos, &Transform), With<FlyCam>>,
    targets: Query<&WorldPos, Without<FlyCam>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut reticle_query: Query<(&mut Node, &mut Visibility), With<TargetReticle>>,
) {
    let Ok((mut node, mut vis)) = reticle_query.single_mut() else {
        return;
    };
    let (Ok((camera, cam_pos, cam_t)), Ok(window)) = (cam_query.single(), windows.single()) else {
        return;
    };
    let vp = Vec2::new(window.width(), window.height());
    let screen = target_offset(&search, cam_pos, &targets).and_then(|rel| {
        camera
            .world_to_viewport(&view_transform(cam_t.rotation), rel)
            .ok()
            .filter(|sp| sp.x >= 0.0 && sp.x <= vp.x && sp.y >= 0.0 && sp.y <= vp.y)
    });
    let Some(sp) = screen else {
        vis.set_if_neq(Visibility::Hidden);
        return;
    };
    node.left = Val::Px(sp.x - RETICLE_RADIUS);
    node.top = Val::Px(sp.y - RETICLE_RADIUS);
    vis.set_if_neq(Visibility::Visible);
}

pub fn draw_offscreen_arrow(
    mut gizmos: Gizmos,
    search: Res<Search>,
    cam_query: Query<(&Camera, &WorldPos, &Transform), With<FlyCam>>,
    targets: Query<&WorldPos, Without<FlyCam>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let (Ok((camera, cam_world, cam_local)), Ok(window)) = (cam_query.single(), windows.single())
    else {
        return;
    };
    let Some(rel) = target_offset(&search, cam_world, &targets) else {
        return;
    };
    let vp = Vec2::new(window.width(), window.height());

    let on_screen = camera
        .world_to_viewport(&view_transform(cam_local.rotation), rel)
        .is_ok_and(|sp| sp.x >= 0.0 && sp.x <= vp.x && sp.y >= 0.0 && sp.y <= vp.y);
    if on_screen || rel.length_squared() == 0.0 {
        return;
    }

    let cam_right = cam_local.right().as_vec3();
    let cam_up = cam_local.up().as_vec3();
    let cam_forward = cam_local.forward().as_vec3();
    // Render-space camera position: synced from WorldPos this frame (NavSet::Rebase), and
    // kept within 1 AU of the origin, so gizmos 1e-4 AU in front of it stay precise.
    let cam_pos = cam_local.translation;

    let dir = rel.normalize();
    let in_front = dir.dot(cam_forward) > 0.0;

    let sr = dir.dot(cam_right);
    let sd = -dir.dot(cam_up);
    let (sr, sd) = if in_front { (sr, sd) } else { (-sr, -sd) };

    let screen_dir = Vec2::new(sr, sd);
    if screen_dir.length_squared() < 1e-10 {
        return;
    }
    let screen_dir = screen_dir.normalize();

    let arrow_dir = (cam_right * screen_dir.x - cam_up * screen_dir.y).normalize();
    let arrow_perp = (cam_right * screen_dir.y + cam_up * screen_dir.x).normalize();

    let d = ARROW_DEPTH;
    let v_half = d * 0.4142; // tan(22.5°)
    let h_half = v_half * (vp.x / vp.y);
    let tx = if screen_dir.x.abs() > 1e-6 {
        h_half / screen_dir.x.abs()
    } else {
        f32::MAX
    };
    let ty = if screen_dir.y.abs() > 1e-6 {
        v_half / screen_dir.y.abs()
    } else {
        f32::MAX
    };
    let t = tx.min(ty) * 0.80;

    let center = cam_pos + cam_forward * d;
    let arrow_tip = center + arrow_dir * (t + d * 0.05);
    let base = center + arrow_dir * (t - d * 0.04);
    let wing1 = base + arrow_perp * (d * 0.04);
    let wing2 = base - arrow_perp * (d * 0.04);

    gizmos.line(arrow_tip, wing1, Color::WHITE);
    gizmos.line(arrow_tip, wing2, Color::WHITE);
    gizmos.line(wing1, wing2, Color::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forward_dir_gives_zero_yaw_pitch() {
        let (yaw, pitch) = dir_to_yaw_pitch(Vec3::new(0.0, 0.0, -1.0));
        assert!(yaw.abs() < 0.01, "expected yaw≈0, got {}", yaw);
        assert!(pitch.abs() < 0.01, "expected pitch≈0, got {}", pitch);
    }

    #[test]
    fn test_up_dir_gives_pitch_90() {
        let (_yaw, pitch) = dir_to_yaw_pitch(Vec3::new(0.0, 1.0, 0.0));
        assert!(
            (pitch - 90.0).abs() < 0.01,
            "expected pitch≈90, got {}",
            pitch
        );
    }

    #[test]
    fn test_right_dir_gives_yaw_neg90() {
        let (yaw, pitch) = dir_to_yaw_pitch(Vec3::new(1.0, 0.0, 0.0));
        assert!(pitch.abs() < 0.01, "expected pitch≈0, got {}", pitch);
        assert!(
            (yaw - (-90.0)).abs() < 0.01,
            "expected yaw≈-90, got {}",
            yaw
        );
    }
}
