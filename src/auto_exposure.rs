//! Auto-exposure (eye adaptation), computed on the CPU.
//!
//! Each frame it finds the brightest star in view at the *manual* exposure: star spheres (the
//! curated stars, the close-range proxy, and the Sun), and the nearest catalog point. If that
//! star would be brighter than [`TARGET_PEAK`], it lowers the exposure until it isn't. It never
//! raises the exposure above the manual setting, so looking at an ordinary patch of sky changes
//! nothing. Near a star the rest of the sky goes dark, as it would for your eyes or a camera.
//!
//! Darkening adapts quickly and brightening more slowly, so turning toward the Sun doesn't flash,
//! and turning away fades the stars back in.

use crate::camera::FlyCam;
use crate::coords::{NavSet, WorldPos};
use crate::hyg::HygCatalog;
use crate::starfield::StarPoints;
use crate::stars::{
    ExposureSettings, SOLAR_RADIUS_UNITS, Star, sphere_brightness_raw as raw_brightness,
};
use crate::zoom::{CameraFov, min_star_angle};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// The brightest star in view is exposed to this emissive brightness (Sirius from Earth is
/// ~1.4 at EV 0, so the night sky from home is left alone).
pub const TARGET_PEAK: f32 = 2.0;
/// Lowest auto adjustment, EV. The Sun from Mercury needs about -32.
pub const MIN_AUTO_OFFSET: f32 = -40.0;
/// Adaptation time constants, seconds.
const DARKEN_TAU: f32 = 0.25;
const BRIGHTEN_TAU: f32 = 1.0;

/// Auto adjustment (EV, <= 0) that brings `peak` (brightness at the manual exposure) down to
/// [`TARGET_PEAK`].
pub fn target_offset(peak: f32) -> f32 {
    if peak <= TARGET_PEAK {
        0.0
    } else {
        (TARGET_PEAK / peak).log2().max(MIN_AUTO_OFFSET)
    }
}

/// One adaptation step from `current` toward `target` over `dt` seconds.
pub fn adapt(current: f32, target: f32, dt: f32) -> f32 {
    let tau = if target < current {
        DARKEN_TAU
    } else {
        BRIGHTEN_TAU
    };
    let next = target + (current - target) * (-dt / tau).exp();
    if (next - target).abs() < 0.01 {
        target
    } else {
        next
    }
}

/// Half the diagonal field of view (radians): anything within this angle of the view axis can
/// be on screen.
pub fn half_diagonal_fov(fov_y: f32, aspect: f32) -> f32 {
    ((fov_y * 0.5).tan() * (1.0 + aspect * aspect).sqrt()).atan()
}

#[allow(clippy::too_many_arguments)]
pub fn auto_exposure(
    time: Res<Time>,
    mut exposure: ResMut<ExposureSettings>,
    fov: Res<CameraFov>,
    cam: Query<(&WorldPos, &Transform), With<FlyCam>>,
    stars: Query<(&WorldPos, &Star), Without<FlyCam>>,
    points: Option<Res<StarPoints>>,
    catalog: Option<Res<HygCatalog>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if !exposure.auto {
        return;
    }
    let Ok((cam_pos, cam_t)) = cam.single() else {
        return;
    };
    let aspect = windows
        .single()
        .map(|w| w.width() / w.height().max(1.0))
        .unwrap_or(16.0 / 9.0);
    let half_view = half_diagonal_fov(fov.0, aspect) as f64;
    let forward = cam_t.forward().as_dvec3();
    let min_angle = min_star_angle(fov.0);
    let manual = exposure.manual_factor();
    let in_view = |rel: DVec3, radius: f64| {
        let dist = rel.length();
        let ang_radius = (radius / dist.max(radius)).asin();
        forward.angle_between(rel) <= half_view + ang_radius
    };

    let mut peak = 0.0f32;
    for (pos, star) in &stars {
        let rel = pos.0 - cam_pos.0;
        let radius = star.radius_solar as f64 * SOLAR_RADIUS_UNITS;
        if in_view(rel, radius) {
            peak = peak.max(raw_brightness(
                manual,
                star.visual_lum,
                radius,
                rel.length(),
                min_angle,
            ));
        }
    }
    if let (Some(points), Some(catalog)) = (points, catalog)
        && let Some((i, _)) = points.nearest
    {
        let s = &catalog.stars[i];
        let rel = s.pos - cam_pos.0;
        if in_view(rel, 0.0) {
            peak = peak.max(raw_brightness(manual, s.lum, 0.0, rel.length(), min_angle));
        }
    }

    let next = adapt(exposure.auto_offset, target_offset(peak), time.delta_secs());
    if next != exposure.auto_offset {
        exposure.auto_offset = next;
    }
}

