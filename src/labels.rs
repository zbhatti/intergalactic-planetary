//! Name labels for stars, planets and moons, filtered and decluttered every frame.
//!
//! Each labelled object has one UI text entity (with [`LabelFor`]). Every frame the candidates
//! are projected to the screen and filtered:
//! - by type ([`LabelSettings`]: planets and dwarf planets, moons, stars; constellation names
//!   follow the figures in [`SkySettings`]);
//! - stars by apparent magnitude from the camera (brighter than `star_mag_limit`);
//! - an orbiting body only once it's [`MIN_PRIMARY_SEP_PX`] away from its primary on screen, so
//!   moons appear as you zoom in or approach and don't pile up on their planet from afar;
//! - hidden behind the resolved disk of a nearer body;
//! - then greedily decluttered by priority (selection, planets, stars by brightness, moons): a
//!   label is dropped if its box would overlap one already placed.
//!
//! Hidden labels use `Display::None` (skipped by layout), and node positions are written only when
//! they move, so idle labels cost no UI work.

use crate::camera::FlyCam;
use crate::constants::AU_PER_PC;
use crate::coords::{BodyRadius, NavSet, WorldPos};
use crate::info::{BodyInfo, BodyKind, disk_radius_px};
use crate::menu::MenuOpen;
use crate::search::Search;
use crate::sky::SkySettings;
use crate::solar_system::OrbitOf;
use crate::zoom::CameraFov;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub const LABEL_FONT_SIZE: f32 = 13.0;
/// Bevy's default font (Fira Mono) is monospaced: each glyph advances ~0.6 em.
const CHAR_WIDTH_PX: f32 = LABEL_FONT_SIZE * 0.6;
const LABEL_HEIGHT_PX: f32 = LABEL_FONT_SIZE + 3.0;
/// Label text starts this far right of and above the object.
const LABEL_OFFSET: Vec2 = Vec2::new(8.0, -8.0);
/// An orbiting body is labelled only this far (px) from its primary on screen.
pub const MIN_PRIMARY_SEP_PX: f32 = 14.0;
/// Bodies whose disk is at least this big (px radius) hide the labels of objects behind them.
const MIN_OCCLUDER_PX: f32 = 2.0;
/// Upper bound on labels shown at once.
pub const MAX_LABELS: usize = 80;

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct LabelSettings {
    /// Master switch (L key).
    pub enabled: bool,
    /// Planets and dwarf planets.
    pub planets: bool,
    pub moons: bool,
    pub stars: bool,
    /// Stars are labelled when brighter (smaller) than this apparent magnitude from the camera.
    pub star_mag_limit: f32,
}

impl Default for LabelSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            planets: true,
            moons: true,
            stars: true,
            star_mag_limit: 2.5,
        }
    }
}

impl LabelSettings {
    pub const MAG_LIMITS: [f32; 6] = [1.0, 2.0, 2.5, 3.5, 4.5, 6.0];

    /// Next magnitude limit in the menu's cycle.
    pub fn next_mag_limit(&self) -> f32 {
        let i = Self::MAG_LIMITS
            .iter()
            .position(|&m| m > self.star_mag_limit)
            .unwrap_or(0);
        Self::MAG_LIMITS[i]
    }

    pub fn shows(&self, kind: BodyKind) -> bool {
        self.enabled
            && match kind {
                BodyKind::Star => self.stars,
                BodyKind::Planet | BodyKind::DwarfPlanet => self.planets,
                BodyKind::Moon => self.moons,
                // Shown with the figures (SkySettings); see update_labels.
                BodyKind::Constellation => true,
            }
    }
}

/// On a label: the object it names, and its estimated width in pixels.
#[derive(Component)]
pub struct LabelFor {
    pub target: Entity,
    pub width_px: f32,
}

/// On a HUD panel: labels never overlap it while it's visible.
#[derive(Component, Default)]
pub struct LabelBlocker;

/// On a star object: visual luminosity (solar), for its magnitude from the camera.
#[derive(Component, Clone, Copy)]
pub struct LabelLum(pub f32);

/// Spawns the (initially hidden) label for `target`.
pub fn spawn_label(commands: &mut Commands, target: Entity, name: &str) -> Entity {
    spawn_label_colored(commands, target, name, Color::srgba(1.0, 0.9, 0.7, 0.85))
}

