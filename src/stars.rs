use crate::camera::FlyCam;
use crate::constants::{AU_PER_PC, KM_PER_AU};
use crate::coords::{BodyRadius, WorldPos, view_transform};
use crate::info::{BodyInfo, BodyKind, star_facts};
use crate::menu::{ShowBackgroundStars, ShowStarLabels};
use crate::search::{ObjectCatalog, Search};
use bevy::math::DVec3;
use bevy::prelude::*;
use rand::Rng;
use std::f64::consts::PI;

// Correct solar radius: 695700 km / 149597870.7 km·AU⁻¹ = 0.004650 AU (world units are AU)
pub const SOLAR_RADIUS_UNITS: f64 = 695_700.0 / KM_PER_AU;

// Physical scale: emissive = 2^ev * BRIGHTNESS_SCALE * luminosity / dist_pc²
// Calibrated so Vega (L=40.12, d=7.68 pc) ≈ 0.34 at EV 0 (bright but not blown out)
const BRIGHTNESS_SCALE: f32 = 0.5;

// Ceiling on a star's emissive brightness. Point-source 1/d^2 brightness reaches ~1e10 within
// a few AU of a star; with bloom that whites out the whole screen on arrival. Capped, a close
// star reads as a bright disk with a modest glow. (Proper fix: auto-exposure, see ROADMAP.)
const MAX_STAR_BRIGHTNESS: f32 = 30.0;

pub fn star_brightness(ev_factor: f32, luminosity_solar: f32, dist_pc: f32) -> f32 {
    let dist_pc = dist_pc.max(1e-4);
    (ev_factor * BRIGHTNESS_SCALE * luminosity_solar / (dist_pc * dist_pc)).min(MAX_STAR_BRIGHTNESS)
}

// Stars are scaled up dynamically so their angular diameter is at least this many radians.
// ~0.002 rad ≈ 0.11° ≈ 2-3 pixels at a typical FOV, ensuring sub-pixel stars are always visible.
const MIN_STAR_ANGLE: f32 = 0.002;

