//! Distance-adaptive flight speed: proportional to the distance to the nearest body's
//! surface, so a second of flight covers a similar fraction of the remaining gap whether
//! you're skimming a planet or crossing between galaxies. Shift cycles a throttle on top.

use crate::coords::Nearest;
use crate::menu::MenuOpen;
use crate::search::Search;
use bevy::prelude::*;

/// Speed floor: never slower than this many AU of nearest-surface distance (~150 m).
pub const MIN_SPEED_DIST_AU: f64 = 1.0e-9;
/// Speed ceiling distance (~5 Gpc), so an empty scene can't produce infinite speed.
pub const MAX_SPEED_DIST_AU: f64 = 1.0e15;

/// Speed in AU/s for a given nearest-surface distance and throttle.
pub fn adaptive_speed(nearest_surface_au: f64, throttle: f64) -> f64 {
    nearest_surface_au.clamp(MIN_SPEED_DIST_AU, MAX_SPEED_DIST_AU) * throttle
}

#[derive(Resource)]
pub struct SpeedState {
    pub idx: usize,
}

impl Default for SpeedState {
    fn default() -> Self {
        Self { idx: 2 }
    }
}

impl SpeedState {
    const THROTTLES: [f64; 6] = [0.1, 0.3, 1.0, 3.0, 10.0, 30.0];
    const LABELS: [&'static str; 6] = ["x0.1", "x0.3", "x1", "x3", "x10", "x30"];

    pub fn throttle(&self) -> f64 {
        Self::THROTTLES[self.idx]
    }

    pub fn label(&self) -> &'static str {
        Self::LABELS[self.idx]
    }

    pub fn cycle(&mut self) {
        self.idx = (self.idx + 1) % Self::THROTTLES.len();
    }
}

/// Current flight speed in AU/s, recomputed every frame.
#[derive(Resource, Default)]
pub struct CurrentSpeed {
    pub au_per_sec: f64,
}

pub fn cycle_speed(
    keys: Res<ButtonInput<KeyCode>>,
    search: Res<Search>,
    menu_open: Res<MenuOpen>,
    mut speed_state: ResMut<SpeedState>,
) {
    if search.active || menu_open.0 {
        return;
    }
    if keys.just_pressed(KeyCode::ShiftLeft) || keys.just_pressed(KeyCode::ShiftRight) {
        speed_state.cycle();
    }
}

pub fn update_current_speed(
    nearest: Res<Nearest>,
    speed_state: Res<SpeedState>,
    mut speed: ResMut<CurrentSpeed>,
) {
    speed.au_per_sec = adaptive_speed(nearest.surface_dist, speed_state.throttle());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{headless_app, press_key, release_key, run_frames};

    fn speed_app() -> App {
        let mut app = headless_app();
        app.init_resource::<SpeedState>()
            .init_resource::<Search>()
            .init_resource::<MenuOpen>()
            .add_systems(Update, cycle_speed);
        app
    }

    #[test]
    fn test_shift_tap_cycles_speed_once() {
        let mut app = speed_app();
        let start = app.world().resource::<SpeedState>().idx;
        press_key(&mut app, KeyCode::ShiftLeft);
        run_frames(&mut app, 5); // held for several frames: still one step
        release_key(&mut app, KeyCode::ShiftLeft);
        run_frames(&mut app, 1);
        assert_eq!(app.world().resource::<SpeedState>().idx, start + 1);
    }

    #[test]
    fn test_throttle_wraps_around() {
        let mut s = SpeedState::default();
        for _ in 0..SpeedState::THROTTLES.len() {
            s.cycle();
        }
        assert_eq!(s.idx, SpeedState::default().idx);
    }

    #[test]
    fn test_adaptive_speed_is_proportional_to_distance() {
        assert!((adaptive_speed(2.0, 1.0) / adaptive_speed(1.0, 1.0) - 2.0).abs() < 1e-12);
        assert!((adaptive_speed(1.0, 3.0) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn test_adaptive_speed_is_clamped() {
        assert_eq!(adaptive_speed(0.0, 1.0), MIN_SPEED_DIST_AU);
        assert_eq!(adaptive_speed(f64::INFINITY, 1.0), MAX_SPEED_DIST_AU);
    }

    #[test]
    fn test_shift_ignored_while_menu_open() {
        let mut app = speed_app();
        app.world_mut().resource_mut::<MenuOpen>().0 = true;
        let start = app.world().resource::<SpeedState>().idx;
        press_key(&mut app, KeyCode::ShiftLeft);
        run_frames(&mut app, 1);
        assert_eq!(app.world().resource::<SpeedState>().idx, start);
    }
}