pub fn spawn_label_colored(
    commands: &mut Commands,
    target: Entity,
    name: &str,
    color: Color,
) -> Entity {
    commands
        .spawn((
            LabelFor {
                target,
                width_px: name.chars().count() as f32 * CHAR_WIDTH_PX,
            },
            Text::new(name),
            TextFont {
                font_size: LABEL_FONT_SIZE,
                ..default()
            },
            TextColor(color),
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                ..default()
            },
        ))
        .id()
}

/// Apparent visual magnitude of a star with visual luminosity `lum` (solar) at `dist_au`.
pub fn apparent_magnitude(lum: f32, dist_au: f64) -> f32 {
    let d_pc = (dist_au / AU_PER_PC).max(1e-9);
    (4.83 - 2.5 * (lum.max(1e-12) as f64).log10() + 5.0 * d_pc.log10() - 5.0) as f32
}

/// Screen position (px, origin top-left) of a point at `rel` from the camera, or `None` if it's
/// behind the camera. Same projection as Bevy's perspective camera (vertical `fov_y`, looking
/// down -Z), done in f64 so huge distances keep their direction.
pub fn project_to_screen(rel: DVec3, rotation: Quat, fov_y: f32, viewport: Vec2) -> Option<Vec2> {
    let dir = rel.normalize_or_zero();
    let v = rotation.as_dquat().inverse() * dir;
    if v.z >= -1e-9 {
        return None;
    }
    let f = 1.0 / (fov_y as f64 * 0.5).tan();
    let aspect = (viewport.x / viewport.y) as f64;
    let ndc_x = v.x * f / aspect / -v.z;
    let ndc_y = v.y * f / -v.z;
    Some(Vec2::new(
        ((ndc_x + 1.0) * 0.5 * viewport.x as f64) as f32,
        ((1.0 - ndc_y) * 0.5 * viewport.y as f64) as f32,
    ))
}

#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub label: Entity,
    pub anchor: Vec2,
    pub width_px: f32,
    /// Lower is more important: 0 selected, 1 planets, 2 stars, 3 moons, 4 constellations.
    pub tier: u8,
    /// Within a tier, lower first (magnitude for stars, distance otherwise).
    pub key: f32,
}

fn label_rect(c: &Candidate) -> Rect {
    let min = c.anchor + LABEL_OFFSET;
    Rect::from_corners(min, min + Vec2::new(c.width_px, LABEL_HEIGHT_PX))
}

/// Keeps the most important labels whose boxes don't overlap each other or `blocked` (screen
/// areas covered by HUD panels), up to `max`.
pub fn declutter(mut cands: Vec<Candidate>, blocked: &[Rect], max: usize) -> Vec<Candidate> {
    cands.sort_by(|a, b| a.tier.cmp(&b.tier).then(a.key.total_cmp(&b.key)));
    let mut placed: Vec<Candidate> = Vec::new();
    for c in cands {
        if placed.len() >= max {
            break;
        }
        let r = label_rect(&c);
        let free = |other: Rect| other.intersect(r).is_empty();
        if blocked.iter().all(|&b| free(b)) && placed.iter().all(|p| free(label_rect(p))) {
            placed.push(c);
        }
    }
    placed
}

/// A body's resolved disk on screen.
struct Occluder {
    entity: Entity,
    screen: Vec2,
    radius_px: f32,
    dist: f64,
}

fn tier_and_key(kind: BodyKind, selected: bool, mag: Option<f32>, dist: f64) -> (u8, f32) {
    if selected {
        return (0, 0.0);
    }
    match kind {
        BodyKind::Planet | BodyKind::DwarfPlanet => (1, dist as f32),
        BodyKind::Star => (2, mag.unwrap_or(0.0)),
        BodyKind::Moon => (3, dist as f32),
        BodyKind::Constellation => (4, 0.0),
    }
}

type ObjectQuery<'a> = (
    Entity,
    &'a WorldPos,
    &'a BodyInfo,
    Option<&'a BodyRadius>,
    Option<&'a LabelLum>,
    Option<&'a OrbitOf>,
);

