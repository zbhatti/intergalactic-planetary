//! Sky overlays: coordinate grids (equatorial, ecliptic, galactic) and the 88 IAU constellation
//! figures.
//!
//! Everything is drawn as gizmo lines on a sphere [`SKY_RADIUS_AU`] around the camera, so every
//! planet and moon (and any star sphere big enough to see) is in front of them. Don't use
//! `depth_bias = 1`: in Bevy's reverse-Z that puts lines at depth 0, which fails the depth test
//! against the cleared buffer and draws nothing. Grids are directions only, so they look
//! the same wherever you are. Constellation figures join the member stars' real 3D positions as
//! seen from the camera: from the solar system they're the familiar shapes, and as you fly away
//! they distort, because the stars are at different distances.
//!
//! Figure data: Stellarium's "modern" sky culture (CC BY-SA 4.0), see `tools/build_constellations.py`.

use crate::camera::FlyCam;
use crate::coords::{NavSet, WorldPos};
use crate::hyg::HygCatalog;
use crate::info::{BodyInfo, BodyKind};
use crate::labels::spawn_label_colored;
use crate::menu::MenuOpen;
use crate::orbit::{ecliptic_to_world, equatorial_to_world, radec_to_equatorial};
use crate::search::Search;
use bevy::math::{DMat3, DVec3};
use bevy::prelude::*;
use std::collections::HashMap;

static CONSTELLATIONS_TXT: &str = include_str!("../data/constellations.txt");

/// Radius (AU) of the sphere the lines are drawn on, around the camera's render position. Large
/// compared with the camera's distance from the floating origin (<= 1 AU), so f32 rounding
/// doesn't bend the lines, and beyond anything big enough to cover them (solar-system bodies,
/// a close star's sphere).
const SKY_RADIUS_AU: f32 = 1000.0;

#[derive(Resource, Debug, Clone, PartialEq, Default)]
pub struct SkySettings {
    pub equatorial_grid: bool,
    pub ecliptic_grid: bool,
    pub galactic_grid: bool,
    /// Figures and names (C key).
    pub constellations: bool,
}

/// A sky coordinate frame: its basis vectors in world space.
#[derive(Clone, Copy, Debug)]
pub struct SkyFrame(pub DMat3);

impl SkyFrame {
    pub fn equatorial() -> Self {
        Self(DMat3::from_cols(
            equatorial_to_world(DVec3::X),
            equatorial_to_world(DVec3::Y),
            equatorial_to_world(DVec3::Z),
        ))
    }

    pub fn ecliptic() -> Self {
        Self(DMat3::from_cols(
            ecliptic_to_world(DVec3::X),
            ecliptic_to_world(DVec3::Y),
            ecliptic_to_world(DVec3::Z),
        ))
    }

    /// Galactic (IAU 1958, J2000 values): +X toward the Galactic center (l = 0, b = 0),
    /// +Z toward the north Galactic pole.
    pub fn galactic() -> Self {
        let ngp = radec_to_equatorial(192.859_48, 27.128_25);
        let gc = radec_to_equatorial(266.404_99, -28.936_17);
        // Re-orthogonalize: the published directions are rounded.
        let x = (gc - ngp * gc.dot(ngp)).normalize();
        let y = ngp.cross(x);
        Self(DMat3::from_cols(
            equatorial_to_world(x),
            equatorial_to_world(y),
            equatorial_to_world(ngp),
        ))
    }

    /// World unit vector for longitude/latitude (degrees) in this frame.
    pub fn dir(&self, lon_deg: f64, lat_deg: f64) -> DVec3 {
        self.0 * radec_to_equatorial(lon_deg, lat_deg)
    }
}

pub struct Constellation {
    pub abbr: String,
    pub name: String,
    pub english: String,
    /// Line segments as HYG catalog indices.
    pub segments: Vec<(usize, usize)>,
}

#[derive(Resource, Default)]
pub struct Constellations(pub Vec<Constellation>);

/// Parses `data/constellations.txt`, resolving Hipparcos numbers against `catalog`.
pub fn parse_constellations(
    text: &str,
    catalog: &HygCatalog,
) -> Result<Vec<Constellation>, String> {
    let by_hip: HashMap<u32, usize> = catalog
        .stars
        .iter()
        .enumerate()
        .filter(|(_, s)| s.hip != 0)
        .map(|(i, s)| (s.hip, i))
        .collect();
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            let [abbr, name, english, polys] = f[..] else {
                return Err(format!("bad line: {line}"));
            };
            let mut segments = Vec::new();
            for poly in polys.split(';') {
                let idx = poly
                    .split_whitespace()
                    .map(|h| {
                        let hip: u32 = h.parse().map_err(|_| format!("{abbr}: bad HIP {h}"))?;
                        by_hip
                            .get(&hip)
                            .copied()
                            .ok_or(format!("{abbr}: HIP {hip} not in the star catalog"))
                    })
                    .collect::<Result<Vec<usize>, String>>()?;
                segments.extend(idx.windows(2).map(|w| (w[0], w[1])));
            }
            Ok(Constellation {
                abbr: abbr.into(),
                name: name.into(),
                english: english.into(),
                segments,
            })
        })
        .collect()
}

