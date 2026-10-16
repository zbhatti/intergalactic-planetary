//! Go-to: press G to fly automatically to the selected object.
//!
//! The camera first turns to face the target, then travels along the line toward it,
//! interpolating the *logarithm* of the remaining distance. Crossing 10 AU or 10^12 AU
//! takes a few seconds either way, decelerating smoothly into a stop a few radii from
//! the surface.

use crate::camera::FlyCam;
use crate::coords::{BodyRadius, WorldPos};
use crate::menu::MenuOpen;
use crate::navigation::dir_to_yaw_pitch;
use crate::search::Search;
use bevy::math::DVec3;
use bevy::prelude::*;

pub const TURN_SECS: f64 = 0.6;
/// Stop this many body radii from the target's center.
pub const STOP_RADII: f64 = 4.0;
/// Never stop closer than this, AU (~1.5 km), even for point-like targets.
pub const MIN_STOP_AU: f64 = 1.0e-8;

#[derive(Clone, Copy, Debug)]
pub struct Flight {
    pub target: Entity,
    pub start_rot: Quat,
    /// Unit vector from the start position toward the target.
    pub dir: DVec3,
    pub start_dist: f64,
    pub stop_dist: f64,
    pub travel_secs: f64,
    pub elapsed: f64,
}

impl Flight {
    pub fn new(
        target: Entity,
        cam_pos: DVec3,
        cam_rot: Quat,
        target_pos: DVec3,
        radius: f64,
    ) -> Self {
        let offset = target_pos - cam_pos;
        let start_dist = offset.length();
        let stop_dist = stop_distance(radius);
        let dir = if start_dist > 0.0 {
            offset / start_dist
        } else {
            DVec3::NEG_Z
        };
        let travel_secs = if start_dist > stop_dist {
            travel_duration(start_dist, stop_dist)
        } else {
            0.0 // already close enough: just turn to face it
        };
        Self {
            target,
            start_rot: cam_rot,
            dir,
            start_dist,
            stop_dist,
            travel_secs,
            elapsed: 0.0,
        }
    }

    pub fn total_secs(&self) -> f64 {
        TURN_SECS + self.travel_secs
    }
}

#[derive(Resource, Default)]
pub struct GoTo(pub Option<Flight>);

impl GoTo {
    pub fn is_active(&self) -> bool {
        self.0.is_some()
    }

    pub fn cancel(&mut self) {
        if self.0.is_some() {
            self.0 = None;
        }
    }
}

pub fn stop_distance(radius_au: f64) -> f64 {
    (radius_au * STOP_RADII).max(MIN_STOP_AU)
}

/// 1.5 s plus 0.35 s per factor of ten in distance covered, capped at 8 s.
pub fn travel_duration(start_dist: f64, stop_dist: f64) -> f64 {
    (1.5 + 0.35 * (start_dist / stop_dist).log10().abs()).clamp(1.5, 8.0)
}

pub fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Distance from the target at normalised travel time `s` (0..1): log-space interpolation.
pub fn flight_distance(start_dist: f64, stop_dist: f64, s: f64) -> f64 {
    let (a, b) = (start_dist.ln(), stop_dist.ln());
    (a + (b - a) * smoothstep(s)).exp()
}

pub fn look_rotation(dir: Vec3) -> (f32, f32, Quat) {
    let (yaw, pitch) = dir_to_yaw_pitch(dir);
    let pitch = pitch.clamp(-89.0, 89.0);
    let rot = Quat::from_euler(EulerRot::YXZ, yaw.to_radians(), pitch.to_radians(), 0.0);
    (yaw, pitch, rot)
}

pub fn start_goto(
    keys: Res<ButtonInput<KeyCode>>,
    search: Res<Search>,
    menu_open: Res<MenuOpen>,
    mut goto: ResMut<GoTo>,
    cam: Query<(&WorldPos, &Transform), With<FlyCam>>,
    targets: Query<(&WorldPos, &BodyRadius)>,
) {
    if search.active || menu_open.0 || !keys.just_pressed(KeyCode::KeyG) {
        return;
    }
    let Some(target) = search.target else { return };
    let (Ok((cam_pos, cam_t)), Ok((target_pos, radius))) = (cam.single(), targets.get(target))
    else {
        return;
    };
    goto.0 = Some(Flight::new(
        target,
        cam_pos.0,
        cam_t.rotation,
        target_pos.0,
        radius.0,
    ));
}

