//! Catalog stars as navigable objects.
//!
//! Most HYG stars only exist as starfield points. A star becomes an entity (WorldPos,
//! BodyRadius, BodyInfo) when something needs to address it:
//! - at startup, every star with a proper name (search, go-to);
//! - when picked with the crosshair;
//! - when the camera comes within [`PROXY_RANGE_AU`]: the nearest catalog star also gets a
//!   sphere mesh (rendered by the same brightness/size system as the curated stars), and its
//!   starfield point is hidden so the two never show side by side.

use crate::constants::{AU_PER_LY, AU_PER_PC};
use crate::coords::{BodyRadius, FloatingOrigin, NavSet, WorldPos, render_offset};
use crate::hyg::HygCatalog;
use crate::info::{BodyInfo, BodyKind, format_sig};
use crate::labels::{LabelLum, spawn_label};
use crate::search::ObjectCatalog;
use crate::starfield::{StarPoints, StarfieldHide};
use crate::stars::{CuratedHygMatches, SOLAR_RADIUS_UNITS, Star, spectral_color};
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

/// Within this distance (AU) of a catalog star, it's drawn as a sphere instead of a point.
pub const PROXY_RANGE_AU: f64 = 2000.0;

/// Starfield points within this distance (AU) of the proxied star are hidden. Larger than
/// the f32 error of chunk-local point positions (~8 AU); companions in very tight binaries
/// (e.g. Alpha Centauri A/B, 11-35 AU apart) hide together.
pub const HIDE_RADIUS_AU: f32 = 50.0;

/// Typical effective temperature (K) for a spectral class, for estimating radii.
pub fn class_temperature(class: char) -> f64 {
    match class {
        'O' => 35_000.0,
        'B' => 15_000.0,
        'A' => 8_500.0,
        'F' => 6_500.0,
        'G' => 5_600.0,
        'K' => 4_500.0,
        'M' => 3_200.0,
        _ => 5_800.0,
    }
}

/// Rough V-band bolometric correction (magnitudes) for a spectral class. Hot and cool stars
/// emit much of their light outside the visual band (UV / infrared).
pub fn bolometric_correction(class: char) -> f64 {
    match class {
        'O' => -3.0,
        'B' => -1.5,
        'A' => -0.2,
        'F' => -0.05,
        'G' => -0.1,
        'K' => -0.4,
        'M' => -1.5,
        _ => 0.0,
    }
}

/// Bolometric luminosity estimated from visual luminosity and spectral class.
pub fn estimated_bolometric_lum(visual_lum: f64, class: char) -> f64 {
    visual_lum * 10f64.powf(-bolometric_correction(class) / 2.5)
}

/// Radius (solar radii) from bolometric luminosity and temperature, via Stefan-Boltzmann.
pub fn estimated_radius_solar(lum: f64, teff: f64) -> f64 {
    lum.max(1e-6).sqrt() * (crate::info::SUN_TEFF_K / teff).powi(2)
}

/// Display name: proper name, else Hipparcos number, else HYG id.
pub fn star_name(catalog: &HygCatalog, i: usize, proper: Option<&str>) -> String {
    let s = &catalog.stars[i];
    match proper {
        Some(n) => n.to_string(),
        None if s.hip != 0 => format!("HIP {}", s.hip),
        None => format!("HYG {}", s.hyg_id),
    }
}

/// HYG index -> entity, for stars that have been promoted.
#[derive(Resource, Default)]
pub struct HygEntities(pub HashMap<usize, Entity>);

pub fn star_body(catalog: &HygCatalog, i: usize, name: String) -> (BodyInfo, f64, f64) {
    let s = &catalog.stars[i];
    let teff = class_temperature(s.class);
    let radius_solar = s.radius_sun.map_or_else(
        || estimated_radius_solar(estimated_bolometric_lum(s.lum as f64, s.class), teff),
        |r| r as f64,
    );
    let dist_pc = s.pos.length() / AU_PER_PC;
    let mut facts = vec![
        (
            "Spectral class",
            if s.class == ' ' {
                "unknown".to_string()
            } else {
                s.class.to_string()
            },
        ),
        (
            "Visual luminosity",
            format!("{} Sun", format_sig(s.lum as f64)),
        ),
        (
            "Apparent magnitude (from Sun)",
            format!(
                "{:.2}",
                catalog.apparent_magnitude(i, bevy::math::DVec3::ZERO)
            ),
        ),
        (
            "Distance from Sun",
            if s.dist_known {
                format!(
                    "{:.2} pc ({:.1} ly)",
                    dist_pc,
                    dist_pc * AU_PER_PC / AU_PER_LY
                )
            } else {
                format!("unknown (no parallax; shown at {dist_pc:.0} pc)")
            },
        ),
        (
            "Radius source",
            if s.radius_sun.is_some() {
                format!("NASA Exoplanet Archive ({} Sun)", format_sig(radius_solar))
            } else {
                format!(
                    "estimated from L and class (~{} Sun)",
                    format_sig(radius_solar)
                )
            },
        ),
    ];
    if s.hyg_id == 0 {
        // Appended at startup (exoplanets.rs): a host star that HYG doesn't list.
        facts.push(("Catalog", "NASA Exoplanet Archive (not in HYG)".to_string()));
    } else if s.hip != 0 {
        facts.push(("Catalog", format!("HIP {}, HYG {}", s.hip, s.hyg_id)));
    } else {
        facts.push(("Catalog", format!("HYG {}", s.hyg_id)));
    }
    (
        BodyInfo {
            name,
            kind: BodyKind::Star,
            facts,
        },
        radius_solar,
        radius_solar * SOLAR_RADIUS_UNITS,
    )
}

