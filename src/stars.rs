use crate::camera::FlyCam;
use crate::constants::{AU_PER_PC, KM_PER_AU};
use crate::coords::{BodyRadius, WorldPos};
use crate::hyg::HygCatalog;
use crate::hyg_bodies::HygEntities;
use crate::info::{BodyInfo, BodyKind, star_facts};
use crate::labels::{LabelLum, spawn_label};
use crate::orbit::{equatorial_to_world, radec_to_equatorial};
use crate::search::{ObjectCatalog, Search};
use crate::zoom::{CameraFov, min_star_angle};
use bevy::math::DVec3;
use bevy::prelude::*;

// Correct solar radius: 695700 km / 149597870.7 km·AU⁻¹ = 0.004650 AU (world units are AU)
pub const SOLAR_RADIUS_UNITS: f64 = 695_700.0 / KM_PER_AU;

// Physical scale: emissive = 2^ev * BRIGHTNESS_SCALE * visual_luminosity / dist_pc²
// Calibrated so Vega (visual L≈50 from HYG, d=7.68 pc) ≈ 0.34 at EV 0 (bright but not blown out)
pub const BRIGHTNESS_SCALE: f32 = 0.4;

// Ceiling on a star's emissive brightness. Point-source 1/d^2 brightness reaches ~1e10 within
// a few AU of a star; with bloom that whites out the whole screen on arrival. Capped, a close
// star reads as a bright disk with a modest glow. (Proper fix: auto-exposure, see ROADMAP.)
pub const MAX_STAR_BRIGHTNESS: f32 = 30.0;
/// Distance clamp for the 1/d^2 brightness law, parsecs.
pub const MIN_BRIGHTNESS_DIST_PC: f32 = 1e-4;

pub fn star_brightness(ev_factor: f32, luminosity_solar: f32, dist_pc: f32) -> f32 {
    let dist_pc = dist_pc.max(MIN_BRIGHTNESS_DIST_PC);
    (ev_factor * BRIGHTNESS_SCALE * luminosity_solar / (dist_pc * dist_pc)).min(MAX_STAR_BRIGHTNESS)
}

// Stars are scaled up dynamically so their angular diameter is at least this many radians.
// ~0.002 rad ≈ 0.11° ≈ 2-3 pixels at a typical FOV, ensuring sub-pixel stars are always visible.
pub const MIN_STAR_ANGLE: f32 = 0.002;

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
pub struct Star {
    pub spectral: char,
    pub radius_solar: f32,
    /// Visual luminosity (from HYG absolute magnitude), used for rendered brightness so these
    /// stars match the starfield. The bolometric value in CATALOG_STARS feeds the info panel.
    pub visual_lum: f32,
    /// Brightness last written to the material; skips GPU re-uploads for changes < 1%.
    pub last_brightness: f32,
}

/// For each curated star (CATALOG_STARS order), its index in the HYG catalog. Matched stars
/// are left out of the starfield (no double-drawing) and take their visual luminosity from HYG.
#[derive(Resource, Default)]
pub struct CuratedHygMatches(pub Vec<Option<usize>>);

/// Curated stars match the brightest HYG star within this angle that's brighter than
/// MATCH_MAX_MAG as seen from the Sun.
const MATCH_MAX_SEP_DEG: f64 = 0.3;
const MATCH_MAX_MAG: f64 = 4.5;

pub fn match_curated(catalog: &HygCatalog) -> Vec<Option<usize>> {
    let bright = catalog.bright_indices(MATCH_MAX_MAG);
    CATALOG_STARS
        .iter()
        .map(|&(_, ra, dec, dist, ..)| {
            let dir = star_world_pos(ra, dec, dist).normalize();
            catalog.match_direction(&bright, dir, MATCH_MAX_SEP_DEG)
        })
        .collect()
}

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
    equatorial_to_world(radec_to_equatorial(ra_hours * 15.0, dec_deg)) * dist_pc * AU_PER_PC
}

pub fn setup_stars(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut catalog: ResMut<ObjectCatalog>,
    hyg: Res<HygCatalog>,
    mut hyg_entities: Option<ResMut<HygEntities>>,
) {
    let matches = match_curated(&hyg);
    for (k, &(name, ra, dec, dist, spect, radius_solar, luminosity)) in
        CATALOG_STARS.iter().enumerate()
    {
        // Unmatched stars fall back to their bolometric luminosity.
        let visual_lum = matches[k].map_or(luminosity, |i| hyg.stars[i].lum);
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
                    spectral: spect,
                    radius_solar,
                    visual_lum,
                    last_brightness: 0.0,
                },
            ))
            .id();

        commands.entity(star_entity).insert(LabelLum(visual_lum));
        // The curated sphere is this catalog star's entity (star browser, picking).
        if let (Some(i), Some(map)) = (matches[k], hyg_entities.as_mut()) {
            map.0.insert(i, star_entity);
        }
        spawn_label(&mut commands, star_entity, name);

        catalog.0.push((name.to_string(), star_entity));
    }

    commands.insert_resource(CuratedHygMatches(matches));
}

