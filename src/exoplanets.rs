//! Exoplanet systems from the NASA Exoplanet Archive (Planetary Systems Composite table).
//!
//! Hosts: at startup each host is matched to its HYG star by Hipparcos number. Hosts that HYG
//! doesn't have (most Kepler/TESS/microlensing hosts) are appended to the catalog
//! ([`attach_to_catalog`]), so they're drawn in the starfield, can be picked, and get a sphere
//! close up like any other star. Every host gets an entity, so search and go-to find it by host
//! or planet name. Hosts get no labels, to keep the UI cost flat with ~4800 of them.
//!
//! Planets: a system's planets exist as entities only while the camera is near its star
//! ([`spawn_range_au`]). They're then in the B browser under their star, labelled, and selectable.
//! The archive gives a, e and the inclination to our line of sight, but not where the orbit's node
//! is or where the planet is along it. Those are set from a hash of the name (stable between
//! runs), and the info panel says so. Inclination is measured from the sky plane as seen from
//! Earth, so transiting planets (i ~ 90 deg) really do cross their star's disk from here.
//!
//! Data: `data/exoplanets.tsv` from `tools/build_exoplanets.py`.

use crate::camera::FlyCam;
use crate::constants::KM_PER_AU;
use crate::coords::{BodyRadius, NavSet, WorldPos};
use crate::hyg::{HygCatalog, HygStar};
use crate::hyg_bodies::{HygEntities, promote_named};
use crate::info::{BodyInfo, BodyKind, format_sig};
use crate::labels::spawn_label;
use crate::orbit::{equatorial_to_world, solve_kepler};
use crate::search::ObjectCatalog;
use crate::solar_system::{MinAngularSize, OrbitOf};
use crate::stars::star_world_pos;
use bevy::math::DVec3;
use bevy::prelude::*;
use std::collections::HashMap;

static EXOPLANETS_TSV: &str = include_str!("../data/exoplanets.tsv");

pub const EARTH_RADIUS_KM: f64 = 6371.0;

#[derive(Clone, Debug, PartialEq)]
pub struct ExoPlanet {
    pub name: String,
    pub a_au: f64,
    pub period_days: Option<f64>,
    pub ecc: Option<f64>,
    pub incl_deg: Option<f64>,
    pub radius_earth: Option<f64>,
    pub mass_earth: Option<f64>,
    pub eq_temp_k: Option<f64>,
    pub method: String,
    pub year: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExoHost {
    pub name: String,
    pub hip: u32,
    pub ra_deg: f64,
    pub dec_deg: f64,
    pub dist_pc: f64,
    pub vmag: Option<f64>,
    pub class: char,
    pub log_lum: Option<f64>,
    pub radius_sun: Option<f64>,
    pub planets: Vec<ExoPlanet>,
}

fn opt<T: std::str::FromStr>(s: &str) -> Option<T> {
    (!s.is_empty()).then(|| s.parse().ok()).flatten()
}

fn req<T: std::str::FromStr>(s: &str, what: &str) -> Result<T, String> {
    s.parse().map_err(|_| format!("bad {what}: {s:?}"))
}

pub fn parse_exoplanets(text: &str) -> Result<Vec<ExoHost>, String> {
    let mut hosts: Vec<ExoHost> = Vec::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        match f[..] {
            [
                "H",
                name,
                hip,
                ra,
                dec,
                dist,
                vmag,
                class,
                _teff,
                rad,
                log_lum,
                _mass,
            ] => hosts.push(ExoHost {
                name: name.into(),
                hip: opt(hip).unwrap_or(0),
                ra_deg: req(ra, "ra")?,
                dec_deg: req(dec, "dec")?,
                dist_pc: req(dist, "distance")?,
                vmag: opt(vmag),
                class: class.chars().next().unwrap_or(' '),
                log_lum: opt(log_lum),
                radius_sun: opt(rad),
                planets: Vec::new(),
            }),
            ["P", name, a, per, ecc, incl, rad, mass, eqt, method, year] => {
                let host = hosts.last_mut().ok_or("planet before any host")?;
                host.planets.push(ExoPlanet {
                    name: name.into(),
                    a_au: req(a, "semi-major axis")?,
                    period_days: opt(per),
                    ecc: opt(ecc),
                    incl_deg: opt(incl),
                    radius_earth: opt(rad),
                    mass_earth: opt(mass),
                    eq_temp_k: opt(eqt),
                    method: method.into(),
                    year: opt(year),
                })
            }
            _ => return Err(format!("bad line: {line}")),
        }
    }
    Ok(hosts)
}

