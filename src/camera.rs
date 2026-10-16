use crate::coords::WorldPos;
use crate::goto::GoTo;
use crate::menu::MenuOpen;
use crate::search::Search;
use crate::speed::CurrentSpeed;
use bevy::{
    core_pipeline::bloom::Bloom,
    input::mouse::MouseMotion,
    math::DVec3,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    window::CursorGrabMode,
};

#[derive(Resource)]
pub struct CursorLocked(pub bool);

#[derive(Component)]
pub struct FlyCam {
    pub sensitivity: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for FlyCam {
    fn default() -> Self {
        Self {
            sensitivity: 0.1,
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

impl FlyCam {
    pub fn rotation(&self) -> Quat {
        Quat::from_euler(
            EulerRot::YXZ,
            self.yaw.to_radians(),
            self.pitch.to_radians(),
            0.0,
        )
    }
}

/// Far plane for frustum culling, AU (~5 Gpc). Depth uses Bevy's infinite reverse-Z, so this
/// only decides what gets culled; it must exceed the farthest object we ever draw.
pub const CAMERA_FAR_AU: f32 = 1.0e15;
/// Near plane, AU (~150 m).
pub const CAMERA_NEAR_AU: f32 = 1.0e-6;

pub fn setup_camera(mut commands: Commands, mut windows: Query<&mut Window>) {
    if let Ok(mut window) = windows.single_mut() {
        window.cursor_options.grab_mode = CursorGrabMode::Locked;
        window.cursor_options.visible = false;
    }
    commands.spawn((
        Camera3d::default(),
        Camera {
            hdr: true,
            ..default()
        },
        Bloom::NATURAL,
        Projection::Perspective(PerspectiveProjection {
            near: CAMERA_NEAR_AU,
            far: CAMERA_FAR_AU,
            ..default()
        }),
        Transform::default(),
        WorldPos(DVec3::new(0.0, 0.002, 0.01)),
        FlyCam::default(),
    ));
}

pub fn mouse_look(
    locked: Res<CursorLocked>,
    search: Res<Search>,
    goto: Res<GoTo>,
    mut motion: EventReader<MouseMotion>,
    mut query: Query<(&mut Transform, &mut FlyCam)>,
) {
    if !locked.0 || search.active || goto.is_active() {
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
        transform.rotation = cam.rotation();
    }
}

#[derive(Resource, Default)]
pub struct ScreenshotCounter(u32);

pub fn take_screenshot(
    mut commands: Commands,
    mut counter: ResMut<ScreenshotCounter>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    if !keys.just_pressed(KeyCode::F12) {
        return;
    }
    counter.0 += 1;
    let path = format!("screenshot_{:04}.png", counter.0);
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

/// WASD flight in world space (f64) at the current adaptive speed. Any movement key
/// cancels an in-progress go-to.
pub fn fly_camera(
    keys: Res<ButtonInput<KeyCode>>,
    search: Res<Search>,
    menu_open: Res<MenuOpen>,
    speed: Res<CurrentSpeed>,
    time: Res<Time>,
    mut goto: ResMut<GoTo>,
    mut query: Query<(&mut WorldPos, &Transform), With<FlyCam>>,
) {
    if search.active || menu_open.0 {
        return;
    }
    let Ok((mut pos, transform)) = query.single_mut() else {
        return;
    };
    let mut dir = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        dir += *transform.forward();
    }
    if keys.pressed(KeyCode::KeyS) {
        dir -= *transform.forward();
    }
    if keys.pressed(KeyCode::KeyA) {
        dir -= *transform.right();
    }
    if keys.pressed(KeyCode::KeyD) {
        dir += *transform.right();
    }
    if dir == Vec3::ZERO {
        return;
    }
    goto.cancel();
    pos.0 += dir.normalize().as_dvec3() * speed.au_per_sec * time.delta_secs_f64();
}
