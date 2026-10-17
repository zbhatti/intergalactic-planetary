//! Object identity and facts, click-to-select at the crosshair, and the info panel (I).

use crate::camera::{CursorLocked, FlyCam};
use crate::constants::{AU_PER_LY, AU_PER_PC, KM_PER_AU};
use crate::coords::{BodyRadius, NavSet, WorldPos, view_transform};
use crate::hyg::HygCatalog;
use crate::hyg_bodies::HygEntities;
use crate::menu::MenuOpen;
use crate::search::Search;
use crate::starfield::StarPoints;
use crate::stars::{ExposureSettings, star_brightness};
use crate::ui::format_distance;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Star,
    Planet,
    DwarfPlanet,
    Moon,
    Constellation,
}

impl BodyKind {
    pub fn label(self) -> &'static str {
        match self {
            BodyKind::Star => "Star",
            BodyKind::Planet => "Planet",
            BodyKind::DwarfPlanet => "Dwarf planet",
            BodyKind::Moon => "Moon",
            BodyKind::Constellation => "Constellation",
        }
    }
}

/// Display name, kind, and static facts for a navigable object.
#[derive(Component, Clone, Debug)]
pub struct BodyInfo {
    pub name: String,
    pub kind: BodyKind,
    pub facts: Vec<(&'static str, String)>,
}

// --- physics helpers (shared by spawners) ---

pub const SUN_TEFF_K: f64 = 5772.0;
pub const EARTH_MASS_KG: f64 = 5.972e24;
const G_SI: f64 = 6.674_30e-11;

/// Effective temperature from luminosity and radius (both solar units), via Stefan-Boltzmann.
pub fn effective_temperature_k(luminosity_solar: f64, radius_solar: f64) -> f64 {
    SUN_TEFF_K * (luminosity_solar / (radius_solar * radius_solar)).powf(0.25)
}

/// Surface gravity in m/s^2.
pub fn surface_gravity(mass_kg: f64, radius_km: f64) -> f64 {
    let r_m = radius_km * 1000.0;
    G_SI * mass_kg / (r_m * r_m)
}

pub fn format_mass(kg: f64) -> String {
    let earths = kg / EARTH_MASS_KG;
    if earths >= 0.01 {
        format!("{kg:.3e} kg ({earths:.3} Earth)")
    } else {
        format!("{kg:.3e} kg")
    }
}

/// About three significant figures without float noise (catalog values are often f32).
pub fn format_sig(x: f64) -> String {
    match x.abs() {
        a if a >= 1.0e6 => format!("{x:.2e}"),
        a if a >= 1000.0 => format!("{x:.0}"),
        a if a >= 10.0 => format!("{x:.1}"),
        a if a >= 1.0 => format!("{x:.2}"),
        a if a >= 0.01 || a == 0.0 => format!("{x:.3}"),
        _ => format!("{x:.2e}"),
    }
}

pub fn format_radius(radius_au: f64) -> String {
    let km = radius_au * KM_PER_AU;
    if km >= 1.0e6 {
        format!("{:.3e} km ({:.2} Sun)", km, km / 695_700.0)
    } else {
        format!("{km:.0} km")
    }
}

/// Facts for a catalog star.
pub fn star_facts(
    spectral: char,
    luminosity_solar: f64,
    radius_solar: f64,
    dist_pc: f64,
) -> Vec<(&'static str, String)> {
    vec![
        ("Spectral class", spectral.to_string()),
        (
            "Luminosity",
            format!("{} Sun", format_sig(luminosity_solar)),
        ),
        (
            "Temperature",
            format!(
                "{:.0} K",
                effective_temperature_k(luminosity_solar, radius_solar)
            ),
        ),
        (
            "Distance from Sun",
            format!(
                "{:.2} pc ({:.1} ly)",
                dist_pc,
                dist_pc * AU_PER_PC / AU_PER_LY
            ),
        ),
    ]
}

// --- selection ---

/// Crosshair pick tolerance: how far (px) outside an object's disk a click still selects it.
pub const PICK_RADIUS_PX: f32 = 24.0;

/// A pickable object as seen on screen.
#[derive(Clone, Copy, Debug)]
pub struct ScreenHit<T> {
    pub id: T,
    pub screen: Vec2,
    /// Apparent disk radius in pixels.
    pub disk_px: f32,
    /// Distance from the camera, AU.
    pub dist: f64,
}

/// Picks the object under `point`: smallest pixel gap between the point and the object's
/// disk (0 if inside), within [`PICK_RADIUS_PX`]. Ties (e.g. both disks contain the point)
/// go to the nearer object.
pub fn pick<T: Copy>(hits: impl Iterator<Item = ScreenHit<T>>, point: Vec2) -> Option<T> {
    hits.filter_map(|h| {
        let gap = (h.screen.distance(point) - h.disk_px).max(0.0);
        (gap <= PICK_RADIUS_PX).then_some((gap, h.dist, h.id))
    })
    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)))
    .map(|(_, _, e)| e)
}