/// A host and the HYG catalog index of its star.
pub struct ExoSystem {
    pub host: ExoHost,
    pub star: usize,
}

#[derive(Resource, Default)]
pub struct ExoSystems(pub Vec<ExoSystem>);

/// Visual luminosity (solar) of a host HYG doesn't have: from its V magnitude and distance,
/// else its bolometric luminosity, else the Sun's.
pub fn host_visual_lum(h: &ExoHost) -> f32 {
    match (h.vmag, h.log_lum) {
        (Some(v), _) => {
            let abs_mag = v - 5.0 * (h.dist_pc / 10.0).log10();
            10f64.powf((4.85 - abs_mag) / 2.5) as f32
        }
        (None, Some(l)) => 10f64.powf(l) as f32,
        (None, None) => 1.0,
    }
}

/// Matches hosts to HYG stars by Hipparcos number and appends the rest to `catalog` (with
/// `hyg_id == 0` and their host name).
pub fn attach_to_catalog(hosts: Vec<ExoHost>, catalog: &mut HygCatalog) -> ExoSystems {
    let by_hip: HashMap<u32, usize> = catalog
        .stars
        .iter()
        .enumerate()
        .filter(|(_, s)| s.hip != 0)
        .map(|(i, s)| (s.hip, i))
        .collect();
    let systems = hosts
        .into_iter()
        .map(|host| {
            let star = match by_hip.get(&host.hip) {
                Some(&i) if host.hip != 0 => {
                    if let Some(r) = host.radius_sun {
                        catalog.stars[i].radius_sun = Some(r as f32);
                    }
                    i
                }
                _ => {
                    catalog.stars.push(HygStar {
                        pos: star_world_pos(host.ra_deg / 15.0, host.dec_deg, host.dist_pc),
                        lum: host_visual_lum(&host),
                        hip: 0,
                        hyg_id: 0,
                        class: host.class,
                        dist_known: true,
                        radius_sun: host.radius_sun.map(|r| r as f32),
                    });
                    let i = catalog.stars.len() - 1;
                    catalog.names.push((i, host.name.clone()));
                    i
                }
            };
            ExoSystem { host, star }
        })
        .collect();
    ExoSystems(systems)
}

/// The embedded systems, attached to `catalog`. Call before anything reads the catalog.
pub fn load(catalog: &mut HygCatalog) -> ExoSystems {
    let hosts = parse_exoplanets(EXOPLANETS_TSV).expect("embedded exoplanet data parses (tested)");
    attach_to_catalog(hosts, catalog)
}

/// On a host star's entity: index into [`ExoSystems`].
#[derive(Component)]
pub struct ExoHostOf(pub usize);

/// Gives every host an entity (named after the host unless HYG already named the star), and
/// makes host and planet names searchable.
pub fn spawn_hosts(
    mut commands: Commands,
    systems: Res<ExoSystems>,
    catalog: Res<HygCatalog>,
    mut entities: ResMut<HygEntities>,
    mut objects: ResMut<ObjectCatalog>,
) {
    let mut known: std::collections::HashSet<String> =
        objects.0.iter().map(|(n, _)| n.clone()).collect();
    for (k, sys) in systems.0.iter().enumerate() {
        let e = promote_named(
            &mut commands,
            &catalog,
            &mut entities,
            sys.star,
            sys.host.name.clone(),
        );
        commands.entity(e).insert(ExoHostOf(k));
        if known.insert(sys.host.name.clone()) {
            objects.0.push((sys.host.name.clone(), e));
        }
        for p in &sys.host.planets {
            objects.0.push((p.name.clone(), e));
        }
    }
}

