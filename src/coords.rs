//! World coordinates and the floating origin.
//!
//! World space is f64 in AU with the Sun at 0 ([`WorldPos`]) — precise from metres to beyond
//! the observable universe. Bevy renders in f32, so each entity's `Transform` holds its
//! position relative to a [`FloatingOrigin`] kept near the camera. When the camera strays too
//! far from the origin for f32 to stay precise around it, the origin jumps to the camera and
//! every render transform is rewritten ("rebase").
//!
//! Limit: f64 has ~16 significant digits, so world space resolves ~1e-9 AU (150 m) out to
//! ~1e7 AU (50 pc) but only ~1.5e-5 AU (~2000 km) at 1e11 AU (~0.5 Mpc). Fine for stars and
//! galaxies; planets and moons around distant stars will need positions stored relative to
//! their star (a local frame) before Phase 4 places such systems far from the Sun.
//!
//! Rule: game logic reads and writes `WorldPos`, never `Transform::translation`. Screen
//! projection goes through [`view_transform`] + camera-relative offsets, so it doesn't
//! depend on where the origin currently is.

use crate::camera::FlyCam;
use bevy::math::DVec3;
use bevy::prelude::*;

/// Position in world space: AU, heliocentric, f64.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldPos(pub DVec3);

/// Physical radius in AU. Entities with this are navigation targets: they count for
/// nearest-object distance (adaptive speed, rebasing) and go-to stop distance.
/// Decorative entities (background stars) deliberately don't have one.
#[derive(Component, Clone, Copy, Debug)]
pub struct BodyRadius(pub f64);

/// World position that render-space (f32 `Transform`) coordinates are relative to.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct FloatingOrigin(pub DVec3);

/// Distance from the camera to the closest navigable surface, AU (0 when inside a body).
#[derive(Resource, Clone, Copy, Debug)]
pub struct Nearest {
    pub surface_dist: f64,
}

impl Default for Nearest {
    fn default() -> Self {
        Self {
            surface_dist: f64::INFINITY,
        }
    }
}

/// f32 has ~7 significant digits. Keeping the camera within this many multiples of the
/// nearest-surface distance from the origin keeps rendering error around the nearest object
/// below ~1e-5 of its distance — far below a pixel.
const REBASE_NEAREST_FACTOR: f64 = 100.0;
/// Absolute cap, AU. Camera-attached gizmos (the off-screen arrow, drawn 1e-4 AU in front
/// of the camera) need the camera's own render position to be precise.
const REBASE_MAX_AU: f64 = 1.0;

pub fn render_offset(world: DVec3, origin: DVec3) -> Vec3 {
    (world - origin).as_vec3()
}

/// How far (AU) the camera may drift from the origin before a rebase.
pub fn rebase_threshold(nearest_surface_dist: f64) -> f64 {
    (nearest_surface_dist * REBASE_NEAREST_FACTOR).min(REBASE_MAX_AU)
}

/// Camera transform for screen projection with the camera at the origin. Project
/// `(object_world - camera_world)` through this to get rebase-independent screen positions.
pub fn view_transform(rotation: Quat) -> GlobalTransform {
    GlobalTransform::from(Transform::from_rotation(rotation))
}

/// Distance from `point` to the surface of a sphere (0 inside).
pub fn surface_distance(point: DVec3, center: DVec3, radius: f64) -> f64 {
    (point.distance(center) - radius).max(0.0)
}

/// Brute-force nearest surface over all navigable bodies. O(n); see PERF.md for timings.
pub fn find_nearest(
    point: DVec3,
    bodies: impl Iterator<Item = (Entity, DVec3, f64)>,
) -> (Option<Entity>, f64) {
    let mut best = (None, f64::INFINITY);
    for (e, center, radius) in bodies {
        let d = surface_distance(point, center, radius);
        if d < best.1 {
            best = (Some(e), d);
        }
    }
    best
}

/// Per-frame ordering in `Update`. Input moves the camera in world space; Rebase finds the
/// nearest body, shifts the origin if needed, and writes render transforms; View reads
/// final positions (labels, HUD, brightness, gizmos).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum NavSet {
    Input,
    Rebase,
    View,
}