pub fn update_goto(
    time: Res<Time>,
    mut goto: ResMut<GoTo>,
    mut cam: Query<(&mut WorldPos, &mut Transform, &mut FlyCam)>,
    targets: Query<&WorldPos, Without<FlyCam>>,
) {
    let Some(flight) = goto.0.as_mut() else {
        return;
    };
    let (Ok((mut cam_pos, mut transform, mut fly)), Ok(target_pos)) =
        (cam.single_mut(), targets.get(flight.target))
    else {
        goto.0 = None;
        return;
    };
    flight.elapsed += time.delta_secs_f64();

    // Travel (after the turn).
    if flight.travel_secs > 0.0 && flight.elapsed > TURN_SECS {
        let s = ((flight.elapsed - TURN_SECS) / flight.travel_secs).min(1.0);
        let d = flight_distance(flight.start_dist, flight.stop_dist, s);
        cam_pos.0 = target_pos.0 - flight.dir * d;
    }

    // Face the target, easing in during the turn phase.
    let to_target = target_pos.0 - cam_pos.0;
    if to_target.length_squared() > 0.0 {
        let (_, _, look) = look_rotation(to_target.normalize().as_vec3());
        let turn = smoothstep(flight.elapsed / TURN_SECS) as f32;
        transform.rotation = flight.start_rot.slerp(look, turn);
        // Keep mouse-look state in step every frame, so cancelling mid-turn (WASD) doesn't
        // snap the view back on the next mouse movement.
        let (yaw, pitch) = dir_to_yaw_pitch(*transform.forward());
        fly.yaw = yaw;
        fly.pitch = pitch.clamp(-89.0, 89.0);
    }

    if flight.elapsed >= flight.total_secs() {
        goto.0 = None;
    }
}

pub struct GoToPlugin;