/// Adds the planet list to each host's info panel (after Startup commands have applied).
pub fn add_host_facts(systems: Res<ExoSystems>, mut hosts: Query<(&ExoHostOf, &mut BodyInfo)>) {
    for (host, mut info) in &mut hosts {
        let planets = &systems.0[host.0].host.planets;
        let names: Vec<&str> = planets
            .iter()
            .map(|p| p.name.rsplit(' ').next().unwrap_or(&p.name))
            .collect();
        info.facts.push((
            "Planets",
            format!(
                "{} ({}); fly closer than ~0.01 pc to see them",
                planets.len(),
                names.join(", ")
            ),
        ));
    }
}

/// Planets of a system are spawned within this distance (AU) of the star, and removed beyond
/// twice that.
pub fn spawn_range_au(host: &ExoHost) -> f64 {
    let max_a = host.planets.iter().map(|p| p.a_au).fold(0.0, f64::max);
    (max_a * 20.0).max(2000.0)
}

/// Stable pseudo-random number in [0, 1) from a name (FNV-1a), for unmeasured orbit angles.
pub fn name_hash01(name: &str, salt: u64) -> f64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ salt;
    for b in name.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Orbit plane basis for a host at `star_pos`: X along the sky's north, Y east, Z along our line
/// of sight (from the Sun to the star).
pub fn sky_frame(star_pos: DVec3) -> (DVec3, DVec3, DVec3) {
    let los = star_pos.normalize();
    let north = equatorial_to_world(DVec3::Z);
    let x = (north - los * north.dot(los))
        .try_normalize()
        .unwrap_or(DVec3::X);
    (x, los.cross(x), los)
}

/// Inclination used for a planet: measured if known, else a stable random one (uniform in cos i).
pub fn inclination_deg(p: &ExoPlanet) -> f64 {
    p.incl_deg
        .unwrap_or_else(|| (2.0 * name_hash01(&p.name, 1) - 1.0).acos().to_degrees())
}

/// Planet position relative to its star (AU, world frame).
pub fn planet_offset(p: &ExoPlanet, star_pos: DVec3) -> DVec3 {
    let (x, y, z) = sky_frame(star_pos);
    let e = p.ecc.unwrap_or(0.0).clamp(0.0, 0.99);
    let i = inclination_deg(p).to_radians();
    let omega = name_hash01(&p.name, 2) * std::f64::consts::TAU;
    let mean_anomaly = name_hash01(&p.name, 3) * std::f64::consts::TAU;
    let ecc_anomaly = solve_kepler(mean_anomaly, e);
    let nu = 2.0
        * ((1.0 + e).sqrt() * (ecc_anomaly / 2.0).sin())
            .atan2((1.0 - e).sqrt() * (ecc_anomaly / 2.0).cos());
    let r = p.a_au * (1.0 - e * ecc_anomaly.cos());
    let u = omega + nu;
    // Node along X (the sky's north); inclination tips the orbit toward the line of sight.
    x * (r * u.cos()) + y * (r * u.sin() * i.cos()) + z * (r * u.sin() * i.sin())
}