// (name, RA hours, Dec deg, dist pc, spectral, radius solar radii, luminosity solar)
const CATALOG_STARS: &[(&str, f64, f64, f64, char, f32, f32)] = &[
    ("Sirius", 6.7525, -16.7161, 2.637, 'A', 1.711, 25.4),
    ("Canopus", 6.3992, -52.6956, 95.88, 'F', 71.0, 10_700.0),
    ("Arcturus", 14.2612, 19.1822, 11.26, 'K', 25.4, 170.0),
    (
        "Rigil Kentaurus",
        14.6599,
        -60.8342,
        1.339,
        'G',
        1.227,
        1.519,
    ),
    ("Vega", 18.6157, 38.7836, 7.68, 'A', 2.362, 40.12),
    ("Capella", 5.2781, 45.9981, 12.94, 'G', 11.98, 78.7),
    ("Rigel", 5.2423, -8.2016, 264.5, 'B', 78.9, 120_000.0),
    ("Procyon", 7.6553, 5.2250, 3.509, 'F', 2.048, 6.93),
    ("Achernar", 1.6286, -57.2367, 44.1, 'B', 6.78, 3_150.0),
    ("Betelgeuse", 5.9195, 7.4069, 168.1, 'M', 887.0, 126_000.0),
    ("Hadar", 14.0637, -60.3730, 161.0, 'B', 9.0, 41_700.0),
    ("Altair", 19.8463, 8.8683, 5.130, 'A', 1.63, 10.6),
    ("Acrux", 12.4432, -63.0991, 98.7, 'B', 7.8, 25_000.0),
    ("Aldebaran", 4.5987, 16.5093, 19.96, 'K', 45.1, 518.0),
    ("Antares", 16.4901, -26.4320, 185.0, 'M', 700.0, 170_000.0),
    ("Spica", 13.4199, -11.1613, 77.0, 'B', 7.47, 20_500.0),
    ("Pollux", 7.7553, 28.0262, 10.34, 'K', 9.06, 32.0),
    ("Fomalhaut", 22.9608, -29.6224, 7.69, 'A', 1.842, 16.6),
    ("Deneb", 20.6905, 45.2803, 802.0, 'A', 203.0, 196_000.0),
    ("Mimosa", 12.7953, -59.6888, 85.0, 'B', 8.4, 34_000.0),
    ("Regulus", 10.1395, 11.9672, 23.8, 'B', 3.092, 363.0),
    ("Adhara", 6.9771, -28.9722, 132.4, 'B', 13.9, 38_700.0),
    ("Shaula", 17.5601, -37.1038, 216.0, 'B', 6.5, 35_000.0),
    ("Gacrux", 12.5193, -57.1132, 27.2, 'M', 84.0, 1_500.0),
    ("Bellatrix", 5.4184, 6.3497, 76.8, 'B', 5.75, 9_211.0),
    ("Elnath", 5.4381, 28.6074, 40.2, 'B', 4.2, 700.0),
    ("Miaplacidus", 9.2199, -69.7172, 34.5, 'A', 6.8, 182.0),
    ("Alnilam", 5.6036, -1.2019, 410.0, 'B', 42.0, 537_000.0),
    ("Alnitak", 5.6795, -1.9425, 387.0, 'O', 22.5, 250_000.0),
    ("Alioth", 12.9004, 55.9599, 24.9, 'A', 4.14, 108.0),
    ("Dubhe", 11.0621, 61.7511, 37.9, 'K', 17.0, 316.0),
    ("Mirfak", 3.4053, 49.8612, 161.0, 'F', 68.0, 5_000.0),
    ("Wezen", 7.1399, -26.3932, 506.0, 'F', 215.0, 170_000.0),
    ("Alkaid", 13.7923, 49.3133, 32.4, 'B', 3.4, 594.0),
    ("Sargas", 17.6215, -42.9978, 80.1, 'F', 26.0, 8_710.0),
    ("Menkent", 14.1114, -36.3699, 17.7, 'K', 10.6, 57.0),
    ("Atria", 16.8113, -69.0277, 92.4, 'K', 143.0, 5_500.0),
    ("Alhena", 6.6285, 16.3993, 30.2, 'A', 3.3, 123.0),
    ("Peacock", 20.4274, -56.7350, 56.2, 'B', 4.05, 2_200.0),
    ("Mirzam", 6.3783, -17.9559, 152.5, 'B', 9.7, 27_000.0),
    ("Alphard", 9.4598, -8.6586, 54.4, 'K', 50.5, 780.0),
    ("Polaris", 2.5303, 89.2641, 132.9, 'F', 46.0, 2_500.0),
    ("Hamal", 2.1199, 23.4624, 20.2, 'K', 14.9, 96.0),
    ("Nunki", 18.9211, -26.2967, 70.5, 'B', 4.8, 3_300.0),
    ("Saiph", 5.7959, -9.6697, 221.0, 'B', 22.2, 56_881.0),
    ("Kaus Australis", 18.4028, -34.3847, 44.5, 'B', 6.8, 363.0),
    ("Rasalhague", 17.5822, 12.5600, 14.86, 'A', 2.858, 24.7),
    ("Algol", 3.1362, 40.9556, 28.2, 'B', 2.73, 98.0),
    ("Denebola", 11.8177, 14.5721, 11.0, 'A', 1.728, 15.1),
    ("Mintaka", 5.5337, -0.2991, 380.0, 'O', 16.5, 90_000.0),
    ("Diphda", 0.7264, -17.9866, 29.4, 'K', 16.78, 139.0),
    ("Caph", 0.1527, 59.1497, 16.8, 'F', 3.43, 27.3),
    ("Avior", 8.3752, -59.5093, 165.0, 'K', 6.5, 10_900.0),
    ("Naos", 8.0594, -40.0032, 429.0, 'O', 16.5, 550_000.0),
    ("Almach", 2.0650, 42.3298, 97.0, 'K', 14.0, 2_000.0),
    ("Phecda", 11.8977, 53.6948, 25.6, 'A', 3.04, 68.0),
    ("Menkib", 3.9814, 35.7911, 344.0, 'O', 13.0, 263_000.0),
    ("Regor", 8.1591, -47.3368, 336.0, 'O', 6.0, 170_000.0),
];

#[derive(Component)]
pub struct BackgroundStar;

#[derive(Component)]
pub struct Star {
    pub luminosity_solar: f32,
    pub spectral: char,
    pub radius_solar: f32,
    /// Brightness last written to the material; skips GPU re-uploads for changes < 1%.
    pub last_brightness: f32,
}

/// Links a UI text label entity back to the catalog star it tracks.
#[derive(Component)]
pub struct StarLabelFor(pub Entity);