#[allow(clippy::too_many_arguments)]
pub fn update_labels(
    settings: Res<LabelSettings>,
    sky: Res<SkySettings>,
    search: Res<Search>,
    fov: Res<CameraFov>,
    cam: Query<(&WorldPos, &Transform), With<FlyCam>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    objects: Query<ObjectQuery, Without<FlyCam>>,
    mut labels: Query<(Entity, &LabelFor, &mut Node)>,
    blockers: Query<(&ComputedNode, &GlobalTransform, &InheritedVisibility), With<LabelBlocker>>,
    mut cands: Local<Vec<Candidate>>,
) {
    let Ok((cam_pos, cam_t)) = cam.single() else {
        return;
    };
    let vp = windows
        .single()
        .map(|w| Vec2::new(w.width(), w.height()))
        .unwrap_or(Vec2::new(1280.0, 720.0));
    let project = |pos: DVec3| project_to_screen(pos - cam_pos.0, cam_t.rotation, fov.0, vp);
    let on_screen = |p: Vec2| p.x >= 0.0 && p.y >= 0.0 && p.x <= vp.x && p.y <= vp.y;

    cands.clear();
    if settings.enabled {
        let occluders: Vec<Occluder> = objects
            .iter()
            .filter_map(|(e, pos, _, radius, ..)| {
                let dist = pos.0.distance(cam_pos.0);
                let radius_px = disk_radius_px(radius?.0, dist, fov.0, vp.y);
                (radius_px >= MIN_OCCLUDER_PX).then_some(())?;
                Some(Occluder {
                    entity: e,
                    screen: project(pos.0)?,
                    radius_px,
                    dist,
                })
            })
            .collect();

        for (label, label_for, _) in &labels {
            let Ok((e, pos, info, _, lum, orbit)) = objects.get(label_for.target) else {
                continue;
            };
            let selected = search.target == Some(e);
            if !selected && !settings.shows(info.kind) {
                continue;
            }
            if info.kind == BodyKind::Constellation && !sky.constellations {
                continue;
            }
            let dist = pos.0.distance(cam_pos.0);
            let mag = lum.map(|l| apparent_magnitude(l.0, dist));
            if !selected && mag.is_some_and(|m| m > settings.star_mag_limit) {
                continue;
            }
            let Some(anchor) = project(pos.0).filter(|&p| on_screen(p)) else {
                continue;
            };
            if !selected
                && let Some(orbit) = orbit
                && let Ok((_, primary, ..)) = objects.get(orbit.primary)
                && project(primary.0).is_some_and(|p| p.distance(anchor) < MIN_PRIMARY_SEP_PX)
            {
                continue;
            }
            let hidden = occluders
                .iter()
                .any(|o| o.entity != e && o.dist < dist && o.screen.distance(anchor) < o.radius_px);
            if hidden {
                continue;
            }
            let (tier, key) = tier_and_key(info.kind, selected, mag, dist);
            cands.push(Candidate {
                label,
                anchor,
                width_px: label_for.width_px,
                tier,
                key,
            });
        }
    }
    // UI node transforms and sizes are in physical pixels; labels are placed in logical ones.
    let blocked: Vec<Rect> = blockers
        .iter()
        .filter(|(node, _, vis)| vis.get() && node.size() != Vec2::ZERO)
        .map(|(node, t, _)| {
            let s = node.inverse_scale_factor();
            Rect::from_center_size(t.translation().truncate() * s, node.size() * s)
        })
        .collect();
    let placed = declutter(std::mem::take(&mut *cands), &blocked, MAX_LABELS);

    for (label, _, mut node) in &mut labels {
        match placed.iter().find(|c| c.label == label) {
            Some(c) => {
                let at = c.anchor + LABEL_OFFSET;
                if node.display != Display::Flex {
                    node.display = Display::Flex;
                }
                if node.left != Val::Px(at.x) || node.top != Val::Px(at.y) {
                    node.left = Val::Px(at.x);
                    node.top = Val::Px(at.y);
                }
            }
            None => {
                if node.display != Display::None {
                    node.display = Display::None;
                }
            }
        }
    }
    *cands = placed;
}

/// L toggles all labels.
pub fn toggle_labels(
    keys: Res<ButtonInput<KeyCode>>,
    search: Res<Search>,
    menu: Res<MenuOpen>,
    mut settings: ResMut<LabelSettings>,
) {
    if !search.active && !menu.0 && keys.just_pressed(KeyCode::KeyL) {
        settings.enabled = !settings.enabled;
    }
}

pub struct LabelsPlugin;

