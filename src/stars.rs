use bevy::prelude::*;
use rand::Rng;
use std::f32::consts::PI;
use crate::search::ObjectCatalog;

pub const PARSEC_TO_UNITS: f32 = 1000.0;
pub const SOLAR_RADIUS_UNITS: f32 = 695_700.0 / 149_597_870.7 / 206_265.0 * PARSEC_TO_UNITS;

// (name, RA hours, Dec degrees, distance parsecs, spectral class, radius solar radii)
const CATALOG_STARS: &[(&str, f32, f32, f32, char, f32)] = &[
    ("Sirius",           6.7525, -16.7161,   2.637, 'A',   1.711),
    ("Canopus",          6.3992, -52.6956,  95.88,  'F',  71.0),
    ("Arcturus",        14.2612,  19.1822,  11.26,  'K',  25.4),
    ("Rigil Kentaurus", 14.6599, -60.8342,   1.339, 'G',   1.227),
    ("Vega",            18.6157,  38.7836,   7.68,  'A',   2.362),
    ("Capella",          5.2781,  45.9981,  12.94,  'G',  11.98),
    ("Rigel",            5.2423,  -8.2016, 264.5,   'B',  78.9),
    ("Procyon",          7.6553,   5.2250,   3.509, 'F',   2.048),
    ("Achernar",         1.6286, -57.2367,  44.1,   'B',   6.78),
    ("Betelgeuse",       5.9195,   7.4069, 168.1,   'M', 887.0),
    ("Hadar",           14.0637, -60.3730, 161.0,   'B',   9.0),
    ("Altair",          19.8463,   8.8683,   5.130, 'A',   1.63),
    ("Acrux",           12.4432, -63.0991,  98.7,   'B',   7.8),
    ("Aldebaran",        4.5987,  16.5093,  19.96,  'K',  45.1),
    ("Antares",         16.4901, -26.4320, 185.0,   'M', 700.0),
    ("Spica",           13.4199, -11.1613,  77.0,   'B',   7.47),
    ("Pollux",           7.7553,  28.0262,  10.34,  'K',   9.06),
    ("Fomalhaut",       22.9608, -29.6224,   7.69,  'A',   1.842),
    ("Deneb",           20.6905,  45.2803, 802.0,   'A', 203.0),
    ("Mimosa",          12.7953, -59.6888,  85.0,   'B',   8.4),
    ("Regulus",         10.1395,  11.9672,  23.8,   'B',   3.092),
    ("Adhara",           6.9771, -28.9722, 132.4,   'B',  13.9),
    ("Shaula",          17.5601, -37.1038, 216.0,   'B',   6.5),
    ("Gacrux",          12.5193, -57.1132,  27.2,   'M',  84.0),
    ("Bellatrix",        5.4184,   6.3497,  76.8,   'B',   5.75),
    ("Elnath",           5.4381,  28.6074,  40.2,   'B',   4.2),
    ("Miaplacidus",      9.2199, -69.7172,  34.5,   'A',   6.8),
    ("Alnilam",          5.6036,  -1.2019, 410.0,   'B',  42.0),
    ("Alnitak",          5.6795,  -1.9425, 387.0,   'O',  22.5),
    ("Alioth",          12.9004,  55.9599,  24.9,   'A',   4.14),
    ("Dubhe",           11.0621,  61.7511,  37.9,   'K',  17.0),
    ("Mirfak",           3.4053,  49.8612, 161.0,   'F',  68.0),
    ("Wezen",            7.1399, -26.3932, 506.0,   'F', 215.0),
    ("Alkaid",          13.7923,  49.3133,  32.4,   'B',   3.4),
    ("Sargas",          17.6215, -42.9978,  80.1,   'F',  26.0),
    ("Menkent",         14.1114, -36.3699,  17.7,   'K',  10.6),
    ("Atria",           16.8113, -69.0277,  92.4,   'K', 143.0),
    ("Alhena",           6.6285,  16.3993,  30.2,   'A',   3.3),
    ("Peacock",         20.4274, -56.7350,  56.2,   'B',   4.05),
    ("Mirzam",           6.3783, -17.9559, 152.5,   'B',   9.7),
    ("Alphard",          9.4598,  -8.6586,  54.4,   'K',  50.5),
    ("Polaris",          2.5303,  89.2641, 132.9,   'F',  46.0),
    ("Hamal",            2.1199,  23.4624,  20.2,   'K',  14.9),
    ("Nunki",           18.9211, -26.2967,  70.5,   'B',   4.8),
    ("Saiph",            5.7959,  -9.6697, 221.0,   'B',  22.2),
    ("Kaus Australis",  18.4028, -34.3847,  44.5,   'B',   6.8),
    ("Rasalhague",      17.5822,  12.5600,  14.86,  'A',   2.858),
    ("Algol",            3.1362,  40.9556,  28.2,   'B',   2.73),
    ("Denebola",        11.8177,  14.5721,  11.0,   'A',   1.728),
    ("Mintaka",          5.5337,  -0.2991, 380.0,   'O',  16.5),
    ("Diphda",           0.7264, -17.9866,  29.4,   'K',  16.78),
    ("Caph",             0.1527,  59.1497,  16.8,   'F',   3.43),
    ("Avior",            8.3752, -59.5093, 165.0,   'K',   6.5),
    ("Naos",             8.0594, -40.0032, 429.0,   'O',  16.5),
    ("Almach",           2.0650,  42.3298,  97.0,   'K',  14.0),
    ("Phecda",          11.8977,  53.6948,  25.6,   'A',   3.04),
    ("Menkib",           3.9814,  35.7911, 344.0,   'O',  13.0),
    ("Regor",            8.1591, -47.3368, 336.0,   'O',   6.0),
];

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