#[derive(Resource)]
pub struct BackgroundStarMaterial(pub Handle<StandardMaterial>);

#[derive(Resource, Default)]
pub struct ExposureSettings {
    pub ev: i32,
}

impl ExposureSettings {
    const MIN_EV: i32 = -5;
    const MAX_EV: i32 = 20;

    pub fn factor(&self) -> f32 {
        2f32.powi(self.ev)
    }

    pub fn label(&self) -> String {
        format!("EV{:+}", self.ev)
    }
}

pub fn spectral_color(class: char) -> LinearRgba {
    match class {
        'O' => LinearRgba::new(0.6, 0.8, 2.0, 1.0),
        'B' => LinearRgba::new(0.8, 0.9, 2.0, 1.0),
        'A' => LinearRgba::new(1.5, 1.5, 1.5, 1.0),
        'F' => LinearRgba::new(1.5, 1.5, 0.9, 1.0),
        'G' => LinearRgba::new(1.8, 1.4, 0.4, 1.0),
        'K' => LinearRgba::new(2.0, 0.8, 0.2, 1.0),
        'M' => LinearRgba::new(2.0, 0.2, 0.05, 1.0),
        _ => LinearRgba::new(1.5, 1.5, 1.5, 1.0),
    }
}

/// Equatorial RA/Dec/distance to heliocentric world position in AU.
pub fn star_world_pos(ra_hours: f64, dec_deg: f64, dist_pc: f64) -> DVec3 {
    let ra = ra_hours * 15.0 * PI / 180.0;
    let dec = dec_deg * PI / 180.0;
    let d = dist_pc * AU_PER_PC;
    DVec3::new(
        d * dec.cos() * ra.cos(),
        d * dec.sin(),
        d * dec.cos() * ra.sin(),
    )
}

pub fn setup_stars(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut catalog: ResMut<ObjectCatalog>,
) {
    for &(name, ra, dec, dist, spect, radius_solar, luminosity) in CATALOG_STARS {
        let pos = star_world_pos(ra, dec, dist);
        let radius = radius_solar as f64 * SOLAR_RADIUS_UNITS;
        let star_entity = commands
            .spawn((
                Mesh3d(meshes.add(Sphere::new(radius as f32))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    emissive: spectral_color(spect),
                    ..default()
                })),
                Transform::default(),
                WorldPos(pos),
                BodyRadius(radius),
                BodyInfo {
                    name: name.to_string(),
                    kind: BodyKind::Star,
                    facts: star_facts(spect, luminosity as f64, radius_solar as f64, dist),
                },
                Star {
                    luminosity_solar: luminosity,
                    spectral: spect,
                    radius_solar,
                    last_brightness: 0.0,
                },
            ))
            .id();

        commands.spawn((
            StarLabelFor(star_entity),
            Text::new(name),
            TextFont {
                font_size: 13.0,
                ..default()
            },
            TextColor(Color::srgba(1.0, 0.9, 0.7, 0.85)),
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            Visibility::Hidden,
        ));

        catalog.0.push((name.to_string(), star_entity));
    }

    let mut rng = rand::thread_rng();
    let bg_handle = materials.add(StandardMaterial {
        emissive: LinearRgba::new(0.07, 0.08, 0.10, 1.0),
        ..default()
    });
    // Unit sphere — each instance is scaled by its distance so it subtends MIN_STAR_ANGLE.
    let bg_mesh = meshes.add(Sphere::new(1.0));
    // Decorative filler in a flattened disk 3-80 pc out. No BodyRadius: not navigable.
    for _ in 0..2000 {
        let r = rng.gen_range(3.0..80.0) * AU_PER_PC;
        let theta = rng.gen_range(0.0..2.0 * PI);
        let y = rng.gen_range(-5.0..5.0) * AU_PER_PC;
        let pos = DVec3::new(r * theta.cos(), y, r * theta.sin());
        let visual_radius = pos.length() as f32 * MIN_STAR_ANGLE;
        commands.spawn((
            Mesh3d(bg_mesh.clone()),
            MeshMaterial3d(bg_handle.clone()),
            Transform::from_scale(Vec3::splat(visual_radius)),
            WorldPos(pos),
            BackgroundStar,
            Visibility::Hidden,
        ));
    }
    commands.insert_resource(BackgroundStarMaterial(bg_handle));
}

