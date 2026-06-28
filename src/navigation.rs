use bevy::{
    prelude::*,
    window::PrimaryWindow,
};
use crate::camera::FlyCam;
use crate::search::Search;

// Only used by draw_offscreen_arrow for 3D placement of the arrow gizmo.
const ARROW_DEPTH: f32 = 1e-4;

const RETICLE_RADIUS: f32 = 30.0; // pixels
const RETICLE_BORDER: f32 = 2.0;  // pixels

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
    mut cam_query: Query<(&mut Transform, &mut FlyCam)>,
) {
    if !search.should_orient {
        return;
    }
    search.should_orient = false;
    let Some(target) = search.target else { return };
    let Ok((mut transform, mut cam)) = cam_query.single_mut() else { return };

    let dir = (target - transform.translation).normalize();
    let (yaw, pitch) = dir_to_yaw_pitch(dir);
    cam.pitch = pitch.clamp(-89.0, 89.0);
    cam.yaw = yaw;
    transform.rotation = Quat::from_euler(
        EulerRot::YXZ,
        cam.yaw.to_radians(),
        cam.pitch.to_radians(),
        0.0,
    );
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
    cam_query: Query<(&Camera, &GlobalTransform), With<FlyCam>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut reticle_query: Query<(&mut Node, &mut Visibility), With<TargetReticle>>,
) {
    let Ok((mut node, mut vis)) = reticle_query.single_mut() else { return };

    let Some(target) = search.target else {
        *vis = Visibility::Hidden;
        return;
    };
    let Ok((camera, cam_global)) = cam_query.single() else { return };
    let Ok(window) = windows.single() else { return };

    let vp = Vec2::new(window.width(), window.height());

    let on_screen = camera
        .world_to_viewport(cam_global, target)
        .map(|sp| {
            sp.x >= 0.0 && sp.x <= vp.x && sp.y >= 0.0 && sp.y <= vp.y
        })
        .unwrap_or(false);

    if !on_screen {
        *vis = Visibility::Hidden;
        return;
    }

    let screen_pos = camera.world_to_viewport(cam_global, target).unwrap();
    node.left = Val::Px(screen_pos.x - RETICLE_RADIUS);
    node.top = Val::Px(screen_pos.y - RETICLE_RADIUS);
    *vis = Visibility::Visible;
}

pub fn draw_offscreen_arrow(
    mut gizmos: Gizmos,
    search: Res<Search>,
    cam_query: Query<(&Camera, &GlobalTransform, &Transform), With<FlyCam>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Some(target) = search.target else { return };
    let Ok((camera, cam_global, cam_local)) = cam_query.single() else { return };
    let Ok(window) = windows.single() else { return };

    let vp = Vec2::new(window.width(), window.height());

    let on_screen = camera
        .world_to_viewport(cam_global, target)
        .map_or(false, |sp| sp.x >= 0.0 && sp.x <= vp.x && sp.y >= 0.0 && sp.y <= vp.y);
    if on_screen {
        return;
    }

    let cam_right = cam_local.right().as_vec3();
    let cam_up = cam_local.up().as_vec3();
    let cam_forward = cam_local.forward().as_vec3();
    let cam_pos = cam_global.translation();

    let dir = (target - cam_pos).normalize();
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
    let tx = if screen_dir.x.abs() > 1e-6 { h_half / screen_dir.x.abs() } else { f32::MAX };
    let ty = if screen_dir.y.abs() > 1e-6 { v_half / screen_dir.y.abs() } else { f32::MAX };
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
        assert!((pitch - 90.0).abs() < 0.01, "expected pitch≈90, got {}", pitch);
    }

    #[test]
    fn test_right_dir_gives_yaw_neg90() {
        let (yaw, pitch) = dir_to_yaw_pitch(Vec3::new(1.0, 0.0, 0.0));
        assert!(pitch.abs() < 0.01, "expected pitch≈0, got {}", pitch);
        assert!((yaw - (-90.0)).abs() < 0.01, "expected yaw≈-90, got {}", yaw);
    }
}