/// Apparent disk radius in pixels for a sphere of `radius` at `dist`.
pub fn disk_radius_px(radius: f64, dist: f64, fov_y: f32, viewport_h: f32) -> f32 {
    if dist <= radius {
        return f32::INFINITY;
    }
    let ang = (radius / dist).asin() as f32;
    ang.tan() / (fov_y * 0.5).tan() * (viewport_h * 0.5)
}

#[derive(Resource)]
pub struct ShowInfoPanel(pub bool);

impl Default for ShowInfoPanel {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Component)]
pub struct InfoPanel;

#[derive(Component)]
pub struct Crosshair;

pub fn setup_info_ui(mut commands: Commands) {
    commands.spawn((
        InfoPanel,
        crate::labels::LabelBlocker,
        Text::new(""),
        TextFont {
            font_size: 15.0,
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.92, 1.0)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(36.0),
            right: Val::Px(16.0),
            padding: UiRect::all(Val::Px(10.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.1, 0.94)),
        Visibility::Hidden,
        // Panels draw above world-space object labels.
        GlobalZIndex(10),
    ));
    commands.spawn((
        Crosshair,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            width: Val::Px(6.0),
            height: Val::Px(6.0),
            margin: UiRect::new(Val::Px(-3.0), Val::ZERO, Val::Px(-3.0), Val::ZERO),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BorderColor(Color::srgba(1.0, 1.0, 1.0, 0.5)),
        BorderRadius::MAX,
    ));
}

/// Catalog stars dimmer than this rendered brightness can't be picked: the crosshair should
/// select what you can see, not an invisible star that happens to be nearer the center.
pub const MIN_PICK_BRIGHTNESS: f32 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq)]
enum PickTarget {
    Entity(Entity),
    CatalogStar(usize),
}