/// Returns the entity for HYG star `i`, spawning a lightweight one if needed.
pub fn promote(
    commands: &mut Commands,
    catalog: &HygCatalog,
    entities: &mut HygEntities,
    i: usize,
) -> Entity {
    if let Some(&e) = entities.0.get(&i) {
        return e;
    }
    let proper = catalog
        .names
        .iter()
        .find(|(k, _)| *k == i)
        .map(|(_, n)| n.as_str());
    let name = star_name(catalog, i, proper);
    promote_named(commands, catalog, entities, i, name)
}

/// [`promote`], naming a newly spawned entity `name` (an existing one keeps its name).
pub fn promote_named(
    commands: &mut Commands,
    catalog: &HygCatalog,
    entities: &mut HygEntities,
    i: usize,
    name: String,
) -> Entity {
    if let Some(&e) = entities.0.get(&i) {
        return e;
    }
    let (info, _, radius_au) = star_body(catalog, i, name);
    let e = commands
        .spawn((WorldPos(catalog.stars[i].pos), BodyRadius(radius_au), info))
        .id();
    entities.0.insert(i, e);
    e
}

/// Spawns entities for every named HYG star that isn't one of the curated sphere stars.
pub fn spawn_named_stars(
    mut commands: Commands,
    catalog: Res<HygCatalog>,
    curated: Res<CuratedHygMatches>,
    mut entities: ResMut<HygEntities>,
    mut objects: ResMut<ObjectCatalog>,
) {
    let skip: HashSet<usize> = curated.0.iter().flatten().copied().collect();
    for (i, name) in &catalog.names {
        // Curated stars have their own entities; appended exoplanet hosts (hyg_id 0) are
        // spawned, without labels, by exoplanets::spawn_hosts.
        if skip.contains(i) || catalog.stars[*i].hyg_id == 0 {
            continue;
        }
        let e = promote(&mut commands, &catalog, &mut entities, *i);
        commands.entity(e).insert(LabelLum(catalog.stars[*i].lum));
        spawn_label(&mut commands, e, name);
        objects.0.push((name.clone(), e));
    }
}

#[derive(Resource, Default)]
pub struct Proxy {
    pub index: Option<usize>,
    pub entity: Option<Entity>,
}

#[allow(clippy::too_many_arguments)]
pub fn update_star_proxy(
    mut commands: Commands,
    catalog: Res<HygCatalog>,
    points: Res<StarPoints>,
    origin: Res<FloatingOrigin>,
    mut entities: ResMut<HygEntities>,
    mut proxy: ResMut<Proxy>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    mut hide: ResMut<StarfieldHide>,
) {
    let wanted = points
        .nearest
        .filter(|&(_, dist)| dist < PROXY_RANGE_AU)
        .map(|(i, _)| i);
    if wanted != proxy.index {
        if let Some(old) = proxy.entity.take() {
            commands
                .entity(old)
                .remove::<(Mesh3d, MeshMaterial3d<StandardMaterial>, Star)>();
        }
        proxy.index = wanted;
        if let Some(i) = wanted {
            let e = promote(&mut commands, &catalog, &mut entities, i);
            let s = &catalog.stars[i];
            let (_, radius_solar, radius_au) = star_body(&catalog, i, String::new());
            commands.entity(e).insert((
                // Promoted stars have no Transform until they're drawn (thousands of them would
                // otherwise be rewritten on every rebase); place it now, since the rebase sync
                // only moves transforms whose WorldPos or the origin changed.
                Transform::from_translation(render_offset(s.pos, origin.0)),
                Visibility::default(),
                Mesh3d(meshes.add(Sphere::new(radius_au as f32))),
                MeshMaterial3d(std_materials.add(StandardMaterial {
                    emissive: spectral_color(s.class),
                    ..default()
                })),
                Star {
                    spectral: s.class,
                    radius_solar: radius_solar as f32,
                    visual_lum: s.lum,
                    last_brightness: 0.0,
                },
            ));
            proxy.entity = Some(e);
        }
    }
    // Hide the proxied star's point (render space follows the floating origin every frame).
    let new = proxy.index.map_or(Vec4::ZERO, |i| {
        render_offset(catalog.stars[i].pos, origin.0).extend(HIDE_RADIUS_AU)
    });
    hide.set_if_neq(StarfieldHide(new));
}

pub struct HygBodiesPlugin;

