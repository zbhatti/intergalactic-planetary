use crate::menu::MenuOpen;
use crate::search::Search;
use bevy::prelude::*;

#[derive(Resource)]
pub struct SpeedState {
    pub idx: usize,
}

impl Default for SpeedState {
    fn default() -> Self {
        Self { idx: 1 }
    }
}

impl SpeedState {
    const MULTIPLIERS: [f32; 8] = [0.1, 1.0, 10.0, 100.0, 1000.0, 10000.0, 100000.0, 1000000.0];
    const LABELS: [&'static str; 8] = [
        "x0.1", "x1", "x10", "x100", "x1000", "x10000", "x100000", "x1000000",
    ];

    pub fn multiplier(&self) -> f32 {
        Self::MULTIPLIERS[self.idx]
    }

    pub fn label(&self) -> &'static str {
        Self::LABELS[self.idx]
    }

    pub fn cycle(&mut self) {
        self.idx = (self.idx + 1) % 8;
    }
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
    fn test_shift_ignored_while_menu_open() {
        let mut app = speed_app();
        app.world_mut().resource_mut::<MenuOpen>().0 = true;
        let start = app.world().resource::<SpeedState>().idx;
        press_key(&mut app, KeyCode::ShiftLeft);
        run_frames(&mut app, 1);
        assert_eq!(app.world().resource::<SpeedState>().idx, start);
    }
}