/// Where a constellation's name goes: the mean direction of its stars as seen from the Sun,
/// at their median distance (so it stays among them when you fly around).
pub fn label_position(c: &Constellation, catalog: &HygCatalog) -> DVec3 {
    let mut members: Vec<usize> = c.segments.iter().flat_map(|&(a, b)| [a, b]).collect();
    members.sort_unstable();
    members.dedup();
    let dir: DVec3 = members
        .iter()
        .map(|&i| catalog.stars[i].pos.normalize())
        .sum();
    let mut dists: Vec<f64> = members
        .iter()
        .map(|&i| catalog.stars[i].pos.length())
        .collect();
    dists.sort_by(f64::total_cmp);
    dir.normalize() * dists[dists.len() / 2]
}

/// Marks a constellation-name entity.
#[derive(Component)]
pub struct ConstellationName;

pub fn setup_constellations(mut commands: Commands, catalog: Res<HygCatalog>) {
    let list = parse_constellations(CONSTELLATIONS_TXT, &catalog)
        .expect("embedded constellation data resolves (checked by tests)");
    for c in &list {
        let e = commands
            .spawn((
                ConstellationName,
                WorldPos(label_position(c, &catalog)),
                BodyInfo {
                    name: c.name.clone(),
                    kind: BodyKind::Constellation,
                    facts: vec![
                        ("English name", c.english.clone()),
                        ("IAU abbreviation", c.abbr.clone()),
                    ],
                },
            ))
            .id();
        spawn_label_colored(&mut commands, e, &c.name, Color::srgba(0.45, 0.7, 0.9, 0.8));
    }
    commands.insert_resource(Constellations(list));
}

/// Sky-sphere gizmos (thin lines).
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct SkyGizmos;

const EQUATORIAL_COLOR: Color = Color::srgba(0.3, 0.5, 0.9, 0.35);
const ECLIPTIC_COLOR: Color = Color::srgba(0.9, 0.75, 0.3, 0.35);
const GALACTIC_COLOR: Color = Color::srgba(0.75, 0.4, 0.9, 0.35);
const CONSTELLATION_COLOR: Color = Color::srgba(0.3, 0.6, 0.8, 0.45);

/// Grid polylines for a frame: meridians every `lon_step` degrees and parallels every
/// `lat_step`, as lists of world directions. The frame's equator comes first.
pub fn grid_lines(frame: SkyFrame, lon_step: f64, lat_step: f64) -> Vec<Vec<DVec3>> {
    const SEG: f64 = 5.0;
    let mut lines = Vec::new();
    let mut lats = vec![0.0];
    let mut lat = lat_step;
    while lat < 90.0 - 1e-9 {
        lats.extend([lat, -lat]);
        lat += lat_step;
    }
    for lat in lats {
        let n = (360.0 / SEG) as usize;
        lines.push((0..=n).map(|k| frame.dir(k as f64 * SEG, lat)).collect());
    }
    let mut lon = 0.0;
    while lon < 360.0 - 1e-9 {
        let n = (180.0 / SEG) as usize;
        lines.push(
            (0..=n)
                .map(|k| frame.dir(lon, -90.0 + k as f64 * SEG))
                .collect(),
        );
        lon += lon_step;
    }
    lines
}

#[derive(Resource)]
pub struct GridCache {
    equatorial: Vec<Vec<DVec3>>,
    ecliptic: Vec<Vec<DVec3>>,
    galactic: Vec<Vec<DVec3>>,
}

impl Default for GridCache {
    fn default() -> Self {
        Self {
            // RA in hours: a meridian every 1h.
            equatorial: grid_lines(SkyFrame::equatorial(), 15.0, 10.0),
            ecliptic: grid_lines(SkyFrame::ecliptic(), 30.0, 10.0),
            galactic: grid_lines(SkyFrame::galactic(), 30.0, 10.0),
        }
    }
}

pub fn draw_sky(
    settings: Res<SkySettings>,
    grids: Res<GridCache>,
    constellations: Res<Constellations>,
    catalog: Res<HygCatalog>,
    cam: Query<(&WorldPos, &Transform), With<FlyCam>>,
    mut gizmos: Gizmos<SkyGizmos>,
) {
    let Ok((cam_pos, cam_t)) = cam.single() else {
        return;
    };
    let center = cam_t.translation;
    let at = |dir: DVec3| center + dir.as_vec3() * SKY_RADIUS_AU;
    for (on, lines, color) in [
        (
            settings.equatorial_grid,
            &grids.equatorial,
            EQUATORIAL_COLOR,
        ),
        (settings.ecliptic_grid, &grids.ecliptic, ECLIPTIC_COLOR),
        (settings.galactic_grid, &grids.galactic, GALACTIC_COLOR),
    ] {
        if !on {
            continue;
        }
        for (k, line) in lines.iter().enumerate() {
            // The frame's equator is brighter.
            let c = if k == 0 { color.with_alpha(0.8) } else { color };
            gizmos.linestrip(line.iter().map(|&d| at(d)), c);
        }
    }
    if settings.constellations {
        let dir = |i: usize| (catalog.stars[i].pos - cam_pos.0).normalize();
        for c in &constellations.0 {
            for &(a, b) in &c.segments {
                gizmos.line(at(dir(a)), at(dir(b)), CONSTELLATION_COLOR);
            }
        }
    }
}

