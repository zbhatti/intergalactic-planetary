use bevy::prelude::*;
use crate::menu::MenuOpen;
use crate::search::Search;

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
    const LABELS: [&'static str; 8] = ["x0.1", "x1", "x10", "x100", "x1000", "x10000", "x100000", "x1000000"];

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