pub fn update_nearest(
    cam: Query<&WorldPos, With<FlyCam>>,
    bodies: Query<(Entity, &WorldPos, &BodyRadius)>,
    mut nearest: ResMut<Nearest>,
) {
    let Ok(cam) = cam.single() else { return };
    let (_, surface_dist) = find_nearest(cam.0, bodies.iter().map(|(e, p, r)| (e, p.0, r.0)));
    nearest.surface_dist = surface_dist;
}

pub fn rebase_origin(
    cam: Query<&WorldPos, With<FlyCam>>,
    nearest: Res<Nearest>,
    mut origin: ResMut<FloatingOrigin>,
) {
    let Ok(cam) = cam.single() else { return };
    if cam.0.distance(origin.0) > rebase_threshold(nearest.surface_dist) {
        origin.0 = cam.0;
    }
}

/// Writes `Transform::translation` from `WorldPos`: for every entity after a rebase,
/// otherwise only for entities whose `WorldPos` changed.
pub fn sync_render_transforms(
    origin: Res<FloatingOrigin>,
    mut query: Query<(Ref<WorldPos>, &mut Transform)>,
) {
    let all = origin.is_changed();
    for (pos, mut transform) in &mut query {
        if all || pos.is_changed() {
            transform.translation = render_offset(pos.0, origin.0);
        }
    }
}

pub struct CoordsPlugin;