/// Recomputes star brightness and size from the camera distance. Runs only when the camera
/// moved or the exposure changed.
pub fn update_star_brightness(
    cam_query: Query<Ref<WorldPos>, With<FlyCam>>,
    mut star_query: Query<
        (
            &WorldPos,
            &mut Transform,
            &MeshMaterial3d<StandardMaterial>,
            &mut Star,
        ),
        Without<FlyCam>,
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
    exposure: Res<ExposureSettings>,
    bg_mat: Res<BackgroundStarMaterial>,
) {
    let Ok(cam) = cam_query.single() else { return };
    if !cam.is_changed() && !exposure.is_changed() {
        return;
    }
    let ev_factor = exposure.factor();

    for (pos, mut transform, mat_handle, mut star) in star_query.iter_mut() {
        let dist_au = pos.0.distance(cam.0);
        let brightness = star_brightness(
            ev_factor,
            star.luminosity_solar,
            (dist_au / AU_PER_PC) as f32,
        );
        if (brightness / star.last_brightness.max(f32::MIN_POSITIVE) - 1.0).abs() > 0.01 {
            star.last_brightness = brightness;
            let base = spectral_color(star.spectral);
            if let Some(mat) = materials.get_mut(&mat_handle.0) {
                mat.emissive = LinearRgba::new(
                    base.red * brightness,
                    base.green * brightness,
                    base.blue * brightness,
                    1.0,
                );
            }
        }

        // Scale the sphere so its angular diameter is always >= MIN_STAR_ANGLE.
        // When the camera is close, physical size takes over naturally.
        let physical_radius = (star.radius_solar as f64 * SOLAR_RADIUS_UNITS) as f32;
        let visual_radius = star_visual_radius(physical_radius, dist_au as f32);
        let scale = Vec3::splat(visual_radius / physical_radius);
        if transform.scale != scale {
            transform.scale = scale;
        }
    }

    // Background stars scale uniformly with exposure only.
    if exposure.is_changed()
        && let Some(mat) = materials.get_mut(&bg_mat.0)
    {
        let b = (ev_factor * 0.1).max(0.001);
        mat.emissive = LinearRgba::new(0.7 * b, 0.8 * b, 1.0 * b, 1.0);
    }
}

/// Returns the visual sphere radius that ensures a star subtends at least
/// MIN_STAR_ANGLE regardless of distance, while preserving physical size up close.
pub fn star_visual_radius(physical_radius: f32, cam_dist_units: f32) -> f32 {
    physical_radius.max(cam_dist_units * MIN_STAR_ANGLE)
}