pub struct AutoExposurePlugin;

impl Plugin for AutoExposurePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            auto_exposure
                .in_set(NavSet::View)
                .before(crate::stars::update_star_brightness)
                .before(crate::starfield::sync_starfield_params),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::run_frames;

    #[test]
    fn test_target_offset_only_darkens() {
        assert_eq!(target_offset(0.5), 0.0);
        assert_eq!(target_offset(TARGET_PEAK), 0.0);
        assert!((target_offset(TARGET_PEAK * 1024.0) + 10.0).abs() < 1e-4);
        assert_eq!(target_offset(f32::INFINITY), MIN_AUTO_OFFSET);
    }

    #[test]
    fn test_adaptation_darkens_fast_and_brightens_slowly() {
        let mut x = 0.0;
        for _ in 0..30 {
            x = adapt(x, -20.0, 1.0 / 60.0);
        }
        assert!(x < -17.0, "half a second of darkening: {x}");
        let mut y = -20.0;
        for _ in 0..30 {
            y = adapt(y, 0.0, 1.0 / 60.0);
        }
        assert!(y < -7.0 && y > -15.0, "half a second of brightening: {y}");
        // Converges exactly, so the exposure stops changing.
        let mut z = -1.0;
        for _ in 0..600 {
            z = adapt(z, 0.0, 1.0 / 60.0);
        }
        assert_eq!(z, 0.0);
    }

    #[test]
    fn test_half_diagonal_fov() {
        let h = half_diagonal_fov(90f32.to_radians(), 1.0);
        assert!((h.to_degrees() - 54.7356).abs() < 1e-3);
    }

    fn sun_app() -> (App, Entity) {
        let mut app = crate::testing::headless_app();
        app.init_resource::<ExposureSettings>()
            .init_resource::<CameraFov>()
            .add_systems(Update, auto_exposure);
        app.world_mut().spawn((
            WorldPos(DVec3::ZERO),
            Star {
                spectral: 'G',
                radius_solar: 1.0,
                visual_lum: 1.0,
                last_brightness: 0.0,
            },
        ));
        // At 1 AU on +Z, looking at the Sun (Bevy cameras look down -Z).
        let cam = app
            .world_mut()
            .spawn((
                FlyCam::default(),
                WorldPos(DVec3::new(0.0, 0.0, 1.0)),
                Transform::default(),
            ))
            .id();
        (app, cam)
    }

    #[test]
    fn test_looking_at_the_sun_darkens_and_looking_away_recovers() {
        let (mut app, cam) = sun_app();
        // Two seconds: eight darkening time constants.
        run_frames(&mut app, 120);
        let offset = app.world().resource::<ExposureSettings>().auto_offset;
        // The Sun from 1 AU is ~3e9 at EV 0: about 30 stops too bright.
        assert!((-32.0..-29.0).contains(&offset), "{offset}");
        // Its exposed brightness lands on the target.
        let e = app.world().resource::<ExposureSettings>().factor();
        let b = raw_brightness(
            e,
            1.0,
            SOLAR_RADIUS_UNITS,
            1.0,
            min_star_angle(CameraFov::default().0),
        );
        assert!((b / TARGET_PEAK - 1.0).abs() < 0.02, "{b}");

        app.world_mut().get_mut::<Transform>(cam).unwrap().rotation =
            Quat::from_rotation_y(std::f32::consts::PI);
        run_frames(&mut app, 600);
        assert_eq!(app.world().resource::<ExposureSettings>().auto_offset, 0.0);
    }

    #[test]
    fn test_auto_off_leaves_the_manual_exposure() {
        let (mut app, _) = sun_app();
        app.world_mut().resource_mut::<ExposureSettings>().auto = false;
        run_frames(&mut app, 60);
        let e = app.world().resource::<ExposureSettings>();
        assert_eq!(e.factor(), 1.0);
    }
}