impl Plugin for CoordsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FloatingOrigin>()
            .init_resource::<Nearest>()
            .configure_sets(
                Update,
                (NavSet::Input, NavSet::Rebase, NavSet::View).chain(),
            )
            .add_systems(
                Update,
                (update_nearest, rebase_origin, sync_render_transforms)
                    .chain()
                    .in_set(NavSet::Rebase),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{headless_app, run_frames};

    #[test]
    fn test_surface_distance_is_zero_inside() {
        assert_eq!(surface_distance(DVec3::ZERO, DVec3::X, 2.0), 0.0);
        assert!((surface_distance(DVec3::ZERO, DVec3::X * 10.0, 2.0) - 8.0).abs() < 1e-12);
    }

    #[test]
    fn test_find_nearest_uses_surface_not_center() {
        let big = Entity::from_raw(1);
        let small = Entity::from_raw(2);
        // Big body's center is farther away, but its surface is closer.
        let bodies = [(big, DVec3::X * 10.0, 9.0), (small, DVec3::X * 5.0, 0.1)];
        let (e, d) = find_nearest(DVec3::ZERO, bodies.into_iter());
        assert_eq!(e, Some(big));
        assert!((d - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_find_nearest_empty_is_infinite() {
        let (e, d) = find_nearest(DVec3::ZERO, std::iter::empty());
        assert_eq!(e, None);
        assert!(d.is_infinite());
    }

    #[test]
    fn test_rebase_threshold_scales_and_caps() {
        assert_eq!(rebase_threshold(0.0), 0.0);
        assert!((rebase_threshold(1e-4) - 1e-2).abs() < 1e-12);
        assert_eq!(rebase_threshold(1e9), REBASE_MAX_AU);
    }

    #[test]
    fn test_render_offset_keeps_precision_far_from_sun() {
        // ~0.5 Mpc away: the absolute value is useless as f32, but the offset from a nearby
        // origin is exact.
        let origin = DVec3::new(1.0e11, -3.0e10, 7.0e10);
        let world = origin + DVec3::new(1.0, 0.0, 0.0);
        let off = render_offset(world, origin);
        assert!((off.x as f64 - 1.0).abs() < 1e-6, "offset was {}", off.x);
    }

    #[test]
    fn test_f64_world_resolution_limit_is_documented() {
        // f64 world space resolves ~1.5e-5 AU (~2000 km) at 1e11 AU. If this ever changes
        // (e.g. hierarchical frames), update the module docs.
        let far = 1.0e11_f64;
        let ulp = f64::from_bits(far.to_bits() + 1) - far;
        assert!(ulp > 1.0e-5 && ulp < 2.0e-5, "ulp at 1e11 AU was {ulp}");
    }

    fn coords_app() -> App {
        let mut app = headless_app();
        app.add_plugins(CoordsPlugin);
        app
    }

    #[test]
    fn test_rebase_follows_camera_and_preserves_relative_render_positions() {
        let mut app = coords_app();
        let far = DVec3::new(3.0e11, 1.0e10, -2.0e11); // ~1.8 Mpc from the Sun
        let body = app
            .world_mut()
            .spawn((WorldPos(far), BodyRadius(1.0), Transform::default()))
            .id();
        let cam = app
            .world_mut()
            .spawn((
                FlyCam::default(),
                WorldPos(far + DVec3::new(0.0, 0.0, 4.0)),
                Transform::default(),
            ))
            .id();
        run_frames(&mut app, 2);

        let origin = app.world().resource::<FloatingOrigin>().0;
        assert!(
            origin.distance(far) < 10.0,
            "origin should have moved to the camera"
        );
        let body_t = app.world().get::<Transform>(body).unwrap().translation;
        let cam_t = app.world().get::<Transform>(cam).unwrap().translation;
        let rel = body_t - cam_t;
        assert!(
            (rel - Vec3::new(0.0, 0.0, -4.0)).length() < 1e-5,
            "relative was {rel}"
        );
        let nearest = app.world().resource::<Nearest>();
        assert!((nearest.surface_dist - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_no_rebase_while_camera_stays_close_to_origin() {
        let mut app = coords_app();
        app.world_mut().spawn((
            WorldPos(DVec3::ZERO),
            BodyRadius(0.0047),
            Transform::default(),
        ));
        app.world_mut().spawn((
            FlyCam::default(),
            WorldPos(DVec3::new(0.0, 0.0, 0.5)),
            Transform::default(),
        ));
        run_frames(&mut app, 1);
        let first = *app.world().resource::<FloatingOrigin>();
        // Nudge the camera by far less than the threshold (~100 AU, capped at 1 AU).
        let mut q = app
            .world_mut()
            .query_filtered::<&mut WorldPos, With<FlyCam>>();
        q.single_mut(app.world_mut()).unwrap().0.z += 0.001;
        run_frames(&mut app, 1);
        assert_eq!(*app.world().resource::<FloatingOrigin>(), first);
    }

    // --- micro-benchmarks: cargo test --release -- --ignored --nocapture perf_ ---

    fn bodies(n: usize) -> Vec<(Entity, DVec3, f64)> {
        (0..n)
            .map(|i| {
                let f = i as f64;
                let p = DVec3::new((f * 7.3).sin(), (f * 3.1).cos(), (f * 1.7).sin())
                    * 1.0e7
                    * (1.0 + f);
                (Entity::from_raw(i as u32 + 1), p, 0.005)
            })
            .collect()
    }

    fn time_per_call(iters: u32, mut f: impl FnMut()) -> std::time::Duration {
        let start = std::time::Instant::now();
        for _ in 0..iters {
            f();
        }
        start.elapsed() / iters
    }

    #[test]
    #[ignore]
    fn perf_find_nearest() {
        for n in [2_000, 120_000] {
            let b = bodies(n);
            let t = time_per_call(50, || {
                std::hint::black_box(find_nearest(DVec3::ONE, b.iter().copied()));
            });
            println!("perf_find_nearest n={n}: {t:?}");
        }
    }

    #[test]
    #[ignore]
    fn perf_rebase_sync() {
        for n in [2_000, 120_000] {
            let mut app = coords_app();
            for (_, p, r) in bodies(n) {
                app.world_mut()
                    .spawn((WorldPos(p), BodyRadius(r), Transform::default()));
            }
            let cam = app
                .world_mut()
                .spawn((
                    FlyCam::default(),
                    WorldPos(DVec3::ZERO),
                    Transform::default(),
                ))
                .id();
            run_frames(&mut app, 2);
            // Move the camera 10 AU every frame: forces a rebase (full transform rewrite)
            // plus the nearest scan, i.e. the worst case for fast flight.
            let t = time_per_call(30, || {
                app.world_mut().get_mut::<WorldPos>(cam).unwrap().0.x += 10.0;
                app.update();
            });
            println!(
                "perf_rebase_sync n={n}: {t:?} per frame (incl. nearest scan + transform propagation)"
            );
        }
    }
}
