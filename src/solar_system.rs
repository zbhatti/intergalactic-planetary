use crate::constants::KM_PER_AU;
use crate::coords::{BodyRadius, WorldPos};
use crate::info::{BodyInfo, BodyKind, format_mass, surface_gravity};
use crate::search::ObjectCatalog;
use crate::stars::StarLabelFor;
use bevy::math::DVec3;
use bevy::prelude::*;

pub fn setup_solar_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut catalog: ResMut<ObjectCatalog>,
) {
    // (name, semi-major axis AU, mean longitude deg for 2026-06-25, radius km, mass kg,
    //  mean surface/effective temperature K, R, G, B). Mass/temperature: NASA planetary fact sheets.
    #[allow(clippy::type_complexity)]
    let bodies: &[(&str, f64, f64, f64, f64, f64, f32, f32, f32)] = &[
        (
            "Sun", 0.0, 0.0, 695_700.0, 1.989e30, 5772.0, 1.00, 0.85, 0.30,
        ),
        (
            "Mercury", 0.387, 28.85, 2_439.7, 3.301e23, 440.0, 0.55, 0.55, 0.55,
        ),
        (
            "Venus", 0.723, 279.68, 6_051.8, 4.867e24, 737.0, 0.88, 0.82, 0.62,
        ),
        (
            "Earth", 1.000, 117.26, 6_371.0, 5.972e24, 288.0, 0.25, 0.45, 0.85,
        ),
        (
            "Mars", 1.524, 26.25, 3_389.5, 6.417e23, 208.0, 0.78, 0.28, 0.12,
        ),
        (
            "Jupiter", 5.203, 117.90, 69_911.0, 1.898e27, 163.0, 0.78, 0.62, 0.45,
        ),
        (
            "Saturn", 9.537, 13.68, 58_232.0, 5.683e26, 133.0, 0.88, 0.78, 0.58,
        ),
        (
            "Uranus", 19.191, 67.56, 25_362.0, 8.681e25, 78.0, 0.48, 0.78, 0.82,
        ),
        (
            "Neptune", 30.069, 2.23, 24_622.0, 1.024e26, 73.0, 0.22, 0.35, 0.88,
        ),
    ];

    for &(name, a, lon_deg, radius_km, mass_kg, temp_k, r, g, b) in bodies {
        let lon_rad = lon_deg.to_radians();
        let pos = DVec3::new(a * lon_rad.cos(), 0.0, a * lon_rad.sin());
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

        let body_entity = commands
            .spawn((
                Mesh3d(meshes.add(Sphere::new(radius_au as f32))),
                MeshMaterial3d(mat),
                Transform::default(),
                WorldPos(pos),
                BodyRadius(radius_au),
                body_info(name, a, radius_km, mass_kg, temp_k),
            ))
            .id();

        commands.spawn((
            StarLabelFor(body_entity),
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

        catalog.0.push((name.to_string(), body_entity));
    }
}

fn body_info(name: &str, a_au: f64, radius_km: f64, mass_kg: f64, temp_k: f64) -> BodyInfo {
    let is_sun = a_au == 0.0;
    let mut facts = vec![
        ("Mass", format_mass(mass_kg)),
        (
            "Surface gravity",
            format!("{:.2} m/s2", surface_gravity(mass_kg, radius_km)),
        ),
        (
            if is_sun {
                "Temperature"
            } else {
                "Mean temperature"
            },
            format!("{temp_k:.0} K"),
        ),
    ];
    if !is_sun {
        facts.push(("Orbit (semi-major axis)", format!("{a_au:.3} AU")));
    }
    BodyInfo {
        name: name.to_string(),
        kind: if is_sun {
            BodyKind::Star
        } else {
            BodyKind::Planet
        },
        facts,
    }
}