/// Radius (Earth radii): measured, else from mass by a rough mass-radius relation, else 1.
pub fn planet_radius_earth(p: &ExoPlanet) -> f64 {
    p.radius_earth.unwrap_or_else(|| match p.mass_earth {
        Some(m) if m < 2.0 => m.powf(0.27),
        Some(m) if m < 130.0 => 0.8 * m.powf(0.59),
        Some(_) => 11.0,
        None => 1.0,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanetType {
    Rocky,
    SubNeptune,
    IceGiant,
    GasGiant,
}

pub fn planet_type(radius_earth: f64) -> PlanetType {
    match radius_earth {
        r if r < 1.6 => PlanetType::Rocky,
        r if r < 4.0 => PlanetType::SubNeptune,
        r if r < 8.0 => PlanetType::IceGiant,
        _ => PlanetType::GasGiant,
    }
}

impl PlanetType {
    fn label(self) -> &'static str {
        match self {
            PlanetType::Rocky => "rocky (by size)",
            PlanetType::SubNeptune => "sub-Neptune (by size)",
            PlanetType::IceGiant => "Neptune-like (by size)",
            PlanetType::GasGiant => "gas giant (by size)",
        }
    }

    fn color(self) -> Color {
        match self {
            PlanetType::Rocky => Color::srgb(0.62, 0.52, 0.45),
            PlanetType::SubNeptune => Color::srgb(0.58, 0.68, 0.78),
            PlanetType::IceGiant => Color::srgb(0.40, 0.55, 0.85),
            PlanetType::GasGiant => Color::srgb(0.80, 0.70, 0.55),
        }
    }
}

pub fn planet_facts(p: &ExoPlanet, host: &str) -> Vec<(&'static str, String)> {
    let r = planet_radius_earth(p);
    let mut facts = vec![
        ("Orbits", host.to_string()),
        ("Type", planet_type(r).label().to_string()),
    ];
    if p.radius_earth.is_none() {
        facts.push(("Radius source", "estimated from mass".to_string()));
    }
    if let Some(m) = p.mass_earth {
        facts.push(("Mass", format!("{} Earth", format_sig(m))));
    }
    facts.push((
        "Orbit (semi-major axis)",
        format!("{} AU", format_sig(p.a_au)),
    ));
    if let Some(per) = p.period_days {
        facts.push(("Orbital period", format!("{} days", format_sig(per))));
    }
    facts.push(("Eccentricity", format!("{:.3}", p.ecc.unwrap_or(0.0))));
    if let Some(t) = p.eq_temp_k {
        facts.push(("Equilibrium temperature", format!("{t:.0} K")));
    }
    let disc = match p.year {
        Some(y) => format!("{} ({y})", p.method),
        None => p.method.clone(),
    };
    facts.push(("Discovered by", disc));
    facts.push((
        "Orbit orientation",
        if p.incl_deg.is_some() {
            "inclination measured; node and position assumed".to_string()
        } else {
            "assumed (not measured)".to_string()
        },
    ));
    facts
}

/// Planet and label entities of the systems currently spawned, by system index.
#[derive(Resource, Default)]
pub struct SpawnedSystems(pub HashMap<usize, Vec<Entity>>);

#[derive(Resource)]
pub struct PlanetMaterials(HashMap<PlanetTypeKey, Handle<StandardMaterial>>);

type PlanetTypeKey = u8;

fn type_key(t: PlanetType) -> PlanetTypeKey {
    t as u8
}

pub fn setup_planet_materials(
    mut commands: Commands,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let Some(materials) = materials.as_mut() else {
        return;
    };
    let map = [
        PlanetType::Rocky,
        PlanetType::SubNeptune,
        PlanetType::IceGiant,
        PlanetType::GasGiant,
    ]
    .into_iter()
    .map(|t| {
        (
            type_key(t),
            materials.add(StandardMaterial {
                base_color: t.color(),
                ..default()
            }),
        )
    })
    .collect();
    commands.insert_resource(PlanetMaterials(map));
}

/// Spawns the planets of systems the camera is near and removes those it has left. Runs only
/// when the camera moved; a scan of ~4800 host distances.
#[allow(clippy::too_many_arguments)]
pub fn update_exo_planets(
    mut commands: Commands,
    cam: Query<Ref<WorldPos>, With<FlyCam>>,
    hosts: Query<&WorldPos, (With<ExoHostOf>, Without<FlyCam>)>,
    systems: Res<ExoSystems>,
    catalog: Res<HygCatalog>,
    entities: Res<HygEntities>,
    mut spawned: ResMut<SpawnedSystems>,
    materials: Option<Res<PlanetMaterials>>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
) {
    let Ok(cam) = cam.single() else { return };
    if !cam.is_changed() {
        return;
    }
    for (k, sys) in systems.0.iter().enumerate() {
        // The host entity's position, not the catalog's: for the curated stars (e.g. Pollux)
        // the sphere sits at the curated table's coordinates, up to ~1e5 AU from HYG's.
        let Some(&host_entity) = entities.0.get(&sys.star) else {
            continue;
        };
        let star_pos = hosts
            .get(host_entity)
            .map_or(catalog.stars[sys.star].pos, |p| p.0);
        let dist = star_pos.distance(cam.0);
        let range = spawn_range_au(&sys.host);
        if dist > range * 2.0 {
            if let Some(list) = spawned.0.remove(&k) {
                for e in list {
                    commands.entity(e).despawn();
                }
            }
            continue;
        }
        if dist > range || spawned.0.contains_key(&k) {
            continue;
        }
        let mut list = Vec::new();
        for p in &sys.host.planets {
            let radius_earth = planet_radius_earth(p);
            let radius_au = radius_earth * EARTH_RADIUS_KM / KM_PER_AU;
            let mut e = commands.spawn((
                WorldPos(star_pos + planet_offset(p, star_pos)),
                BodyRadius(radius_au),
                BodyInfo {
                    name: p.name.clone(),
                    kind: BodyKind::Planet,
                    facts: planet_facts(p, &sys.host.name),
                },
                OrbitOf {
                    primary: host_entity,
                    a_au: p.a_au,
                },
                MinAngularSize,
                Transform::default(),
                Visibility::default(),
            ));
            if let (Some(materials), Some(meshes)) = (materials.as_ref(), meshes.as_mut()) {
                e.insert((
                    Mesh3d(meshes.add(Sphere::new(radius_au as f32))),
                    MeshMaterial3d(materials.0[&type_key(planet_type(radius_earth))].clone()),
                ));
            }
            let e = e.id();
            list.push(e);
            list.push(spawn_label(&mut commands, e, &p.name));
        }
        spawned.0.insert(k, list);
    }
}

pub struct ExoplanetsPlugin;

impl Plugin for ExoplanetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpawnedSystems>()
            .add_systems(
                Startup,
                (
                    setup_planet_materials,
                    spawn_hosts
                        .after(crate::hyg_bodies::spawn_named_stars)
                        .after(crate::stars::setup_stars),
                ),
            )
            .add_systems(PostStartup, add_host_facts)
            .add_systems(
                Update,
                update_exo_planets
                    .before(NavSet::Rebase)
                    .after(NavSet::Input),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::AU_PER_PC;
    use bevy::ecs::system::RunSystemOnce;

    fn loaded() -> (HygCatalog, ExoSystems) {
        let mut cat = HygCatalog::embedded();
        let systems = load(&mut cat);
        (cat, systems)
    }

    fn system<'a>(systems: &'a ExoSystems, name: &str) -> &'a ExoSystem {
        systems.0.iter().find(|s| s.host.name == name).unwrap()
    }

    #[test]
    fn test_embedded_data_parses() {
        let hosts = parse_exoplanets(EXOPLANETS_TSV).unwrap();
        assert_eq!(
            hosts.len(),
            4813,
            "matches data/exoplanets.tsv.manifest.txt"
        );
        let planets: usize = hosts.iter().map(|h| h.planets.len()).sum();
        assert_eq!(planets, 6416);
        let trappist = hosts.iter().find(|h| h.name == "TRAPPIST-1").unwrap();
        assert_eq!(trappist.planets.len(), 7);
        assert!(hosts.iter().all(|h| h.name.is_ascii() && h.dist_pc > 0.0));
    }

    #[test]
    fn test_hosts_match_hyg_by_hip_or_are_appended() {
        let base = HygCatalog::embedded().stars.len();
        let (cat, systems) = loaded();
        // 51 Pegasi (HIP 113357) is in HYG: same star, nothing appended for it.
        let peg = system(&systems, "51 Peg");
        assert!(peg.star < base);
        assert_eq!(cat.stars[peg.star].hip, 113357);
        // TRAPPIST-1 isn't in HYG: appended at its real position, with its name.
        let t1 = system(&systems, "TRAPPIST-1");
        assert!(t1.star >= base);
        assert_eq!(cat.stars[t1.star].hyg_id, 0);
        assert!((cat.stars[t1.star].pos.length() / AU_PER_PC - 12.43).abs() < 0.01);
        // Measured radius from the archive (an estimate from L and class gives ~0.03).
        assert_eq!(cat.stars[t1.star].radius_sun, Some(0.1192));
        assert!(cat.names.contains(&(t1.star, "TRAPPIST-1".to_string())));
        let appended = cat.stars.len() - base;
        assert!((3900..4100).contains(&appended), "{appended}");
    }

    #[test]
    fn test_appended_host_brightness_from_v_magnitude() {
        let (cat, systems) = loaded();
        let t1 = system(&systems, "TRAPPIST-1");
        // V = 17.0 at 12.4 pc: a very faint M8 dwarf.
        let m = cat.apparent_magnitude(t1.star, DVec3::ZERO);
        assert!((m - 17.02).abs() < 0.05, "{m}");
    }

    #[test]
    fn test_transiting_planets_cross_their_star_from_earth() {
        let (cat, systems) = loaded();
        let t1 = system(&systems, "TRAPPIST-1");
        let star = cat.stars[t1.star].pos;
        for p in &t1.host.planets {
            // i ~ 90: the orbit plane contains our line of sight, so the offset projected onto
            // the sky is (nearly) along one line through the star: the node line, X.
            let off = planet_offset(p, star);
            let (_, y, _) = sky_frame(star);
            let i = p.incl_deg.unwrap().to_radians();
            assert!(
                off.dot(y).abs() <= p.a_au * i.cos().abs() * 1.1 + 1e-9,
                "{}",
                p.name
            );
            assert!((off.length() / p.a_au - 1.0).abs() < 0.05, "{}", p.name);
        }
    }

    #[test]
    fn test_unmeasured_angles_are_stable() {
        assert_eq!(name_hash01("Kepler-22 b", 3), name_hash01("Kepler-22 b", 3));
        assert_ne!(name_hash01("Kepler-22 b", 3), name_hash01("Kepler-22 c", 3));
        let h = name_hash01("x", 0);
        assert!((0.0..1.0).contains(&h));
    }

    #[test]
    fn test_planet_types_and_facts() {
        assert_eq!(planet_type(1.0), PlanetType::Rocky);
        assert_eq!(planet_type(11.0), PlanetType::GasGiant);
        let p = ExoPlanet {
            name: "Test b".into(),
            a_au: 0.05,
            period_days: Some(4.2),
            ecc: None,
            incl_deg: None,
            radius_earth: None,
            mass_earth: Some(300.0),
            eq_temp_k: Some(1200.0),
            method: "Radial Velocity".into(),
            year: Some(1995),
        };
        assert_eq!(planet_radius_earth(&p), 11.0);
        let facts = planet_facts(&p, "Test");
        assert!(facts.iter().all(|(k, v)| k.is_ascii() && v.is_ascii()));
        assert!(facts.iter().any(|(_, v)| v == "assumed (not measured)"));
    }

    fn spawn_app() -> (App, Entity, DVec3) {
        let mut app = crate::testing::headless_app();
        let (cat, systems) = loaded();
        let star = cat.stars[system(&systems, "TRAPPIST-1").star].pos;
        app.insert_resource(cat)
            .insert_resource(systems)
            .init_resource::<HygEntities>()
            .init_resource::<ObjectCatalog>()
            .init_resource::<SpawnedSystems>()
            .add_systems(Update, update_exo_planets);
        // headless_app() has already run Startup, so run these directly.
        app.world_mut().run_system_once(spawn_hosts).unwrap();
        app.world_mut().run_system_once(add_host_facts).unwrap();
        let cam = app
            .world_mut()
            .spawn((FlyCam::default(), WorldPos(DVec3::new(0.0, 0.0, 1.0))))
            .id();
        app.update();
        (app, cam, star)
    }

    #[test]
    fn test_planets_follow_the_host_entity_not_the_catalog_position() {
        // Pollux (HD 62509) is a curated sphere ~3800 AU from its HYG position. Fake that with
        // an entity registered for the HYG index, placed 1e5 AU away.
        let (mut app, cam, _) = spawn_app();
        let (star, max_a) = {
            let systems = app.world().resource::<ExoSystems>();
            let s = system(systems, "HD 62509");
            (
                s.star,
                s.host.planets.iter().map(|p| p.a_au).fold(0.0, f64::max),
            )
        };
        let host = app.world().resource::<HygEntities>().0[&star];
        let sphere = app.world().resource::<HygCatalog>().stars[star].pos + DVec3::splat(1e5);
        app.world_mut().get_mut::<WorldPos>(host).unwrap().0 = sphere;
        app.world_mut().get_mut::<WorldPos>(cam).unwrap().0 = sphere + DVec3::new(0.0, 0.0, 1.0);
        app.update();
        app.update();
        let mut q = app.world_mut().query::<(&WorldPos, &OrbitOf)>();
        let planets: Vec<f64> = q
            .iter(app.world())
            .filter(|(_, o)| o.primary == host)
            .map(|(p, _)| p.0.distance(sphere))
            .collect();
        assert!(!planets.is_empty(), "Pollux b spawned");
        assert!(planets.iter().all(|&d| d <= max_a * 1.5), "{planets:?}");
    }

    #[test]
    fn test_planets_appear_near_their_star_and_go_away_after() {
        let (mut app, cam, star) = spawn_app();
        // Searchable by host and by planet name.
        let objects = &app.world().resource::<ObjectCatalog>().0;
        let host = objects.iter().find(|(n, _)| n == "TRAPPIST-1").unwrap().1;
        assert!(
            objects
                .iter()
                .any(|(n, e)| n == "TRAPPIST-1 e" && *e == host)
        );
        let info = app.world().get::<BodyInfo>(host).unwrap();
        assert!(
            info.facts
                .iter()
                .any(|(k, v)| *k == "Planets" && v.starts_with("7 (b, c"))
        );

        let planets = |app: &mut App| {
            let mut q = app.world_mut().query::<(&BodyInfo, &OrbitOf)>();
            q.iter(app.world())
                .filter(|(_, o)| o.primary == host)
                .count()
        };
        assert_eq!(
            planets(&mut app),
            0,
            "nothing spawned from the solar system"
        );
        app.world_mut().get_mut::<WorldPos>(cam).unwrap().0 = star + DVec3::new(0.0, 0.0, 0.1);
        app.update();
        app.update();
        assert_eq!(planets(&mut app), 7);
        app.world_mut().get_mut::<WorldPos>(cam).unwrap().0 = DVec3::new(0.0, 0.0, 1.0);
        app.update();
        app.update();
        assert_eq!(planets(&mut app), 0);
        assert!(app.world().resource::<SpawnedSystems>().0.is_empty());
    }
}