/// C toggles constellation figures and names.
pub fn toggle_constellations(
    keys: Res<ButtonInput<KeyCode>>,
    search: Res<Search>,
    menu: Res<MenuOpen>,
    mut settings: ResMut<SkySettings>,
) {
    if !search.active && !menu.0 && keys.just_pressed(KeyCode::KeyC) {
        settings.constellations = !settings.constellations;
    }
}

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SkySettings>()
            .init_resource::<GridCache>()
            .init_resource::<Constellations>()
            .insert_gizmo_config(
                SkyGizmos,
                GizmoConfig {
                    line: GizmoLineConfig {
                        width: 1.0,
                        ..default()
                    },
                    ..default()
                },
            )
            .add_systems(Startup, setup_constellations)
            .add_systems(Update, toggle_constellations)
            .add_systems(Update, draw_sky.in_set(NavSet::View));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_every_figure_star_resolves() {
        // build_hyg.py keeps naked-eye stars without a distance (e.g. Polis in Sagittarius)
        // precisely so that no constellation loses a line.
        let catalog = HygCatalog::embedded();
        let list = parse_constellations(CONSTELLATIONS_TXT, &catalog).unwrap();
        assert_eq!(list.len(), 88);
        let segments: usize = list.iter().map(|c| c.segments.len()).sum();
        assert_eq!(
            segments, 695,
            "matches data/constellations.txt.manifest.txt"
        );
        assert!(
            list.iter()
                .all(|c| c.name.is_ascii() && c.english.is_ascii())
        );
    }

    #[test]
    fn test_orion_figure_joins_its_stars() {
        let catalog = HygCatalog::embedded();
        let list = parse_constellations(CONSTELLATIONS_TXT, &catalog).unwrap();
        let ori = list.iter().find(|c| c.abbr == "Ori").unwrap();
        assert_eq!(ori.name, "Orion");
        let betelgeuse = catalog.index_of_name("Betelgeuse").unwrap();
        assert!(
            ori.segments
                .iter()
                .any(|&(a, b)| a == betelgeuse || b == betelgeuse)
        );
        // Its name sits among its stars, as seen from the Sun.
        let label = label_position(ori, &catalog).normalize();
        let sep = label
            .angle_between(catalog.stars[betelgeuse].pos)
            .to_degrees();
        assert!(sep < 15.0, "{sep}");
    }

    #[test]
    fn test_parse_rejects_unknown_stars() {
        let catalog = HygCatalog::embedded();
        let err = parse_constellations("Xxx\tX\tX\t1 999999999\n", &catalog)
            .err()
            .unwrap();
        assert!(err.contains("not in the star catalog"), "{err}");
    }

    #[test]
    fn test_frames() {
        let angle = |a: DVec3, b: DVec3| a.angle_between(b).to_degrees();
        // The ecliptic pole is 23.44 deg from the celestial pole (world +Y).
        let ecl = SkyFrame::ecliptic();
        assert!((angle(ecl.dir(0.0, 90.0), DVec3::Y) - 23.439).abs() < 0.01);
        // Equinox: the ecliptic and equator share longitude 0.
        assert!(angle(ecl.dir(0.0, 0.0), SkyFrame::equatorial().dir(0.0, 0.0)) < 1e-9);
        // The galactic north pole is at Dec +27.13, so 62.87 deg from the celestial pole, and
        // the frame is orthonormal and right-handed.
        let gal = SkyFrame::galactic();
        assert!((angle(gal.dir(0.0, 90.0), DVec3::Y) - 62.87).abs() < 0.01);
        assert!((gal.0.determinant() - 1.0).abs() < 1e-9);
        // The Galactic center is in Sagittarius: RA 17h45m, Dec -29.
        let gc = equatorial_to_world(radec_to_equatorial(266.405, -28.936));
        assert!(angle(gal.dir(0.0, 0.0), gc) < 0.01);
    }

    #[test]
    fn test_grid_lines_start_with_the_equator() {
        let lines = grid_lines(SkyFrame::equatorial(), 15.0, 10.0);
        // 17 parallels (0, +-10 .. +-80) and 24 meridians.
        assert_eq!(lines.len(), 17 + 24);
        assert!(
            lines[0].iter().all(|d| d.y.abs() < 1e-12),
            "equator is in the world XZ plane"
        );
    }
}