pub fn star_world_pos(ra_hours: f32, dec_deg: f32, dist_pc: f32) -> Vec3 {
    let ra = ra_hours * 15.0 * PI / 180.0;
    let dec = dec_deg * PI / 180.0;
    let d = dist_pc * PARSEC_TO_UNITS;
    Vec3::new(d * dec.cos() * ra.cos(), d * dec.sin(), d * dec.cos() * ra.sin())
}

pub fn setup_stars(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut catalog: ResMut<ObjectCatalog>,
) {
    for &(name, ra, dec, dist, spect, radius_solar) in CATALOG_STARS {
        let pos = star_world_pos(ra, dec, dist);
        let radius = radius_solar * SOLAR_RADIUS_UNITS;
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(radius))),
            MeshMaterial3d(materials.add(StandardMaterial {
                emissive: spectral_color(spect),
                ..default()
            })),
            Transform::from_translation(pos),
        ));
        catalog.0.push((name.to_string(), pos));
    }

    let mut rng = rand::thread_rng();
    let bg_mat = materials.add(StandardMaterial {
        emissive: LinearRgba::new(0.7, 0.8, 1.0, 1.0),
        ..default()
    });
    let bg_mesh = meshes.add(Sphere::new(SOLAR_RADIUS_UNITS * 2.0));
    for _ in 0..2000 {
        let r = rng.gen_range(3000.0f32..80000.0f32);
        let theta = rng.gen_range(0.0f32..2.0 * PI);
        let y = rng.gen_range(-5000.0f32..5000.0f32);
        commands.spawn((
            Mesh3d(bg_mesh.clone()),
            MeshMaterial3d(bg_mat.clone()),
            Transform::from_translation(Vec3::new(r * theta.cos(), y, r * theta.sin())),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_ra0_dec0_dist1_maps_to_x_parsec() {
        let pos = star_world_pos(0.0, 0.0, 1.0);
        assert!((pos.x - PARSEC_TO_UNITS).abs() < 0.01, "x should be PARSEC_TO_UNITS, got {}", pos.x);
        assert!(pos.y.abs() < 0.01, "y should be ~0, got {}", pos.y);
        assert!(pos.z.abs() < 0.01, "z should be ~0, got {}", pos.z);
    }

    #[test]
    fn test_ra6h_dec0_dist1_maps_to_z_parsec() {
        let pos = star_world_pos(6.0, 0.0, 1.0);
        assert!(pos.x.abs() < 0.01, "x should be ~0, got {}", pos.x);
        assert!(pos.y.abs() < 0.01, "y should be ~0, got {}", pos.y);
        assert!((pos.z - PARSEC_TO_UNITS).abs() < 0.01, "z should be PARSEC_TO_UNITS, got {}", pos.z);
    }

    #[test]
    fn test_north_pole_dec90_maps_to_y_parsec() {
        let pos = star_world_pos(0.0, 90.0, 1.0);
        assert!(pos.x.abs() < 0.01, "x should be ~0, got {}", pos.x);
        assert!((pos.y - PARSEC_TO_UNITS).abs() < 0.01, "y should be PARSEC_TO_UNITS, got {}", pos.y);
        assert!(pos.z.abs() < 0.01, "z should be ~0, got {}", pos.z);
    }

    #[test]
    fn test_distance_scales_linearly() {
        let pos1 = star_world_pos(0.0, 0.0, 1.0);
        let pos2 = star_world_pos(0.0, 0.0, 2.0);
        assert!(
            (pos2.length() - 2.0 * pos1.length()).abs() < 0.01,
            "distance should scale linearly: 2x dist gave length {}, expected {}",
            pos2.length(),
            2.0 * pos1.length()
        );
    }
}