/// Left click selects the object under the crosshair (screen center): any navigable entity,
/// or any visible catalog star (which is then promoted to an entity).
#[allow(clippy::too_many_arguments)]
pub fn select_on_click(
    mut commands: Commands,
    buttons: Res<ButtonInput<MouseButton>>,
    locked: Res<CursorLocked>,
    menu_open: Res<MenuOpen>,
    mut search: ResMut<Search>,
    mut show_info: ResMut<ShowInfoPanel>,
    cam: Query<(&Camera, &Projection, &WorldPos, &Transform), With<FlyCam>>,
    bodies: Query<(Entity, &WorldPos, &BodyRadius, &BodyInfo), Without<FlyCam>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    stars: (
        Res<HygCatalog>,
        Res<StarPoints>,
        ResMut<HygEntities>,
        Res<ExposureSettings>,
    ),
) {
    let (catalog, points, mut hyg_entities, exposure) = stars;
    if !buttons.just_pressed(MouseButton::Left) || !locked.0 || menu_open.0 || search.active {
        return;
    }
    let (Ok((camera, projection, cam_pos, cam_t)), Ok(window)) = (cam.single(), windows.single())
    else {
        return;
    };
    let Projection::Perspective(persp) = projection else {
        return;
    };
    let vp = Vec2::new(window.width(), window.height());
    let view = view_transform(cam_t.rotation);
    let entity_hits = bodies.iter().filter_map(|(e, pos, r, _)| {
        let rel = pos.0 - cam_pos.0;
        let dist = rel.length();
        let screen = camera.world_to_viewport(&view, rel.as_vec3()).ok()?;
        Some(ScreenHit {
            id: PickTarget::Entity(e),
            screen,
            disk_px: disk_radius_px(r.0, dist, persp.fov, vp.y),
            dist,
        })
    });
    // Catalog stars: ~100k projections, only on click.
    let ev = exposure.factor();
    let star_hits = points
        .indices
        .iter()
        .zip(&points.positions)
        .filter_map(|(&i, p)| {
            let rel = *p - cam_pos.0;
            let dist = rel.length();
            let lum = catalog.stars[i].lum;
            if star_brightness(ev, lum, (dist / AU_PER_PC) as f32) < MIN_PICK_BRIGHTNESS {
                return None;
            }
            let screen = camera.world_to_viewport(&view, rel.as_vec3()).ok()?;
            Some(ScreenHit {
                id: PickTarget::CatalogStar(i),
                screen,
                disk_px: 0.0,
                dist,
            })
        });
    let entity = match pick(entity_hits.chain(star_hits), vp * 0.5) {
        Some(PickTarget::Entity(e)) => e,
        Some(PickTarget::CatalogStar(i)) => {
            crate::hyg_bodies::promote(&mut commands, &catalog, &mut hyg_entities, i)
        }
        None => return,
    };
    search.target = Some(entity);
    search.target_name = Some(match bodies.get(entity) {
        Ok((_, _, _, info)) => info.name.clone(),
        // Just promoted: the entity's components arrive when commands apply.
        Err(_) => hyg_star_name(&catalog, entity, &hyg_entities),
    });
    show_info.0 = true;
}

fn hyg_star_name(catalog: &HygCatalog, entity: Entity, entities: &HygEntities) -> String {
    entities
        .0
        .iter()
        .find(|(_, e)| **e == entity)
        .map(|(&i, _)| crate::hyg_bodies::star_name(catalog, i, None))
        .unwrap_or_default()
}

pub fn toggle_info_panel(
    keys: Res<ButtonInput<KeyCode>>,
    search: Res<Search>,
    mut show: ResMut<ShowInfoPanel>,
) {
    if !search.active && keys.just_pressed(KeyCode::KeyI) {
        show.0 = !show.0;
    }
}

/// Info panel text for the selection, with live distance from the camera.
pub fn info_text(info: &BodyInfo, radius_au: f64, dist_au: f64) -> String {
    let mut lines = vec![
        info.name.clone(),
        info.kind.label().to_string(),
        format!("Distance: {}", format_distance(dist_au)),
        format!("Radius: {}", format_radius(radius_au)),
    ];
    lines.extend(info.facts.iter().map(|(k, v)| format!("{k}: {v}")));
    lines.push("[I] hide".to_string());
    lines.join("\n")
}

pub fn update_info_panel(
    show: Res<ShowInfoPanel>,
    search: Res<Search>,
    cam: Query<&WorldPos, With<FlyCam>>,
    bodies: Query<(&WorldPos, &BodyRadius, &BodyInfo), Without<FlyCam>>,
    mut panel: Query<(&mut Text, &mut Visibility), With<InfoPanel>>,
) {
    let Ok((mut text, mut vis)) = panel.single_mut() else {
        return;
    };
    let selected = search.target.and_then(|e| bodies.get(e).ok());
    let (Some((pos, radius, info)), true, Ok(cam)) = (selected, show.0, cam.single()) else {
        vis.set_if_neq(Visibility::Hidden);
        return;
    };
    let new = info_text(info, radius.0, pos.0.distance(cam.0));
    if text.0 != new {
        text.0 = new;
    }
    vis.set_if_neq(Visibility::Visible);
}

pub struct InfoPlugin;

