//! Helpers for headless app-level tests: real Bevy schedules and systems,
//! no window or GPU, and a fixed 60 Hz clock so results never depend on wall time.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use std::time::Duration;

pub const FRAME: Duration = Duration::from_micros(16_667);

/// A minimal app with input and transforms; each `update()` advances time by one 60 Hz frame.
pub fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, bevy::input::InputPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    // First update initialises Time; later updates each advance exactly one FRAME.
    app.update();
    app
}

/// Runs `frames` updates.
pub fn run_frames(app: &mut App, frames: u32) {
    for _ in 0..frames {
        app.update();
    }
}

/// Sends a real key-press event through Bevy's input pipeline, as a keyboard would.
/// The key stays held (`pressed`) until [`release_key`]; `just_pressed` lasts one update.
pub fn press_key(app: &mut App, key: KeyCode) {
    send_key(app, key, ButtonState::Pressed);
}

pub fn release_key(app: &mut App, key: KeyCode) {
    send_key(app, key, ButtonState::Released);
}

fn send_key(app: &mut App, key_code: KeyCode, state: ButtonState) {
    app.world_mut().send_event(KeyboardInput {
        key_code,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_headless_clock_is_deterministic() {
        let mut app = headless_app();
        let before = app.world().resource::<Time>().elapsed_secs_f64();
        run_frames(&mut app, 60);
        let after = app.world().resource::<Time>().elapsed_secs_f64();
        assert!(
            ((after - before) - 1.0).abs() < 1e-3,
            "60 frames should be ~1 s, got {}",
            after - before
        );
    }
}
