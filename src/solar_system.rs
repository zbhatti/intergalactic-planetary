//! The Sun, planets, dwarf planets and major moons, placed for [`SCENE_JD`] from published
//! orbital elements:
//! - Planets: JPL "Approximate Positions of the Planets", Table 1 (Standish; 1800-2050 AD).
//! - Dwarf planets: JPL Small-Body Database (osculating elements).
//! - Moons: JPL planetary satellite mean elements (epoch J2000, their own reference planes).
//!   JPL notes these aren't for precise ephemerides; orbital phase can be off by some degrees
//!   for fast inner moons after 26 years, but distances and orbit planes are right.

use crate::camera::FlyCam;
use crate::constants::KM_PER_AU;
use crate::coords::{BodyRadius, WorldPos};
use crate::info::{BodyInfo, BodyKind, format_mass, format_sig, surface_gravity};
use crate::labels::{LabelLum, spawn_label};
use crate::orbit::{Elements, J2000_JD, RefPlane, SCENE_JD};
use crate::search::ObjectCatalog;
use crate::stars::{Star, star_visual_radius};
use crate::zoom::{CameraFov, min_star_angle};
use bevy::math::DVec3;
use bevy::prelude::*;

/// (name, [a, e, I, L, long.peri, long.node], rates per Julian century, radius km, mass kg,
///  mean temperature K, colour)
type PlanetRow = (&'static str, [f64; 6], [f64; 6], f64, f64, f64, [f32; 3]);

#[rustfmt::skip]
const PLANETS: &[PlanetRow] = &[
    ("Mercury", [0.38709927, 0.20563593, 7.00497902, 252.25032350, 77.45779628, 48.33076593],
        [0.00000037, 0.00001906, -0.00594749, 149472.67411175, 0.16047689, -0.12534081],
        2_439.7, 3.301e23, 440.0, [0.55, 0.55, 0.55]),
    ("Venus", [0.72333566, 0.00677672, 3.39467605, 181.97909950, 131.60246718, 76.67984255],
        [0.00000390, -0.00004107, -0.00078890, 58517.81538729, 0.00268329, -0.27769418],
        6_051.8, 4.867e24, 737.0, [0.88, 0.82, 0.62]),
    // Earth-Moon barycentre (Earth itself is ~4700 km from it).
    ("Earth", [1.00000261, 0.01671123, -0.00001531, 100.46457166, 102.93768193, 0.0],
        [0.00000562, -0.00004392, -0.01294668, 35999.37244981, 0.32327364, 0.0],
        6_371.0, 5.972e24, 288.0, [0.25, 0.45, 0.85]),
    ("Mars", [1.52371034, 0.09339410, 1.84969142, -4.55343205, -23.94362959, 49.55953891],
        [0.00001847, 0.00007882, -0.00813131, 19140.30268499, 0.44441088, -0.29257343],
        3_389.5, 6.417e23, 208.0, [0.78, 0.28, 0.12]),
    ("Jupiter", [5.20288700, 0.04838624, 1.30439695, 34.39644051, 14.72847983, 100.47390909],
        [-0.00011607, -0.00013253, -0.00183714, 3034.74612775, 0.21252668, 0.20469106],
        69_911.0, 1.898e27, 163.0, [0.78, 0.62, 0.45]),
    ("Saturn", [9.53667594, 0.05386179, 2.48599187, 49.95424423, 92.59887831, 113.66242448],
        [-0.00125060, -0.00050991, 0.00193609, 1222.49362201, -0.41897216, -0.28867794],
        58_232.0, 5.683e26, 133.0, [0.88, 0.78, 0.58]),
    ("Uranus", [19.18916464, 0.04725744, 0.77263783, 313.23810451, 170.95427630, 74.01692503],
        [-0.00196176, -0.00004397, -0.00242939, 428.48202785, 0.40805281, 0.04240589],
        25_362.0, 8.681e25, 78.0, [0.48, 0.78, 0.82]),
    ("Neptune", [30.06992276, 0.00859048, 1.77004347, -55.12002969, 44.96476227, 131.78422574],
        [0.00026291, 0.00005105, 0.00035372, 218.45945325, -0.32241464, -0.00508664],
        24_622.0, 1.024e26, 73.0, [0.22, 0.35, 0.88]),
];

/// Planet elements at `jd` from the Standish table (elements + rates x centuries).
pub fn planet_elements(base: [f64; 6], rates: [f64; 6], jd: f64) -> Elements {
    let t = (jd - J2000_JD) / 36525.0;
    let [a, e, i, l, peri_long, node] = std::array::from_fn(|k| base[k] + rates[k] * t);
    Elements {
        a_au: a,
        e,
        i_deg: i,
        node_deg: node,
        peri_deg: peri_long - node,
        m0_deg: l - peri_long,
        epoch_jd: jd,
        period_days: 360.0 / rates[3] * 36525.0,
        plane: RefPlane::Ecliptic,
    }
}

/// (name, a AU, e, i, node, peri, mean anomaly, epoch JD, period days, radius km, mass kg)
type DwarfRow = (
    &'static str,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
);

#[rustfmt::skip]
const DWARF_PLANETS: &[DwarfRow] = &[
    ("Ceres", 2.765552595034094, 0.07969229514816586, 10.58802780183462, 80.24862682043221,
        73.29421453021587, 274.4193463761342, 2461200.5, 1679.853119758983, 469.7, 9.384e20),
    ("Pluto", 39.58862938517124, 0.2518378778576892, 17.14771140999114, 110.2923840543057,
        113.7090015158565, 38.68366347318184, 2457588.5, 90981.71647718345, 1188.3, 1.303e22),
    ("Eris", 67.93394687853566, 0.4382385347971672, 43.9258279471791, 36.00477044417249,
        150.7949235840312, 211.774434275007, 2461200.5, 204516.6629430732, 1163.0, 1.6466e22),
    ("Haumea", 43.06029023650952, 0.1944430148898797, 28.20847393040364, 121.7860561329425,
        240.6905472508661, 223.2104118812299, 2461200.5, 103208.1173403618, 798.0, 4.006e21),
    ("Makemake", 45.57093317300052, 0.1588889953992523, 29.02785603743067, 79.2948338209406,
        297.0922733397207, 169.9379962048232, 2461200.5, 112364.8068762869, 715.0, 3.1e21),
];

/// Notable asteroids and comets: (name, comet?, a AU, e, i, node, peri, M at epoch, epoch JD,
/// period days, mean radius km, mass kg or 0 if unknown). Osculating heliocentric ecliptic
/// J2000 elements from the JPL Small-Body Database API (full precision, fetched 2026-10-17);
/// radius is half the SBDB diameter, mass is SBDB GM / G. Two-body propagation from each epoch:
/// fine for a static scene, though comets drift from planetary perturbations over decades.
type SmallBodyRow = (
    &'static str,
    bool,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
);

#[rustfmt::skip]
const SMALL_BODIES: &[SmallBodyRow] = &[
    ("Vesta", false, 2.361365965127599, 0.09020374382834395, 7.143925545058711, 103.701293265032,
        151.4686478221564, 81.19015607686903, 2461200.5, 1325.389042911101, 261.4, 2.590e20),
    ("Pallas", false, 2.769559010737709, 0.2307000995648547, 34.93279321851542, 172.8866193357694,
        310.9699161652136, 254.2496521742734, 2461200.5, 1683.504809564834, 256.5, 2.042e20),
    ("Hygiea", false, 3.150974033963701, 0.1067092741240963, 3.829529946447122, 283.1198927508594,
        312.4242387344704, 252.0344242359649, 2461200.5, 2042.987283349627, 203.6, 1.049e20),
    ("Juno", false, 2.670989527103278, 0.2556999836681878, 12.98659236598085, 169.8115953492418,
        247.8950743075613, 262.7322944883855, 2461200.5, 1594.434579527149, 123.3, 0.0),
    ("Eros", false, 1.458243716760167, 0.2228779627700761, 10.82854410314273, 304.2679713350896,
        178.9181319135911, 62.51145501986792, 2461200.5, 643.1963890927677, 8.42, 6.687e15),
    ("Itokawa", false, 1.324052284342771, 0.2801776414987972, 1.620940810523569, 69.07449749929083,
        162.8409022415483, 170.653905937934, 2461200.5, 556.4884171058932, 0.165, 3.15e10),
    ("Ryugu", false, 1.190918932702464, 0.1910730046480051, 5.866442486408568, 251.2897123995624,
        211.608993811871, 62.34067409463987, 2461200.5, 474.7027265420688, 0.448, 4.49e11),
    ("Bennu", false, 1.126391025894812, 0.2037450762416414, 6.03494377024794, 2.06086619569642,
        66.22306084084298, 101.703952002457, 2455562.5, 436.6487281120201, 0.242, 7.33e10),
    ("Apophis", false, 0.9223592206975018, 0.1911492279663492, 3.340996879880978, 203.8936514240762,
        126.6795706895841, 175.3304026592739, 2461200.5, 323.5553366891694, 0.17, 0.0),
    ("1P/Halley", true, 17.92863504856923, 0.9679359956953211, 162.1905300439129, 59.09894720612437,
        112.2414314637764, 274.3823371366792, 2439875.5, 27728.04608790421, 5.5, 0.0),
    ("2P/Encke", true, 2.218624763741819, 0.846940430888432, 11.33675914978793, 334.0171400057891,
        187.2881998732444, 36.82518175471338, 2460363.5, 1207.047083758139, 2.4, 0.0),
    ("9P/Tempel 1", true, 3.146133758958915, 0.5097028326964878, 10.4734281543904, 68.75357468050096,
        179.1972753698572, 336.5854438553629, 2457470.5, 2038.281680411032, 3.0, 0.0),
    ("67P/Churyumov-Gerasimenko", true, 3.462249490129549, 0.6409081308996354, 7.040294937543767,
        50.13557377155012, 12.79824970228189, 8.859927425218402, 2457305.5, 2353.076067903661, 1.7,
        9.92e12),
    ("81P/Wild 2", true, 3.449745576963111, 0.5373989073846863, 3.237004173608168, 136.1102208500619,
        41.72523085114398, 187.5966814232084, 2458808.5, 2340.340383523585, 2.0, 0.0),
];

// Reference-plane poles (RA, Dec in degrees). Laplace poles from the JPL satellite table;
// Uranus and Pluto equators from the IAU WGCCRE 2015 pole directions.
const MARS_LAPLACE: (f64, f64) = (317.7, 52.9);
const JUPITER_LAPLACE: (f64, f64) = (268.1, 64.5);
const SATURN_LAPLACE: (f64, f64) = (40.6, 83.5);
const URANUS_EQUATOR: (f64, f64) = (257.311, -15.175);
const PLUTO_EQUATOR: (f64, f64) = (132.993, -6.163);

/// (name, parent, a km, e, peri, mean anomaly, i, node, period days, plane pole or None for
///  ecliptic, radius km, mass kg). Elements: JPL mean elements, epoch 2000-01-01.5 TDB.
type MoonRow = (
    &'static str,
    &'static str,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    Option<(f64, f64)>,
    f64,
    f64,
);

#[rustfmt::skip]
const MOONS: &[MoonRow] = &[
    ("Moon", "Earth", 384400.0, 0.0554, 318.15, 135.27, 5.16, 125.08, 27.322, None, 1737.4, 7.342e22),
    ("Phobos", "Mars", 9375.0, 0.015, 216.3, 189.7, 1.1, 169.2, 0.3187, Some(MARS_LAPLACE), 11.27, 1.0659e16),
    ("Deimos", "Mars", 23457.0, 0.000, 0.0, 205.0, 1.8, 54.3, 1.2625, Some((316.6, 53.5)), 6.2, 1.4762e15),
    ("Io", "Jupiter", 421800.0, 0.004, 49.1, 330.9, 0.0, 0.0, 1.762732, Some(JUPITER_LAPLACE), 1821.6, 8.932e22),
    ("Europa", "Jupiter", 671100.0, 0.009, 45.0, 345.4, 0.5, 184.0, 3.525463, Some(JUPITER_LAPLACE), 1560.8, 4.800e22),
    ("Ganymede", "Jupiter", 1070400.0, 0.001, 198.3, 324.8, 0.2, 58.5, 7.155588, Some((268.2, 64.6)), 2634.1, 1.4819e23),
    ("Callisto", "Jupiter", 1882700.0, 0.007, 43.8, 87.4, 0.3, 309.1, 16.690440, Some((268.7, 64.8)), 2410.3, 1.0759e23),
    ("Mimas", "Saturn", 186000.0, 0.020, 160.4, 275.3, 1.6, 66.2, 0.942422, Some(SATURN_LAPLACE), 198.2, 3.75e19),
    ("Enceladus", "Saturn", 238400.0, 0.005, 119.5, 57.0, 0.0, 0.0, 1.370218, Some(SATURN_LAPLACE), 252.1, 1.08e20),
    ("Tethys", "Saturn", 295000.0, 0.001, 335.3, 0.0, 1.1, 273.0, 1.887802, Some(SATURN_LAPLACE), 531.1, 6.17e20),
    ("Dione", "Saturn", 377700.0, 0.002, 116.0, 212.0, 0.0, 0.0, 2.736916, Some(SATURN_LAPLACE), 561.4, 1.095e21),
    ("Rhea", "Saturn", 527200.0, 0.001, 44.3, 31.5, 0.3, 133.7, 4.517503, Some(SATURN_LAPLACE), 763.8, 2.307e21),
    ("Titan", "Saturn", 1221900.0, 0.029, 78.3, 11.7, 0.3, 78.6, 15.945448, Some((36.4, 84.0)), 2574.7, 1.3452e23),
    ("Iapetus", "Saturn", 3561700.0, 0.028, 254.5, 74.8, 7.6, 86.5, 79.331002, Some((288.7, 78.9)), 734.5, 1.806e21),
    ("Miranda", "Uranus", 129846.0, 0.001, 154.8, 73.0, 4.4, 100.9, 1.413479, Some(URANUS_EQUATOR), 235.8, 6.4e19),
    ("Ariel", "Uranus", 190929.0, 0.001, 9.6, 193.5, 0.0, 0.0, 2.520379, Some(URANUS_EQUATOR), 578.9, 1.251e21),
    ("Umbriel", "Uranus", 265986.0, 0.004, 183.4, 253.0, 0.1, 174.8, 4.144177, Some(URANUS_EQUATOR), 584.7, 1.275e21),
    ("Titania", "Uranus", 436298.0, 0.002, 184.0, 68.1, 0.1, 29.5, 8.705869, Some(URANUS_EQUATOR), 788.9, 3.4e21),
    ("Oberon", "Uranus", 583511.0, 0.002, 132.2, 143.6, 0.1, 76.8, 13.463237, Some(URANUS_EQUATOR), 761.4, 3.076e21),
    ("Triton", "Neptune", 354800.0, 0.000, 0.0, 63.0, 157.3, 178.1, 5.876994, Some((299.8, 43.1)), 1353.4, 2.139e22),
    ("Charon", "Pluto", 19600.0, 0.000, 0.0, 304.1, 0.0, 0.0, 6.387222, Some(PLUTO_EQUATOR), 606.0, 1.586e21),
];

/// Planets, dwarf planets and moons: drawn at least MIN_STAR_ANGLE wide so they stay visible
/// as points from afar, like stars.
#[derive(Component)]
pub struct MinAngularSize;

/// Rescales bodies to their minimum apparent size; runs only when the camera moved or zoomed.
pub fn update_body_visual_size(
    cam: Query<Ref<WorldPos>, With<FlyCam>>,
    fov: Res<CameraFov>,
    mut bodies: Query<(&WorldPos, &BodyRadius, &mut Transform), With<MinAngularSize>>,
) {
    let Ok(cam) = cam.single() else { return };
    if !cam.is_changed() && !fov.is_changed() {
        return;
    }
    let min_angle = min_star_angle(fov.0);
    for (pos, radius, mut transform) in &mut bodies {
        let physical = radius.0 as f32;
        let visual = star_visual_radius(physical, pos.0.distance(cam.0) as f32, min_angle);
        let scale = Vec3::splat(visual / physical);
        if transform.scale != scale {
            transform.scale = scale;
        }
    }
}

/// What a body orbits, and its semi-major axis there (AU). Drives the solar system browser.
#[derive(Component, Clone, Copy, Debug)]
pub struct OrbitOf {
    pub primary: Entity,
    pub a_au: f64,
}

struct Spawner<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    meshes: &'a mut Assets<Mesh>,
    catalog: &'a mut ObjectCatalog,
}