impl Plugin for GoToPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GoTo>().add_systems(
            Update,
            (start_goto, update_goto)
                .chain()
                .in_set(crate::coords::NavSet::Input),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::{CoordsPlugin, FloatingOrigin};
    use crate::testing::{headless_app, press_key, run_frames};

    #[test]
    fn test_flight_distance_endpoints_and_monotonic() {
        let (d0, d1) = (1.0e11, 4.0);
        assert!((flight_distance(d0, d1, 0.0) - d0).abs() / d0 < 1e-12);
        assert!((flight_distance(d0, d1, 1.0) - d1).abs() < 1e-9);
        let mut prev = f64::INFINITY;
        for i in 0..=100 {
            let d = flight_distance(d0, d1, i as f64 / 100.0);
            assert!(d <= prev, "distance must never increase");
            prev = d;
        }
    }

    #[test]
    fn test_travel_duration_bounds() {
        assert_eq!(travel_duration(10.0, 10.0), 1.5);
        assert!(travel_duration(1.0e12, 1.0e-4) <= 8.0);
        assert!(travel_duration(1.0e3, 1.0) > travel_duration(10.0, 1.0));
    }

    #[test]
    fn test_stop_distance_has_floor() {
        assert_eq!(stop_distance(0.0), MIN_STOP_AU);
        assert_eq!(stop_distance(1.0), STOP_RADII);
    }

    fn goto_app() -> App {
        let mut app = headless_app();
        app.add_plugins((CoordsPlugin, GoToPlugin))
            .init_resource::<Search>()
            .init_resource::<MenuOpen>();
        app
    }

    /// Flies to a body ~5 Mpc away and checks we arrive at the right distance, facing it,
    /// with render-space positions that still agree with f64 world space.
    #[test]
    fn test_goto_far_body_arrives_precisely() {
        let mut app = goto_app();
        let far = DVec3::new(-6.0e11, 2.0e11, 8.0e11); // ~1e12 AU ≈ 5 Mpc
        let radius = 2.0;
        let body = app
            .world_mut()
            .spawn((WorldPos(far), BodyRadius(radius), Transform::default()))
            .id();
        let cam = app
            .world_mut()
            .spawn((
                FlyCam::default(),
                WorldPos(DVec3::new(0.0, 0.0, 1.0)),
                Transform::default(),
            ))
            .id();
        app.world_mut().resource_mut::<Search>().target = Some(body);
        press_key(&mut app, KeyCode::KeyG);
        run_frames(&mut app, 1);
        assert!(app.world().resource::<GoTo>().is_active());

        run_frames(&mut app, 60 * 10); // longer than the 8 s cap + turn
        assert!(!app.world().resource::<GoTo>().is_active());

        let cam_pos = app.world().get::<WorldPos>(cam).unwrap().0;
        let dist = cam_pos.distance(far);
        assert!(
            (dist - stop_distance(radius)).abs() < 1e-3,
            "arrived at {dist} AU, expected {}",
            stop_distance(radius)
        );

        // Facing the body.
        let cam_t = *app.world().get::<Transform>(cam).unwrap();
        let body_t = *app.world().get::<Transform>(body).unwrap();
        let to_body = (body_t.translation - cam_t.translation).normalize();
        assert!(cam_t.forward().dot(to_body) > 0.999);

        // Rendering stays precise: origin is near the camera, render offset matches f64.
        let origin = app.world().resource::<FloatingOrigin>().0;
        assert!(origin.distance(cam_pos) <= 1.0);
        let render_dist = (body_t.translation - cam_t.translation).length() as f64;
        assert!(
            (render_dist - dist).abs() < 1e-4,
            "render {render_dist} vs world {dist}"
        );
    }

    #[test]
    fn test_wasd_cancels_goto() {
        let mut app = goto_app();
        app.init_resource::<crate::speed::CurrentSpeed>()
            .add_systems(
                Update,
                crate::camera::fly_camera
                    .after(update_goto)
                    .in_set(crate::coords::NavSet::Input),
            );
        let body = app
            .world_mut()
            .spawn((
                WorldPos(DVec3::X * 1.0e6),
                BodyRadius(1.0),
                Transform::default(),
            ))
            .id();
        app.world_mut().spawn((
            FlyCam::default(),
            WorldPos(DVec3::ZERO),
            Transform::default(),
        ));
        app.world_mut().resource_mut::<Search>().target = Some(body);
        press_key(&mut app, KeyCode::KeyG);
        run_frames(&mut app, 30);
        assert!(app.world().resource::<GoTo>().is_active());
        press_key(&mut app, KeyCode::KeyW);
        run_frames(&mut app, 1);
        assert!(!app.world().resource::<GoTo>().is_active());
    }

    #[test]
    fn test_cancel_mid_turn_keeps_mouse_look_in_sync() {
        let mut app = goto_app();
        app.init_resource::<crate::speed::CurrentSpeed>()
            .add_systems(
                Update,
                crate::camera::fly_camera
                    .after(update_goto)
                    .in_set(crate::coords::NavSet::Input),
            );
        // Target off to the side (+X) so the turn is a large yaw.
        let body = app
            .world_mut()
            .spawn((
                WorldPos(DVec3::X * 1.0e6),
                BodyRadius(1.0),
                Transform::default(),
            ))
            .id();
        let cam = app
            .world_mut()
            .spawn((
                FlyCam::default(),
                WorldPos(DVec3::ZERO),
                Transform::default(),
            ))
            .id();
        app.world_mut().resource_mut::<Search>().target = Some(body);
        press_key(&mut app, KeyCode::KeyG);
        run_frames(&mut app, 10); // ~0.17 s into the 0.6 s turn
        press_key(&mut app, KeyCode::KeyW);
        run_frames(&mut app, 1);
        assert!(!app.world().resource::<GoTo>().is_active());

        let fly_forward = app.world().get::<FlyCam>(cam).unwrap().rotation() * Vec3::NEG_Z;
        let cam_forward = *app.world().get::<Transform>(cam).unwrap().forward();
        assert!(
            fly_forward.dot(cam_forward) > 0.9999,
            "mouse-look state {fly_forward} diverged from view {cam_forward}"
        );
        // And the turn really was partial (not already at the target).
        assert!(cam_forward.dot(Vec3::X) < 0.99);
    }

    #[test]
    fn test_g_ignored_while_typing_in_search() {
        let mut app = goto_app();
        let body = app
            .world_mut()
            .spawn((
                WorldPos(DVec3::X * 10.0),
                BodyRadius(1.0),
                Transform::default(),
            ))
            .id();
        app.world_mut().spawn((
            FlyCam::default(),
            WorldPos(DVec3::ZERO),
            Transform::default(),
        ));
        {
            let mut search = app.world_mut().resource_mut::<Search>();
            search.target = Some(body);
            search.active = true;
        }
        press_key(&mut app, KeyCode::KeyG);
        run_frames(&mut app, 1);
        assert!(!app.world().resource::<GoTo>().is_active());
    }
}
