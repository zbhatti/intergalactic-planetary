use bevy::{
    core_pipeline::bloom::Bloom,
    input::mouse::MouseMotion,
    prelude::*,
    window::CursorGrabMode,
};
use crate::constants::C_AU_PER_S;
use crate::search::Search;
use crate::speed::SpeedState;

#[derive(Resource)]
pub struct CursorLocked(pub bool);

#[derive(Component)]
pub struct FlyCam {
    pub speed: f32,
    pub sensitivity: f32,
    pub yaw: f32,
    pub pitch: f32,
}

pub fn setup_camera(mut commands: Commands, mut windows: Query<&mut Window>) {
    if let Ok(mut window) = windows.single_mut() {
        window.cursor_options.grab_mode = CursorGrabMode::Locked;
        window.cursor_options.visible = false;
    }
    commands.spawn((
        Camera3d::default(),
        Camera { hdr: true, ..default() },
        Bloom::NATURAL,
        Projection::Perspective(PerspectiveProjection {
            near: 1e-6,
            far: 2_000_000.0,
            ..default()
        }),
        Transform::from_xyz(0.0, 0.002, 0.01),
        FlyCam {
            speed: C_AU_PER_S,
            sensitivity: 0.1,
            yaw: 0.0,
            pitch: 0.0,
        },
    ));
}

pub fn toggle_cursor(
    keys: Res<ButtonInput<KeyCode>>,
    search: Res<Search>,
    mut locked: ResMut<CursorLocked>,
    mut windows: Query<&mut Window>,
) {
    if search.active {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        locked.0 = !locked.0;
        if let Ok(mut window) = windows.single_mut() {
            window.cursor_options.grab_mode = if locked.0 {
                CursorGrabMode::Locked
            } else {
                CursorGrabMode::None
            };
            window.cursor_options.visible = !locked.0;
        }
    }
}

pub fn mouse_look(
    locked: Res<CursorLocked>,
    search: Res<Search>,
    mut motion: EventReader<MouseMotion>,
    mut query: Query<(&mut Transform, &mut FlyCam)>,
) {
    if !locked.0 || search.active {
        motion.clear();
        return;
    }
    let Ok((mut transform, mut cam)) = query.single_mut() else {
        return;
    };
    for ev in motion.read() {
        cam.yaw -= ev.delta.x * cam.sensitivity;
        cam.pitch -= ev.delta.y * cam.sensitivity;
        cam.pitch = cam.pitch.clamp(-89.0, 89.0);
        transform.rotation = Quat::from_euler(
            EulerRot::YXZ,
            cam.yaw.to_radians(),
            cam.pitch.to_radians(),
            0.0,
        );
    }
}

pub fn fly_camera(
    keys: Res<ButtonInput<KeyCode>>,
    search: Res<Search>,
    speed_state: Res<SpeedState>,
    time: Res<Time>,
    mut query: Query<(&mut Transform, &FlyCam)>,
) {
    if search.active {
        return;
    }
    let Ok((mut transform, cam)) = query.single_mut() else {
        return;
    };
    let forward = transform.forward().as_vec3();
    let right = transform.right().as_vec3();
    let speed = cam.speed * speed_state.multiplier() * time.delta_secs();
    if keys.pressed(KeyCode::KeyW) {
        transform.translation += forward * speed;
    }
    if keys.pressed(KeyCode::KeyS) {
        transform.translation -= forward * speed;
    }
    if keys.pressed(KeyCode::KeyA) {
        transform.translation -= right * speed;
    }
    if keys.pressed(KeyCode::KeyD) {
        transform.translation += right * speed;
    }
}
