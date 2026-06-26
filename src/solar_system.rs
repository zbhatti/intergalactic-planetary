use bevy::prelude::*;
use crate::constants::KM_PER_AU;
use crate::search::ObjectCatalog;

pub fn setup_solar_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut catalog: ResMut<ObjectCatalog>,
) {
    // (name, semi-major axis AU, mean longitude deg for 2026-06-25, radius km, R, G, B)
    let bodies: &[(&str, f32, f32, f32, f32, f32, f32)] = &[
        ("Sun",     0.0,     0.0,   695_700.0, 1.00, 0.85, 0.30),
        ("Mercury", 0.387,  28.85,   2_439.7,  0.55, 0.55, 0.55),
        ("Venus",   0.723, 279.68,   6_051.8,  0.88, 0.82, 0.62),
        ("Earth",   1.000, 117.26,   6_371.0,  0.25, 0.45, 0.85),
        ("Mars",    1.524,  26.25,   3_389.5,  0.78, 0.28, 0.12),
        ("Jupiter", 5.203, 117.90,  69_911.0,  0.78, 0.62, 0.45),
        ("Saturn",  9.537,  13.68,  58_232.0,  0.88, 0.78, 0.58),
        ("Uranus",  19.191, 67.56,  25_362.0,  0.48, 0.78, 0.82),
        ("Neptune", 30.069,  2.23,  24_622.0,  0.22, 0.35, 0.88),
    ];

    for &(name, a, lon_deg, radius_km, r, g, b) in bodies {
        let lon_rad = lon_deg.to_radians();
        let pos = Vec3::new(a * lon_rad.cos(), 0.0, a * lon_rad.sin());
        let radius_au = radius_km / KM_PER_AU;

        let mat = if name == "Sun" {
            materials.add(StandardMaterial {
                emissive: LinearRgba::new(2.0, 1.5, 0.5, 1.0),
                ..default()
            })
        } else {
            materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                ..default()
            })
        };

        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(radius_au))),
            MeshMaterial3d(mat),
            Transform::from_translation(pos),
        ));

        catalog.0.push((name.to_string(), pos));
    }
}