impl Plugin for LabelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LabelSettings>()
            .add_systems(Update, toggle_labels)
            .add_systems(Update, update_labels.in_set(NavSet::View));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VP: Vec2 = Vec2::new(1280.0, 720.0);

    #[test]
    fn test_projection_matches_a_perspective_camera() {
        let fov = std::f32::consts::FRAC_PI_4;
        let ahead = project_to_screen(DVec3::new(0.0, 0.0, -5.0), Quat::IDENTITY, fov, VP);
        assert_eq!(ahead, Some(VP * 0.5));
        assert_eq!(
            project_to_screen(DVec3::new(0.0, 0.0, 5.0), Quat::IDENTITY, fov, VP),
            None
        );
        // At half the vertical FOV above the view axis: the top edge.
        let up = DVec3::new(0.0, (fov as f64 * 0.5).tan(), -1.0);
        let p = project_to_screen(up, Quat::IDENTITY, fov, VP).unwrap();
        assert!(
            (p.y - 0.0).abs() < 1e-3 && (p.x - 640.0).abs() < 1e-3,
            "{p}"
        );
        // Right is +X; the horizontal extent follows the aspect ratio.
        let right = DVec3::new((fov as f64 * 0.5).tan() * 1280.0 / 720.0, 0.0, -1.0);
        let p = project_to_screen(right, Quat::IDENTITY, fov, VP).unwrap();
        assert!((p.x - 1280.0).abs() < 1e-2, "{p}");
        // Huge distances keep their direction.
        let far = project_to_screen(DVec3::new(0.0, 0.0, -1e14), Quat::IDENTITY, fov, VP);
        assert_eq!(far, Some(VP * 0.5));
        // Turning the camera left 90 deg puts -X straight ahead.
        let rot = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let p = project_to_screen(DVec3::new(-3.0, 0.0, 0.0), rot, fov, VP).unwrap();
        assert!(p.distance(VP * 0.5) < 1e-2, "{p}");
    }

    #[test]
    fn test_apparent_magnitude() {
        // The Sun at 10 pc is its absolute magnitude.
        let m = apparent_magnitude(1.0, 10.0 * AU_PER_PC);
        assert!((m - 4.83).abs() < 1e-4, "{m}");
        // From Earth it's about -26.7.
        let m = apparent_magnitude(1.0, 1.0);
        assert!((m + 26.74).abs() < 0.05, "{m}");
    }

    fn cand(i: u32, x: f32, tier: u8, key: f32) -> Candidate {
        Candidate {
            label: Entity::from_raw(i),
            anchor: Vec2::new(x, 100.0),
            width_px: 50.0,
            tier,
            key,
        }
    }

    #[test]
    fn test_declutter_keeps_the_more_important_of_overlapping_labels() {
        let placed = declutter(
            vec![
                cand(1, 100.0, 2, 3.0), // dim star
                cand(2, 110.0, 2, 0.5), // bright star, overlaps 1
                cand(3, 120.0, 1, 5.0), // planet, overlaps both
                cand(4, 400.0, 3, 1.0), // moon, far away from the rest
            ],
            &[],
            10,
        );
        let ids: Vec<u32> = placed.iter().map(|c| c.label.index()).collect();
        assert_eq!(ids, vec![3, 4]);
        let placed = declutter(
            vec![cand(1, 100.0, 2, 3.0), cand(2, 110.0, 2, 0.5)],
            &[],
            10,
        );
        assert_eq!(placed[0].label.index(), 2);
        assert_eq!(placed.len(), 1);
    }

    #[test]
    fn test_declutter_respects_the_cap() {
        let many: Vec<_> = (0..100)
            .map(|i| cand(i, i as f32 * 100.0, 2, 0.0))
            .collect();
        assert_eq!(declutter(many, &[], 7).len(), 7);
    }

    #[test]
    fn test_labels_avoid_hud_panels() {
        let panel = Rect::new(90.0, 80.0, 300.0, 200.0);
        let placed = declutter(
            vec![cand(1, 100.0, 1, 0.0), cand(2, 600.0, 2, 0.0)],
            &[panel],
            10,
        );
        let ids: Vec<u32> = placed.iter().map(|c| c.label.index()).collect();
        assert_eq!(ids, vec![2]);
    }

    #[test]
    fn test_mag_limit_cycle_wraps() {
        let mut s = LabelSettings::default();
        let mut seen = vec![s.star_mag_limit];
        for _ in 0..LabelSettings::MAG_LIMITS.len() {
            s.star_mag_limit = s.next_mag_limit();
            seen.push(s.star_mag_limit);
        }
        assert_eq!(seen.first(), seen.last());
        assert!(seen.contains(&6.0) && seen.contains(&1.0));
    }

    struct Scene {
        app: App,
        cam: Entity,
        earth_label: Entity,
        moon_label: Entity,
        star_label: Entity,
    }

    fn body(world: &mut World, name: &str, kind: BodyKind, pos: DVec3, radius: f64) -> Entity {
        world
            .spawn((
                WorldPos(pos),
                BodyRadius(radius),
                BodyInfo {
                    name: name.into(),
                    kind,
                    facts: vec![],
                },
            ))
            .id()
    }

    fn label(world: &mut World, target: Entity, name: &str) -> Entity {
        let mut commands = world.commands();
        let e = spawn_label(&mut commands, target, name);
        world.flush();
        e
    }

    /// Earth at 1 AU on +X, the Moon 0.00257 AU beyond it, and a bright star far behind them.
    fn scene() -> Scene {
        let mut app = crate::testing::headless_app();
        app.init_resource::<LabelSettings>()
            .init_resource::<SkySettings>()
            .init_resource::<Search>()
            .init_resource::<CameraFov>()
            .add_systems(Update, update_labels);
        let world = app.world_mut();
        let earth_pos = DVec3::new(1.0, 0.0, 0.0);
        let earth = body(world, "Earth", BodyKind::Planet, earth_pos, 4.26e-5);
        let moon = body(
            world,
            "Moon",
            BodyKind::Moon,
            earth_pos + DVec3::new(0.0, 0.0, -0.00257),
            1.16e-5,
        );
        world.entity_mut(moon).insert(OrbitOf {
            primary: earth,
            a_au: 0.00257,
        });
        let star = body(
            world,
            "Star",
            BodyKind::Star,
            DVec3::new(1e6, 5e4, 0.0), // ~3 deg above Earth, seen from the Sun
            0.005,
        );
        world.entity_mut(star).insert(LabelLum(1e4));
        let earth_label = label(world, earth, "Earth");
        let moon_label = label(world, moon, "Moon");
        let star_label = label(world, star, "Star");
        let cam = world
            .spawn((
                FlyCam::default(),
                WorldPos(DVec3::ZERO),
                // Look along +X (yaw -90 deg).
                Transform::from_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
            ))
            .id();
        Scene {
            app,
            cam,
            earth_label,
            moon_label,
            star_label,
        }
    }

    fn shown(app: &App, label: Entity) -> bool {
        app.world().get::<Node>(label).unwrap().display != Display::None
    }

    #[test]
    fn test_moon_label_appears_only_when_separated_from_its_planet() {
        let mut s = scene();
        s.app.update();
        // From the Sun the Moon is ~0.15 deg from Earth: a pixel or two.
        assert!(shown(&s.app, s.earth_label));
        assert!(!shown(&s.app, s.moon_label));
        // Zooming in 100x separates them.
        *s.app.world_mut().resource_mut::<CameraFov>() =
            CameraFov(crate::zoom::DEFAULT_FOV / 100.0);
        s.app.update();
        assert!(shown(&s.app, s.earth_label));
        assert!(shown(&s.app, s.moon_label));
    }

    #[test]
    fn test_labels_hide_behind_a_planet_disk() {
        let mut s = scene();
        s.app.update();
        assert!(
            shown(&s.app, s.star_label),
            "star visible beside Earth's tiny disk"
        );
        // 2 Earth radii from Earth's center, between the Sun and Earth: the disk fills the
        // view and the star directly behind it is hidden; Earth's own label stays.
        s.app.world_mut().get_mut::<WorldPos>(s.cam).unwrap().0 =
            DVec3::new(1.0 - 8.5e-5, 0.0, 0.0);
        s.app.update();
        assert!(!shown(&s.app, s.star_label));
        assert!(shown(&s.app, s.earth_label));
    }

    #[test]
    fn test_master_switch_hides_everything() {
        let mut s = scene();
        s.app.world_mut().resource_mut::<LabelSettings>().enabled = false;
        s.app.update();
        assert!(!shown(&s.app, s.earth_label) && !shown(&s.app, s.star_label));
    }

    #[test]
    fn test_kind_filters() {
        let mut s = LabelSettings::default();
        assert!(s.shows(BodyKind::Moon));
        s.moons = false;
        assert!(!s.shows(BodyKind::Moon) && s.shows(BodyKind::Planet));
        s.enabled = false;
        assert!(!s.shows(BodyKind::Planet));
    }
}