pub fn update_star_labels(
    show: Res<ShowStarLabels>,
    cam_query: Query<(&Camera, &WorldPos, &Transform), With<FlyCam>>,
    positions: Query<&WorldPos>,
    mut label_query: Query<(&StarLabelFor, &mut Node, &mut Visibility)>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let Ok((camera, cam_pos, cam_t)) = cam_query.single() else {
        return;
    };
    let view = view_transform(cam_t.rotation);
    let vp = windows
        .single()
        .map(|w| Vec2::new(w.width(), w.height()))
        .unwrap_or(Vec2::new(1280.0, 720.0));

    for (label_for, mut node, mut vis) in label_query.iter_mut() {
        if !show.0 {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        }
        let screen = positions.get(label_for.0).ok().and_then(|pos| {
            camera
                .world_to_viewport(&view, (pos.0 - cam_pos.0).as_vec3())
                .ok()
                .filter(|sp| sp.x >= 0.0 && sp.x <= vp.x && sp.y >= 0.0 && sp.y <= vp.y)
        });
        match screen {
            Some(sp) => {
                vis.set_if_neq(Visibility::Visible);
                node.left = Val::Px(sp.x + 8.0);
                node.top = Val::Px(sp.y - 8.0);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

pub fn adjust_exposure(
    keys: Res<ButtonInput<KeyCode>>,
    search: Res<Search>,
    mut exposure: ResMut<ExposureSettings>,
) {
    if search.active {
        return;
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        exposure.ev = (exposure.ev - 1).max(ExposureSettings::MIN_EV);
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        exposure.ev = (exposure.ev + 1).min(ExposureSettings::MAX_EV);
    }
}

pub fn update_background_star_visibility(
    show: Res<ShowBackgroundStars>,
    mut query: Query<&mut Visibility, With<BackgroundStar>>,
) {
    if !show.is_changed() {
        return;
    }
    let vis = if show.0 {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut v in query.iter_mut() {
        *v = vis;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- coordinate conversion ---

    #[test]
    fn test_ra0_dec0_dist1_maps_to_x_parsec() {
        let pos = star_world_pos(0.0, 0.0, 1.0);
        assert!(
            (pos.x - AU_PER_PC).abs() < 0.01,
            "x should be AU_PER_PC, got {}",
            pos.x
        );
        assert!(pos.y.abs() < 0.01, "y should be ~0, got {}", pos.y);
        assert!(pos.z.abs() < 0.01, "z should be ~0, got {}", pos.z);
    }

    #[test]
    fn test_ra6h_dec0_dist1_maps_to_z_parsec() {
        let pos = star_world_pos(6.0, 0.0, 1.0);
        assert!(pos.x.abs() < 0.01, "x should be ~0, got {}", pos.x);
        assert!(pos.y.abs() < 0.01, "y should be ~0, got {}", pos.y);
        assert!(
            (pos.z - AU_PER_PC).abs() < 0.01,
            "z should be AU_PER_PC, got {}",
            pos.z
        );
    }

    #[test]
    fn test_north_pole_dec90_maps_to_y_parsec() {
        let pos = star_world_pos(0.0, 90.0, 1.0);
        assert!(pos.x.abs() < 0.01, "x should be ~0, got {}", pos.x);
        assert!(
            (pos.y - AU_PER_PC).abs() < 0.01,
            "y should be AU_PER_PC, got {}",
            pos.y
        );
        assert!(pos.z.abs() < 0.01, "z should be ~0, got {}", pos.z);
    }

    #[test]
    fn test_sirius_is_at_its_real_distance() {
        // Regression: stars used to be placed at 1000 AU/pc (206x too close), so the HUD
        // showed Sirius at 0.04 ly. It's 2.637 pc = 8.60 ly.
        let pos = star_world_pos(6.7525, -16.7161, 2.637);
        let ly = pos.length() / crate::constants::AU_PER_LY;
        assert!((ly - 8.60).abs() < 0.01, "Sirius at {ly} ly");
    }

    #[test]
    fn test_distance_scales_linearly() {
        let pos1 = star_world_pos(0.0, 0.0, 1.0);
        let pos2 = star_world_pos(0.0, 0.0, 2.0);
        assert!(
            (pos2.length() - 2.0 * pos1.length()).abs() < 0.01,
            "distance should scale linearly"
        );
    }

    // --- SOLAR_RADIUS_UNITS regression (bug: was divided by 206265, making stars 200k× too small) ---

    #[test]
    fn test_solar_radius_units_is_correct_au() {
        // 1 solar radius = 695700 km / 149597870.7 km·AU⁻¹ ≈ 0.004650 AU
        let expected: f64 = 695_700.0 / 149_597_870.7;
        assert!(
            (SOLAR_RADIUS_UNITS - expected).abs() < 1e-6,
            "SOLAR_RADIUS_UNITS should be ~0.004650 AU, got {}",
            SOLAR_RADIUS_UNITS
        );
    }

    #[test]
    fn test_solar_radius_units_not_divided_by_arcsec_factor() {
        // The old buggy formula included / 206_265.0 (arcseconds per radian),
        // shrinking all star spheres by a factor of ~206000.
        // (with the old 1000 units/pc scale it came from)
        let buggy = 695_700.0_f64 / 149_597_870.7 / 206_265.0 * 1000.0;
        assert!(
            (SOLAR_RADIUS_UNITS - buggy).abs() > 0.001,
            "SOLAR_RADIUS_UNITS must not be the parsec-formula value {:.2e}",
            buggy
        );
    }

    #[test]
    fn test_sun_subtends_half_degree_at_1au() {
        // At 1 AU the Sun's angular diameter should be ≈ 0.53° (0.5–0.6° range).
        let angle_deg = 2.0 * (SOLAR_RADIUS_UNITS / 1.0_f64).atan().to_degrees();
        assert!(angle_deg > 0.4, "Sun too small at 1 AU: {:.4}°", angle_deg);
        assert!(angle_deg < 0.7, "Sun too large at 1 AU: {:.4}°", angle_deg);
    }

    // --- dynamic angular size / sub-pixel star regression ---

    #[test]
    fn test_visual_radius_uses_physical_size_when_close() {
        let physical = 4.0; // AU – large star like Betelgeuse up close
        let dist = 0.5; // 0.5 AU away
        let visual = star_visual_radius(physical, dist);
        assert_eq!(visual, physical, "physical size should dominate when close");
    }

    #[test]
    fn test_visual_radius_scales_up_for_distant_star() {
        // Sirius: physical radius ≈ 1.711 × 0.00465 AU ≈ 0.00796 AU, distance ≈ 543,920 AU
        let physical: f32 = 1.711 * SOLAR_RADIUS_UNITS as f32;
        let dist: f32 = (2.637 * AU_PER_PC) as f32;
        let visual = star_visual_radius(physical, dist);
        assert!(
            visual > physical,
            "distant star should be scaled up; physical={:.5}, visual={:.5}",
            physical,
            visual
        );
        // Must meet minimum angle
        let angle = visual / dist;
        assert!(
            angle >= MIN_STAR_ANGLE,
            "angle {:.6} rad should be >= MIN_STAR_ANGLE {}",
            angle,
            MIN_STAR_ANGLE
        );
    }

    #[test]
    fn test_visual_radius_ensures_min_angle_for_deneb() {
        // Deneb: radius ≈ 203 solar radii, distance ≈ 802 pc ≈ 1.65e8 AU
        let physical: f32 = 203.0 * SOLAR_RADIUS_UNITS as f32;
        let dist: f32 = (802.0 * AU_PER_PC) as f32;
        let visual = star_visual_radius(physical, dist);
        let angle = visual / dist;
        assert!(
            angle >= MIN_STAR_ANGLE,
            "Deneb should meet minimum angle at full distance"
        );
    }

    #[test]
    fn test_background_star_radius_proportional_to_distance() {
        // Background stars are spawned with radius = r * MIN_STAR_ANGLE so they
        // always subtend the minimum angle when viewed from the origin.
        let r = 10_000.0_f32;
        let expected_angle = r * MIN_STAR_ANGLE / r;
        assert!((expected_angle - MIN_STAR_ANGLE).abs() < 1e-6);
    }

    // --- brightness / exposure ---

    #[test]
    fn test_brightness_scales_inverse_square() {
        let lum = 100.0;
        let b1 = BRIGHTNESS_SCALE * lum / (1.0f32 * 1.0);
        let b2 = BRIGHTNESS_SCALE * lum / (2.0f32 * 2.0);
        assert!(
            (b1 / b2 - 4.0).abs() < 0.001,
            "brightness should follow inverse-square law"
        );
    }

    #[test]
    fn test_star_brightness_is_capped_near_a_star() {
        // Sirius from 0.06 AU (~3e-7 pc) must not blow out the screen.
        assert_eq!(star_brightness(1.0, 25.4, 3.0e-7), MAX_STAR_BRIGHTNESS);
        // Vega from Earth keeps its calibrated value (~0.34 at EV 0).
        let vega = star_brightness(1.0, 40.12, 7.68);
        assert!((vega - 0.34).abs() < 0.01, "vega {vega}");
    }

    #[test]
    fn test_exposure_factor_doubles_per_ev() {
        let e0 = ExposureSettings { ev: 0 };
        let e1 = ExposureSettings { ev: 1 };
        assert!((e1.factor() / e0.factor() - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_exposure_label_format() {
        assert_eq!(ExposureSettings { ev: 4 }.label(), "EV+4");
        assert_eq!(ExposureSettings { ev: -2 }.label(), "EV-2");
        assert_eq!(ExposureSettings { ev: 0 }.label(), "EV+0");
    }

    #[test]
    fn test_exposure_default_is_ev_zero() {
        assert_eq!(ExposureSettings::default().ev, 0);
    }

    #[test]
    fn test_exposure_clamps_at_min() {
        let mut e = ExposureSettings {
            ev: ExposureSettings::MIN_EV,
        };
        e.ev = (e.ev - 1).max(ExposureSettings::MIN_EV);
        assert_eq!(
            e.ev,
            ExposureSettings::MIN_EV,
            "EV should not go below MIN_EV"
        );
    }

    #[test]
    fn test_exposure_clamps_at_max() {
        let mut e = ExposureSettings {
            ev: ExposureSettings::MAX_EV,
        };
        e.ev = (e.ev + 1).min(ExposureSettings::MAX_EV);
        assert_eq!(
            e.ev,
            ExposureSettings::MAX_EV,
            "EV should not exceed MAX_EV"
        );
    }
}