/// Recomputes star brightness and size from the camera distance. Runs only when the camera
/// moved or the exposure or FOV changed.
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
    fov: Res<CameraFov>,
) {
    let Ok(cam) = cam_query.single() else { return };
    if !cam.is_changed() && !exposure.is_changed() && !fov.is_changed() {
        return;
    }
    let ev_factor = exposure.factor();
    let min_angle = min_star_angle(fov.0);

    for (pos, mut transform, mat_handle, mut star) in star_query.iter_mut() {
        let dist_au = pos.0.distance(cam.0);
        let brightness = star_brightness(ev_factor, star.visual_lum, (dist_au / AU_PER_PC) as f32);
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
        let visual_radius = star_visual_radius(physical_radius, dist_au as f32, min_angle);
        let scale = Vec3::splat(visual_radius / physical_radius);
        if transform.scale != scale {
            transform.scale = scale;
        }
    }
}

/// Returns the visual sphere radius that ensures a star subtends at least
/// `min_angle` (`zoom::min_star_angle`, MIN_STAR_ANGLE at the default FOV) regardless of distance, while preserving physical size up close.
pub fn star_visual_radius(physical_radius: f32, cam_dist_units: f32, min_angle: f32) -> f32 {
    physical_radius.max(cam_dist_units * min_angle)
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
        // RA 6h is world -Z: world = (x_eq, z_eq, -y_eq), a rotation (not a mirror).
        assert!(pos.x.abs() < 0.01, "x should be ~0, got {}", pos.x);
        assert!(pos.y.abs() < 0.01, "y should be ~0, got {}", pos.y);
        assert!(
            (pos.z + AU_PER_PC).abs() < 0.01,
            "z should be -AU_PER_PC, got {}",
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
    fn test_sky_is_not_mirrored() {
        // Facing RA 6h on the celestial equator with north up, RA 0h (to the west) must be
        // on the right, as on the real sky. A mirrored world mapping puts it on the left.
        let toward_6h = star_world_pos(6.0, 0.0, 1.0).normalize().as_vec3();
        let (_, _, rot) = crate::goto::look_rotation(toward_6h);
        let right = rot * Vec3::X;
        let toward_0h = star_world_pos(0.0, 0.0, 1.0).normalize().as_vec3();
        assert!(right.dot(toward_0h) > 0.99, "RA 0h should be to the right");
    }

    #[test]
    fn test_orion_belt_order_from_earth() {
        // Looking at Orion's belt with north up: Mintaka (west) right, Alnitak (east) left.
        let mintaka = star_world_pos(5.5337, -0.2991, 380.0).normalize().as_vec3();
        let alnitak = star_world_pos(5.6795, -1.9425, 387.0).normalize().as_vec3();
        let (_, _, rot) = crate::goto::look_rotation((mintaka + alnitak).normalize());
        let right = rot * Vec3::X;
        assert!(mintaka.dot(right) > alnitak.dot(right));
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
        let visual = star_visual_radius(physical, dist, MIN_STAR_ANGLE);
        assert_eq!(visual, physical, "physical size should dominate when close");
    }

    #[test]
    fn test_visual_radius_scales_up_for_distant_star() {
        // Sirius: physical radius ≈ 1.711 × 0.00465 AU ≈ 0.00796 AU, distance ≈ 543,920 AU
        let physical: f32 = 1.711 * SOLAR_RADIUS_UNITS as f32;
        let dist: f32 = (2.637 * AU_PER_PC) as f32;
        let visual = star_visual_radius(physical, dist, MIN_STAR_ANGLE);
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
        let visual = star_visual_radius(physical, dist, MIN_STAR_ANGLE);
        let angle = visual / dist;
        assert!(
            angle >= MIN_STAR_ANGLE,
            "Deneb should meet minimum angle at full distance"
        );
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
        // Vega from Earth keeps its calibrated value (~0.34 at EV 0), using its visual
        // luminosity from HYG as the renderer does.
        let hyg = HygCatalog::embedded();
        let vega_lum = hyg.stars[hyg.index_of_name("Vega").unwrap()].lum;
        let vega = star_brightness(1.0, vega_lum, 7.68);
        assert!(
            (vega - 0.34).abs() < 0.02,
            "vega {vega} (visual L {vega_lum})"
        );
    }

    #[test]
    fn test_every_curated_star_matches_a_distinct_hyg_star() {
        // Unmatched curated stars would be drawn twice (sphere + starfield point).
        let hyg = HygCatalog::embedded();
        let matches = match_curated(&hyg);
        let unmatched: Vec<&str> = CATALOG_STARS
            .iter()
            .zip(&matches)
            .filter(|(_, m)| m.is_none())
            .map(|(s, _)| s.0)
            .collect();
        assert!(unmatched.is_empty(), "unmatched: {unmatched:?}");
        let unique: std::collections::HashSet<_> = matches.iter().flatten().collect();
        assert_eq!(unique.len(), CATALOG_STARS.len());
        // Spot-check: Sirius matched the HYG star named Sirius.
        assert_eq!(matches[0], hyg.index_of_name("Sirius"));
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