impl Plugin for HygBodiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HygEntities>()
            .init_resource::<Proxy>()
            .add_systems(Startup, spawn_named_stars.after(crate::stars::setup_stars))
            .add_systems(
                Update,
                update_star_proxy
                    .in_set(NavSet::Rebase)
                    .after(crate::coords::sync_render_transforms),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimated_radius_of_the_sun_is_one() {
        let r = estimated_radius_solar(1.0, crate::info::SUN_TEFF_K);
        assert!((r - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_estimated_radius_scales() {
        // Same temperature, 100x the luminosity -> 10x the radius.
        let r = estimated_radius_solar(100.0, 5772.0);
        assert!((r - 10.0).abs() < 1e-9);
        // Cooler at the same luminosity -> bigger.
        assert!(estimated_radius_solar(1.0, 3200.0) > 1.0);
    }

    #[test]
    fn test_star_names() {
        let cat = HygCatalog::embedded();
        let i = cat.index_of_name("Altair").unwrap();
        assert_eq!(star_name(&cat, i, Some("Altair")), "Altair");
        let hip = cat.stars.iter().position(|s| s.hip != 0).unwrap();
        assert!(star_name(&cat, hip, None).starts_with("HIP "));
        let no_hip = cat.stars.iter().position(|s| s.hip == 0).unwrap();
        assert!(star_name(&cat, no_hip, None).starts_with("HYG "));
    }

    #[test]
    fn test_named_star_info_is_ascii_and_sane() {
        let cat = HygCatalog::embedded();
        let i = cat
            .index_of_name("Barnard's Star")
            .expect("Barnard's Star in HYG");
        let (info, radius_solar, _) = star_body(&cat, i, "Barnard's Star".into());
        assert!(info.facts.iter().all(|(k, v)| k.is_ascii() && v.is_ascii()));
        // A red dwarf: published radius ~0.19 Sun; the class-based estimate should land in
        // the right range.
        assert!((0.08..0.3).contains(&radius_solar), "radius {radius_solar}");
        let lum = info
            .facts
            .iter()
            .find(|(k, _)| *k == "Visual luminosity")
            .unwrap();
        assert!(!lum.1.starts_with("0.000"), "{}", lum.1);
    }

    fn proxy_app() -> App {
        let mut app = crate::testing::headless_app();
        app.add_plugins(AssetPlugin::default())
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .insert_resource(HygCatalog::embedded())
            .init_resource::<StarfieldHide>()
            .init_resource::<StarPoints>()
            .init_resource::<FloatingOrigin>()
            .init_resource::<HygEntities>()
            .init_resource::<Proxy>()
            .add_systems(Update, update_star_proxy);
        app
    }

    fn hide_uniform(app: &App) -> Vec4 {
        app.world().resource::<StarfieldHide>().0
    }

    fn set_nearest(app: &mut App, nearest: Option<(usize, f64)>) {
        app.world_mut().resource_mut::<StarPoints>().nearest = nearest;
        app.update();
    }

    #[test]
    fn test_proxy_sphere_appears_near_and_goes_away_far() {
        let mut app = proxy_app();
        let i = app
            .world()
            .resource::<HygCatalog>()
            .index_of_name("Altair")
            .unwrap();

        set_nearest(&mut app, Some((i, PROXY_RANGE_AU * 10.0)));
        assert!(app.world().resource::<Proxy>().entity.is_none());

        set_nearest(&mut app, Some((i, 100.0)));
        let e = app
            .world()
            .resource::<Proxy>()
            .entity
            .expect("proxy spawned");
        assert!(app.world().get::<Mesh3d>(e).is_some());
        assert!(app.world().get::<Star>(e).is_some());
        assert!(app.world().get::<BodyInfo>(e).is_some());
        // The proxied star's point is hidden in the starfield.
        assert_eq!(hide_uniform(&app).w, HIDE_RADIUS_AU);

        set_nearest(&mut app, Some((i, PROXY_RANGE_AU * 2.0)));
        assert!(app.world().resource::<Proxy>().entity.is_none());
        // The entity stays (it's selectable/searchable); only the sphere is removed.
        assert!(app.world().get::<Mesh3d>(e).is_none());
        assert!(app.world().get::<BodyInfo>(e).is_some());
        assert_eq!(hide_uniform(&app), Vec4::ZERO);
    }

    #[test]
    fn test_proxy_moves_to_the_new_nearest_star() {
        let mut app = proxy_app();
        let (a, b) = {
            let cat = app.world().resource::<HygCatalog>();
            (
                cat.index_of_name("Altair").unwrap(),
                cat.index_of_name("Vega").unwrap(),
            )
        };
        set_nearest(&mut app, Some((a, 10.0)));
        let ea = app.world().resource::<Proxy>().entity.unwrap();
        set_nearest(&mut app, Some((b, 10.0)));
        let eb = app.world().resource::<Proxy>().entity.unwrap();
        assert_ne!(ea, eb);
        assert!(app.world().get::<Mesh3d>(ea).is_none());
        assert!(app.world().get::<Mesh3d>(eb).is_some());
        // Promotion is cached: the same star maps to the same entity again.
        set_nearest(&mut app, Some((a, 10.0)));
        assert_eq!(app.world().resource::<Proxy>().entity, Some(ea));
    }
}
