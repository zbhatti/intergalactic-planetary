//! Field-of-view zoom: the mouse wheel narrows or widens the vertical FOV, middle click resets.
//!
//! Point-like objects (stars, distant planets) are drawn at least [`min_star_angle`] wide. That
//! angle scales with the FOV, so they keep the same size in pixels at every zoom instead of
//! ballooning when zoomed in.

use crate::camera::FlyCam;
use crate::coords::NavSet;
use crate::menu::MenuOpen;
use crate::search::Search;
use crate::stars::MIN_STAR_ANGLE;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

/// Bevy's default vertical FOV (45 deg). MIN_STAR_ANGLE was tuned at this FOV.
pub const DEFAULT_FOV: f32 = std::f32::consts::FRAC_PI_4;
pub const MIN_FOV: f32 = 0.0001745; // 0.01 deg
pub const MAX_FOV: f32 = 2.0944; // 120 deg
/// FOV factor per wheel notch.
pub const ZOOM_STEP: f32 = 1.2;

/// Vertical field of view, radians. The camera's projection follows it.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CameraFov(pub f32);

impl Default for CameraFov {
    fn default() -> Self {
        Self(DEFAULT_FOV)
    }
}

impl CameraFov {
    /// Zoom in (`notches > 0`) or out by wheel notches, clamped to the FOV range.
    pub fn zoomed(self, notches: f32) -> Self {
        Self((self.0 / ZOOM_STEP.powf(notches)).clamp(MIN_FOV, MAX_FOV))
    }

    /// Magnification relative to the default FOV, as shown in the HUD.
    pub fn magnification(self) -> f32 {
        (DEFAULT_FOV * 0.5).tan() / (self.0 * 0.5).tan()
    }

    pub fn label(self) -> String {
        let deg = self.0.to_degrees();
        let fov = if deg >= 10.0 {
            format!("{deg:.0}")
        } else if deg >= 1.0 {
            format!("{deg:.1}")
        } else {
            format!("{deg:.3}")
        };
        let mag = self.magnification();
        if (mag - 1.0).abs() < 0.01 {
            format!("FOV {fov} deg")
        } else {
            format!("FOV {fov} deg (x{})", crate::info::format_sig(mag as f64))
        }
    }
}

/// Minimum angular radius for point-like objects at this FOV (radians).
pub fn min_star_angle(fov: f32) -> f32 {
    MIN_STAR_ANGLE * fov / DEFAULT_FOV
}

/// Mouse-look sensitivity factor: turning slows down as you zoom in.
pub fn look_scale(fov: f32) -> f32 {
    fov / DEFAULT_FOV
}

/// Wheel lines (or pixels, ~1 notch per 100 px on touchpads) to notches.
pub fn wheel_notches(unit: MouseScrollUnit, y: f32) -> f32 {
    match unit {
        MouseScrollUnit::Line => y,
        MouseScrollUnit::Pixel => y / 100.0,
    }
}

pub fn zoom_input(
    mut wheel: EventReader<MouseWheel>,
    buttons: Res<ButtonInput<MouseButton>>,
    search: Res<Search>,
    menu_open: Res<MenuOpen>,
    mut fov: ResMut<CameraFov>,
) {
    let notches: f32 = wheel.read().map(|e| wheel_notches(e.unit, e.y)).sum();
    if search.active || menu_open.0 {
        return;
    }
    if buttons.just_pressed(MouseButton::Middle) {
        fov.set_if_neq(CameraFov::default());
    } else if notches != 0.0 {
        let new = fov.zoomed(notches);
        fov.set_if_neq(new);
    }
}

pub fn apply_fov(fov: Res<CameraFov>, mut cams: Query<&mut Projection, With<FlyCam>>) {
    if !fov.is_changed() {
        return;
    }
    for mut projection in &mut cams {
        if let Projection::Perspective(p) = projection.as_mut() {
            p.fov = fov.0;
        }
    }
}

pub struct ZoomPlugin;

impl Plugin for ZoomPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraFov>().add_systems(
            Update,
            (zoom_input, apply_fov).chain().in_set(NavSet::Input),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::headless_app;
    use bevy::input::ButtonState;
    use bevy::input::mouse::MouseButtonInput;

    #[test]
    fn test_zoom_steps_and_clamps() {
        let fov = CameraFov::default();
        assert!(fov.zoomed(1.0).0 < fov.0);
        assert!((fov.zoomed(1.0).zoomed(-1.0).0 - fov.0).abs() < 1e-6);
        assert_eq!(fov.zoomed(1000.0).0, MIN_FOV);
        assert_eq!(fov.zoomed(-1000.0).0, MAX_FOV);
    }

    #[test]
    fn test_points_keep_their_pixel_size_when_zooming() {
        // Pixel size ~ angle / tan(fov/2); the min angle must shrink with the FOV.
        let px = |fov: f32| min_star_angle(fov) / (fov * 0.5).tan();
        let wide = px(DEFAULT_FOV);
        let narrow = px(DEFAULT_FOV / 100.0);
        assert!((narrow / wide - 1.0).abs() < 0.25, "{wide} vs {narrow}");
        assert_eq!(min_star_angle(DEFAULT_FOV), MIN_STAR_ANGLE);
    }

    #[test]
    fn test_fov_label() {
        assert_eq!(CameraFov::default().label(), "FOV 45 deg");
        let zoomed = CameraFov(1.0f32.to_radians()).label();
        assert!(zoomed.starts_with("FOV 1.0 deg (x"), "{zoomed}");
        assert!(zoomed.is_ascii());
    }

    fn zoom_app() -> App {
        let mut app = headless_app();
        app.init_resource::<Search>()
            .init_resource::<MenuOpen>()
            .add_event::<MouseWheel>()
            .add_plugins(ZoomPlugin);
        app.world_mut().spawn((
            FlyCam::default(),
            Projection::Perspective(PerspectiveProjection::default()),
        ));
        app.update();
        app
    }

    fn scroll(app: &mut App, y: f32) {
        app.world_mut().send_event(MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y,
            window: Entity::PLACEHOLDER,
        });
        app.update();
    }

    fn camera_fov(app: &mut App) -> f32 {
        let mut q = app.world_mut().query::<&Projection>();
        match q.single(app.world()).unwrap() {
            Projection::Perspective(p) => p.fov,
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_wheel_zooms_the_camera_and_middle_click_resets() {
        let mut app = zoom_app();
        scroll(&mut app, 3.0);
        let fov = camera_fov(&mut app);
        assert!(
            (fov - DEFAULT_FOV / ZOOM_STEP.powi(3)).abs() < 1e-5,
            "{fov}"
        );

        app.world_mut().send_event(MouseButtonInput {
            button: MouseButton::Middle,
            state: ButtonState::Pressed,
            window: Entity::PLACEHOLDER,
        });
        app.update();
        assert_eq!(camera_fov(&mut app), DEFAULT_FOV);
    }

    #[test]
    fn test_wheel_ignored_while_searching() {
        let mut app = zoom_app();
        app.world_mut().resource_mut::<Search>().active = true;
        scroll(&mut app, 3.0);
        assert_eq!(camera_fov(&mut app), DEFAULT_FOV);
    }
}
