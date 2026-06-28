use bevy::prelude::*;
use crate::camera::FlyCam;
use crate::constants::{C_AU_PER_S, KM_PER_AU, AU_PER_LY};
use crate::search::Search;
use crate::speed::SpeedState;
use crate::stars::ExposureSettings;

#[derive(Component)]
pub struct DistanceLabel;

#[derive(Component)]
pub struct SpeedLabel;

#[derive(Component)]
pub struct ExposureLabel;

pub fn format_distance(dist_au: f32) -> String {
    let ly = dist_au / AU_PER_LY;
    if ly >= 1.0 {
        return format!("{:.2} ly", ly);
    }
    if ly >= 0.01 {
        return format!("{:.3} ly", ly);
    }
    if dist_au >= 0.01 {
        return format!("{:.4} AU", dist_au);
    }
    if dist_au >= 0.0001 {
        return format!("{:.6} AU", dist_au);
    }
    let km = dist_au * KM_PER_AU;
    if km >= 1_000_000.0 {
        format!("{:.0} km", km)
    } else if km >= 1.0 {
        format!("{:.1} km", km)
    } else {
        format!("{:.0} m", km * 1000.0)
    }
}

pub fn format_speed(au_per_sec: f32) -> String {
    let c = au_per_sec / C_AU_PER_S;
    if c >= 1000.0 {
        format!("{:.0}c", c)
    } else if c >= 1.0 {
        format!("{:.2}c", c)
    } else if c >= 0.01 {
        format!("{:.4}c", c)
    } else {
        let km_s = au_per_sec * KM_PER_AU;
        if km_s >= 1.0 {
            format!("{:.0} km/s", km_s)
        } else {
            format!("{:.1} m/s", km_s * 1000.0)
        }
    }
}

pub fn setup_distance_label(mut commands: Commands) {
    commands.spawn((
        DistanceLabel,
        Text::new(""),
        TextFont { font_size: 18.0, ..default() },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(24.0),
            right: Val::Px(24.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

pub fn setup_speed_label(mut commands: Commands) {
    commands.spawn((
        SpeedLabel,
        Text::new(""),
        TextFont { font_size: 18.0, ..default() },
        TextColor(Color::srgb(0.7, 0.9, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(60.0),
            right: Val::Px(24.0),
            ..default()
        },
    ));
}

pub fn setup_exposure_label(mut commands: Commands) {
    commands.spawn((
        ExposureLabel,
        Text::new(""),
        TextFont { font_size: 18.0, ..default() },
        TextColor(Color::srgb(0.9, 0.8, 0.5)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(96.0),
            right: Val::Px(24.0),
            ..default()
        },
    ));
}

pub fn update_distance_label(
    search: Res<Search>,
    cam_query: Query<&Transform, With<FlyCam>>,
    mut label_query: Query<(&mut Text, &mut Visibility), With<DistanceLabel>>,
) {
    let Ok((mut text, mut vis)) = label_query.single_mut() else { return };
    let Some(target) = search.target else {
        *vis = Visibility::Hidden;
        return;
    };
    let Ok(cam) = cam_query.single() else { return };
    let dist_au = (target - cam.translation).length();
    let name = search.target_name.as_deref().unwrap_or("");
    text.0 = format!("{}\n{}", name, format_distance(dist_au));
    *vis = Visibility::Visible;
}

pub fn update_speed_label(
    speed_state: Res<SpeedState>,
    cam_query: Query<&FlyCam>,
    mut label_query: Query<&mut Text, With<SpeedLabel>>,
) {
    let Ok(mut text) = label_query.single_mut() else { return };
    let Ok(cam) = cam_query.single() else { return };
    let au_per_sec = cam.speed * speed_state.multiplier();
    text.0 = format!("{} [{}]", format_speed(au_per_sec), speed_state.label());
}

pub fn update_exposure_label(
    exposure: Res<ExposureSettings>,
    mut label_query: Query<&mut Text, With<ExposureLabel>>,
) {
    let Ok(mut text) = label_query.single_mut() else { return };
    text.0 = format!("{} [ / ]", exposure.label());
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- distance formatting ---

    #[test]
    fn test_format_distance_1ly() {
        assert_eq!(format_distance(AU_PER_LY), "1.00 ly");
    }

    #[test]
    fn test_format_distance_100ly() {
        let result = format_distance(100.0 * AU_PER_LY);
        assert!(result.ends_with("ly"), "got: {}", result);
    }

    #[test]
    fn test_format_distance_1au_contains_au() {
        let result = format_distance(1.0);
        assert!(result.contains("AU"), "Expected AU in result, got: {}", result);
    }

    #[test]
    fn test_format_distance_small_au_contains_km() {
        let result = format_distance(1e-5);
        assert!(result.contains("km"), "Expected km in result, got: {}", result);
    }

    #[test]
    fn test_format_distance_very_small_shows_metres() {
        // < 1 km should fall through to metres
        let result = format_distance(1e-9);
        assert!(result.ends_with(" m"), "Expected metres, got: {}", result);
    }

    #[test]
    fn test_format_distance_zero_does_not_panic() {
        let _ = format_distance(0.0);
    }

    // --- speed formatting ---

    #[test]
    fn test_format_speed_at_c() {
        assert_eq!(format_speed(C_AU_PER_S), "1.00c");
    }

    #[test]
    fn test_format_speed_1000c() {
        assert_eq!(format_speed(1000.0 * C_AU_PER_S), "1000c");
    }

    #[test]
    fn test_format_speed_sub_c_shows_km_per_s() {
        // c ≈ 0.002004 AU/s; 0.01c threshold ≈ 0.00002 AU/s
        // Use 1e-5 AU/s (≈ 0.005c) which falls below 0.01c into km/s range.
        let result = format_speed(1e-5);
        assert!(result.contains("km/s"), "Expected km/s, got: {}", result);
    }

    #[test]
    fn test_format_speed_zero_does_not_panic() {
        let _ = format_speed(0.0);
    }

    // --- ASCII-only regression (Bevy's default font has no multibyte glyphs) ---

    #[test]
    fn test_format_distance_is_ascii_only() {
        for dist in [0.0, 1e-9, 1e-5, 0.001, 1.0, 100.0, AU_PER_LY, 1000.0 * AU_PER_LY] {
            let s = format_distance(dist);
            assert!(s.is_ascii(), "format_distance({}) produced non-ASCII: {:?}", dist, s);
        }
    }

    #[test]
    fn test_format_speed_is_ascii_only() {
        for speed in [0.0, 0.001, C_AU_PER_S * 0.5, C_AU_PER_S, C_AU_PER_S * 1000.0] {
            let s = format_speed(speed);
            assert!(s.is_ascii(), "format_speed({}) produced non-ASCII: {:?}", speed, s);
        }
    }
}