impl Plugin for InfoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShowInfoPanel>()
            .add_systems(Startup, setup_info_ui)
            .add_systems(
                Update,
                (select_on_click, toggle_info_panel).in_set(NavSet::Input),
            )
            .add_systems(Update, update_info_panel.in_set(NavSet::View));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: u32, x: f32, disk_px: f32, dist: f64) -> ScreenHit<Entity> {
        ScreenHit {
            id: Entity::from_raw(id),
            screen: Vec2::new(x, 0.0),
            disk_px,
            dist,
        }
    }

    #[test]
    fn test_pick_closest_to_point() {
        let hits = [hit(1, 10.0, 1.0, 5.0), hit(2, 3.0, 1.0, 50.0)];
        assert_eq!(
            pick(hits.into_iter(), Vec2::ZERO),
            Some(Entity::from_raw(2))
        );
    }

    #[test]
    fn test_pick_nothing_outside_tolerance() {
        let hits = [hit(1, PICK_RADIUS_PX + 5.0, 1.0, 5.0)];
        assert_eq!(pick(hits.into_iter(), Vec2::ZERO), None);
    }

    #[test]
    fn test_pick_large_disk_counts_as_hit() {
        // A planet filling the view: its center is 300 px away but the point is on its disk.
        let hits = [hit(1, 300.0, 400.0, 0.01), hit(2, 20.0, 1.0, 1.0e6)];
        assert_eq!(
            pick(hits.into_iter(), Vec2::ZERO),
            Some(Entity::from_raw(1))
        );
    }

    #[test]
    fn test_pick_tie_goes_to_nearer() {
        let hits = [hit(1, 0.0, 10.0, 100.0), hit(2, 0.0, 10.0, 1.0)];
        assert_eq!(
            pick(hits.into_iter(), Vec2::ZERO),
            Some(Entity::from_raw(2))
        );
    }

    #[test]
    fn test_disk_radius_px() {
        // Sun from 1 AU (~0.27 deg radius) with a 45-degree FOV on 720 px: ~4.0 px.
        let px = disk_radius_px(0.00465, 1.0, std::f32::consts::FRAC_PI_4, 720.0);
        assert!((px - 4.04).abs() < 0.05, "got {px}");
        assert!(disk_radius_px(1.0, 0.5, 1.0, 720.0).is_infinite());
    }

    #[test]
    fn test_format_sig_keeps_small_and_large_values_readable() {
        assert_eq!(format_sig(25.4_f32 as f64), "25.4");
        assert_eq!(format_sig(0.5), "0.500");
        assert_eq!(format_sig(0.0004), "4.00e-4");
        assert_eq!(format_sig(126_000.0), "126000");
        assert_eq!(format_sig(3.2e7), "3.20e7");
    }

    #[test]
    fn test_effective_temperature() {
        assert!((effective_temperature_k(1.0, 1.0) - SUN_TEFF_K).abs() < 1e-9);
        // Sirius A: L=25.4, R=1.711 -> ~9900 K (published ~9940 K)
        let t = effective_temperature_k(25.4, 1.711);
        assert!((t - 9900.0).abs() < 150.0, "got {t}");
    }

    #[test]
    fn test_surface_gravity_earth() {
        let g = surface_gravity(EARTH_MASS_KG, 6371.0);
        assert!((g - 9.82).abs() < 0.02, "got {g}");
    }

    #[test]
    fn test_info_text_is_ascii() {
        let info = BodyInfo {
            name: "Sirius".into(),
            kind: BodyKind::Star,
            facts: star_facts('A', 25.4, 1.711, 2.637),
        };
        let t = info_text(&info, 0.008, 543_920.0);
        assert!(t.is_ascii(), "{t}");
        assert!(t.contains("8.6 ly"), "{t}");
        // f32 catalog values must not print float noise (25.399999618530273).
        let f32_lum = 25.4_f32 as f64;
        let t = info_text(
            &BodyInfo {
                facts: star_facts('A', f32_lum, 1.711, 2.637),
                ..info
            },
            0.008,
            1.0,
        );
        assert!(t.contains("Luminosity: 25.4 Sun"), "{t}");
    }
}