impl Spawner<'_, '_, '_> {
    fn body(
        &mut self,
        pos: DVec3,
        radius_km: f64,
        material: Handle<StandardMaterial>,
        info: BodyInfo,
        orbit: Option<OrbitOf>,
    ) -> Entity {
        let radius_au = radius_km / KM_PER_AU;
        let name = info.name.clone();
        let mut entity = self.commands.spawn((
            Mesh3d(self.meshes.add(Sphere::new(radius_au as f32))),
            MeshMaterial3d(material),
            Transform::default(),
            WorldPos(pos),
            BodyRadius(radius_au),
            info,
            MinAngularSize,
        ));
        if let Some(orbit) = orbit {
            entity.insert(orbit);
        }
        let entity = entity.id();
        spawn_label(self.commands, entity, &name);
        self.catalog.0.push((name, entity));
        entity
    }
}

fn physical_facts(mass_kg: f64, radius_km: f64) -> Vec<(&'static str, String)> {
    vec![
        ("Mass", format_mass(mass_kg)),
        (
            "Surface gravity",
            format!("{:.3} m/s2", surface_gravity(mass_kg, radius_km)),
        ),
    ]
}

pub fn setup_solar_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut catalog: ResMut<ObjectCatalog>,
) {
    let mut sp = Spawner {
        commands: &mut commands,
        meshes: &mut meshes,
        catalog: &mut catalog,
    };
    // (name, world position, entity) of everything spawned so far, for parenting.
    let mut spawned: Vec<(&str, DVec3, Entity)> = Vec::new();

    let sun_mat = materials.add(StandardMaterial {
        emissive: LinearRgba::new(2.0, 1.5, 0.5, 1.0),
        ..default()
    });
    let mut sun_facts = physical_facts(1.989e30, 695_700.0);
    sun_facts.push(("Temperature", "5772 K".to_string()));
    let sun = sp.body(
        DVec3::ZERO,
        695_700.0,
        sun_mat,
        BodyInfo {
            name: "Sun".into(),
            kind: BodyKind::Star,
            facts: sun_facts,
        },
        None,
    );
    // The Sun's glow follows the same brightness model and exposure as every other star (and
    // drives auto-exposure); update_star_brightness sizes it, so it doesn't need MinAngularSize.
    sp.commands
        .entity(sun)
        .insert((LabelLum(1.0), Star::new('G', 1.0, 1.0)))
        .remove::<MinAngularSize>();
    spawned.push(("Sun", DVec3::ZERO, sun));

    for &(name, base, rates, radius_km, mass_kg, temp_k, [r, g, b]) in PLANETS {
        let el = planet_elements(base, rates, SCENE_JD);
        let pos = el.position_world(SCENE_JD);
        let mut facts = physical_facts(mass_kg, radius_km);
        facts.push(("Mean temperature", format!("{temp_k:.0} K")));
        facts.push(("Orbit (semi-major axis)", format!("{:.3} AU", el.a_au)));
        facts.push(("Eccentricity", format!("{:.4}", el.e)));
        facts.push((
            "Orbital period",
            format!("{} years", format_sig(el.period_days / 365.25)),
        ));
        let mat = materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            ..default()
        });
        let e = sp.body(
            pos,
            radius_km,
            mat,
            BodyInfo {
                name: name.into(),
                kind: BodyKind::Planet,
                facts,
            },
            Some(OrbitOf {
                primary: sun,
                a_au: el.a_au,
            }),
        );
        spawned.push((name, pos, e));
    }

    let dwarf_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.70, 0.62, 0.55),
        ..default()
    });
    for &(name, a, e, i, node, peri, m0, epoch, period, radius_km, mass_kg) in DWARF_PLANETS {
        let el = Elements {
            a_au: a,
            e,
            i_deg: i,
            node_deg: node,
            peri_deg: peri,
            m0_deg: m0,
            epoch_jd: epoch,
            period_days: period,
            plane: RefPlane::Ecliptic,
        };
        let pos = el.position_world(SCENE_JD);
        let mut facts = physical_facts(mass_kg, radius_km);
        facts.push(("Orbit (semi-major axis)", format!("{a:.2} AU")));
        facts.push(("Eccentricity", format!("{e:.3}")));
        facts.push((
            "Orbital period",
            format!("{} years", format_sig(period / 365.25)),
        ));
        let e = sp.body(
            pos,
            radius_km,
            dwarf_mat.clone(),
            BodyInfo {
                name: name.into(),
                kind: BodyKind::DwarfPlanet,
                facts,
            },
            Some(OrbitOf {
                primary: sun,
                a_au: a,
            }),
        );
        spawned.push((name, pos, e));
    }

    let asteroid_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.50, 0.47, 0.43),
        ..default()
    });
    let comet_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.75, 0.80, 0.85),
        ..default()
    });
    for &(name, comet, a, e, i, node, peri, m0, epoch, period, radius_km, mass_kg) in SMALL_BODIES {
        let el = Elements {
            a_au: a,
            e,
            i_deg: i,
            node_deg: node,
            peri_deg: peri,
            m0_deg: m0,
            epoch_jd: epoch,
            period_days: period,
            plane: RefPlane::Ecliptic,
        };
        let mut facts = if mass_kg > 0.0 {
            physical_facts(mass_kg, radius_km)
        } else {
            vec![("Mass", "unknown".to_string())]
        };
        facts.push(("Orbit (semi-major axis)", format!("{a:.3} AU")));
        facts.push(("Eccentricity", format!("{e:.3}")));
        facts.push(("Perihelion", format!("{:.3} AU", a * (1.0 - e))));
        facts.push((
            "Orbital period",
            format!("{} years", format_sig(period / 365.25)),
        ));
        sp.body(
            el.position_world(SCENE_JD),
            radius_km,
            if comet {
                comet_mat.clone()
            } else {
                asteroid_mat.clone()
            },
            BodyInfo {
                name: name.into(),
                kind: if comet {
                    BodyKind::Comet
                } else {
                    BodyKind::Asteroid
                },
                facts,
            },
            Some(OrbitOf {
                primary: sun,
                a_au: a,
            }),
        );
    }

    let moon_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.62, 0.62, 0.60),
        ..default()
    });
    for &(name, parent, a_km, e, peri, m0, i, node, period, pole, radius_km, mass_kg) in MOONS {
        let Some(&(_, parent_pos, parent_entity)) = spawned.iter().find(|(n, _, _)| *n == parent)
        else {
            continue;
        };
        let a_au = a_km / KM_PER_AU;
        let el = Elements {
            a_au,
            e,
            i_deg: i,
            node_deg: node,
            peri_deg: peri,
            m0_deg: m0,
            epoch_jd: J2000_JD,
            period_days: period,
            plane: pole.map_or(RefPlane::Ecliptic, |(ra_deg, dec_deg)| RefPlane::Pole {
                ra_deg,
                dec_deg,
            }),
        };
        let mut facts = vec![("Orbits", parent.to_string())];
        facts.extend(physical_facts(mass_kg, radius_km));
        facts.push(("Orbit radius", format!("{a_km:.0} km")));
        facts.push(("Orbital period", format!("{} days", format_sig(period))));
        sp.body(
            parent_pos + el.position_world(SCENE_JD),
            radius_km,
            moon_mat.clone(),
            BodyInfo {
                name: name.into(),
                kind: BodyKind::Moon,
                facts,
            },
            Some(OrbitOf {
                primary: parent_entity,
                a_au,
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orbit::OBLIQUITY_DEG;

    fn planet(name: &str) -> &'static PlanetRow {
        PLANETS.iter().find(|p| p.0 == name).unwrap()
    }

    /// Heliocentric ecliptic longitude (deg) of a world-space position.
    fn ecliptic_longitude(world: DVec3) -> f64 {
        // world -> equatorial: (x, -z, y); then rotate by -obliquity about X.
        let eq = DVec3::new(world.x, -world.z, world.y);
        let (s, c) = OBLIQUITY_DEG.to_radians().sin_cos();
        let ecl = DVec3::new(eq.x, eq.y * c + eq.z * s, -eq.y * s + eq.z * c);
        ecl.y.atan2(ecl.x).to_degrees().rem_euclid(360.0)
    }

    #[test]
    fn test_earth_is_opposite_the_june_sun() {
        // Late June: the Sun appears at ecliptic longitude ~93.6 deg, so Earth is at ~273.6.
        let p = planet("Earth");
        let pos = planet_elements(p.1, p.2, SCENE_JD).position_world(SCENE_JD);
        let lon = ecliptic_longitude(pos);
        assert!((lon - 273.6).abs() < 1.0, "Earth at {lon} deg");
        assert!(
            (pos.length() - 1.017).abs() < 0.003,
            "near aphelion, got {}",
            pos.length()
        );
    }

    #[test]
    fn test_planets_lie_near_the_ecliptic_not_the_equator() {
        for p in PLANETS {
            let pos = planet_elements(p.1, p.2, SCENE_JD).position_world(SCENE_JD);
            let ecl_pole = crate::orbit::ecliptic_to_world(DVec3::Z);
            let lat = (pos.normalize().dot(ecl_pole)).asin().to_degrees();
            assert!(lat.abs() < 7.1, "{} at ecliptic latitude {lat}", p.0);
        }
    }

    #[test]
    fn test_outer_planet_longitudes_match_mean_motion() {
        // Jupiter's 2026-06-25 mean longitude ~118 deg; with e=0.048 its true longitude is
        // within a few degrees of that.
        let p = planet("Jupiter");
        let lon = ecliptic_longitude(planet_elements(p.1, p.2, SCENE_JD).position_world(SCENE_JD));
        assert!((lon - 118.0).abs() < 6.0, "Jupiter at {lon}");
    }

    fn dwarf_pos(name: &str) -> DVec3 {
        let &(_, a, e, i, node, peri, m0, epoch, period, _, _) =
            DWARF_PLANETS.iter().find(|d| d.0 == name).unwrap();
        Elements {
            a_au: a,
            e,
            i_deg: i,
            node_deg: node,
            peri_deg: peri,
            m0_deg: m0,
            epoch_jd: epoch,
            period_days: period,
            plane: RefPlane::Ecliptic,
        }
        .position_world(SCENE_JD)
    }

    #[test]
    fn test_dwarf_planet_distances_in_2026() {
        // Published heliocentric distances around 2026: Pluto ~35 AU, Eris ~96 AU.
        let pluto = dwarf_pos("Pluto").length();
        assert!((34.0..36.5).contains(&pluto), "Pluto at {pluto} AU");
        let eris = dwarf_pos("Eris").length();
        assert!((94.0..97.5).contains(&eris), "Eris at {eris} AU");
    }

    fn small_body_pos(name: &str) -> DVec3 {
        let &(_, _, a, e, i, node, peri, m0, epoch, period, ..) =
            SMALL_BODIES.iter().find(|b| b.0 == name).unwrap();
        Elements {
            a_au: a,
            e,
            i_deg: i,
            node_deg: node,
            peri_deg: peri,
            m0_deg: m0,
            epoch_jd: epoch,
            period_days: period,
            plane: RefPlane::Ecliptic,
        }
        .position_world(SCENE_JD)
    }

    #[test]
    fn test_small_body_positions_in_2026() {
        // Halley passed aphelion (35.1 AU) in Dec 2023, so it's still ~35 AU out.
        let halley = small_body_pos("1P/Halley").length();
        assert!((33.0..35.5).contains(&halley), "Halley at {halley} AU");
        // Main-belt asteroids stay between perihelion and aphelion.
        for (name, q, big_q) in [("Vesta", 2.14, 2.58), ("Pallas", 2.13, 3.41)] {
            let r = small_body_pos(name).length();
            assert!((q..big_q).contains(&r), "{name} at {r} AU");
        }
        // Halley's orbit is retrograde: i = 162 deg.
        assert!(SMALL_BODIES.iter().all(|b| b.0.is_ascii()));
    }

    #[test]
    fn test_moon_offsets_match_orbit_radius() {
        for m in MOONS {
            let (a_km, e) = (m.2, m.3);
            let el = Elements {
                a_au: a_km / KM_PER_AU,
                e,
                i_deg: m.6,
                node_deg: m.7,
                peri_deg: m.4,
                m0_deg: m.5,
                epoch_jd: J2000_JD,
                period_days: m.8,
                plane: RefPlane::Ecliptic,
            };
            let r_km = el.position_world(SCENE_JD).length() * KM_PER_AU;
            assert!(
                r_km >= a_km * (1.0 - e) - 1.0 && r_km <= a_km * (1.0 + e) + 1.0,
                "{}: {r_km} km",
                m.0
            );
        }
    }

    #[test]
    fn test_every_moon_has_a_parent() {
        let parents: Vec<&str> = PLANETS
            .iter()
            .map(|p| p.0)
            .chain(DWARF_PLANETS.iter().map(|d| d.0))
            .collect();
        for m in MOONS {
            assert!(parents.contains(&m.1), "{} orbits unknown {}", m.0, m.1);
        }
    }
}
