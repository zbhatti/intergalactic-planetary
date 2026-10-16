//! Performance tooling: F3 overlay and a scripted benchmark mode.
//!
//! Benchmark: `IP_BENCH=20 cargo run` flies a scripted go-to tour for 20 seconds,
//! prints one summary line (paste it into PERF.md), saves a mid-run screenshot to
//! `target/ip_bench.png`, then exits. Runs are vsync-capped
//! so they never push the GPU harder than normal play.

use crate::camera::FlyCam;
use crate::coords::{BodyRadius, NavSet, WorldPos};
use crate::goto::{Flight, GoTo};
use crate::menu::{ShowCatalogStars, ShowStarLabels};
use crate::search::{ObjectCatalog, Search};
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use std::time::Instant;

/// Frame times (seconds) for the most recent frames, oldest first.
#[derive(Resource, Default)]
pub struct FrameStats {
    pub recent: std::collections::VecDeque<f32>,
}

const RECENT_FRAMES: usize = 120;

#[derive(Resource, Default)]
pub struct ShowPerfOverlay(pub bool);

#[derive(Component)]
pub struct PerfOverlay;

/// Present only in benchmark mode.
#[derive(Resource)]
pub struct Bench {
    pub duration_secs: f32,
    pub started: Option<Instant>,
    pub frame_ms: Vec<f32>,
    pub screenshot_taken: bool,
    pub tour_idx: usize,
    /// Go-to targets, looped. `IP_BENCH_TOUR=Saturn,Titan` overrides [`BENCH_TOUR`].
    pub tour: Vec<String>,
    /// `IP_BENCH_LOOK=Betelgeuse`: stay home and just turn toward this object (no tour),
    /// for screenshots of the sky as seen from the solar system.
    pub look: Option<String>,
}

impl Bench {
    /// Reads `IP_BENCH=<seconds>` from the environment.
    pub fn from_env() -> Option<Self> {
        let secs: f32 = std::env::var("IP_BENCH").ok()?.parse().ok()?;
        Some(Self {
            duration_secs: secs.max(1.0),
            started: None,
            frame_ms: Vec::new(),
            screenshot_taken: false,
            tour_idx: 0,
            tour: std::env::var("IP_BENCH_TOUR")
                .map(|t| t.split(',').map(|n| n.trim().to_string()).collect())
                .unwrap_or_else(|_| BENCH_TOUR.iter().map(|n| n.to_string()).collect()),
            look: std::env::var("IP_BENCH_LOOK").ok(),
        })
    }
}

/// Summary statistics over a set of frame times in milliseconds.
#[derive(Debug, PartialEq)]
pub struct FrameSummary {
    pub frames: usize,
    pub mean_ms: f32,
    pub p50_ms: f32,
    pub p95_ms: f32,
    pub max_ms: f32,
    /// Frames slower than 1.5x the median — visible hitches.
    pub hitches: usize,
}

pub fn summarize(frame_ms: &[f32]) -> Option<FrameSummary> {
    if frame_ms.is_empty() {
        return None;
    }
    let mut sorted = frame_ms.to_vec();
    sorted.sort_by(f32::total_cmp);
    let pct = |p: f32| sorted[((sorted.len() - 1) as f32 * p).round() as usize];
    let p50 = pct(0.5);
    Some(FrameSummary {
        frames: sorted.len(),
        mean_ms: sorted.iter().sum::<f32>() / sorted.len() as f32,
        p50_ms: p50,
        p95_ms: pct(0.95),
        max_ms: *sorted.last().unwrap(),
        hitches: sorted.iter().filter(|&&t| t > p50 * 1.5).count(),
    })
}

pub fn record_frame_time(time: Res<Time>, mut stats: ResMut<FrameStats>) {
    stats.recent.push_back(time.delta_secs());
    if stats.recent.len() > RECENT_FRAMES {
        stats.recent.pop_front();
    }
}

pub fn setup_perf_overlay(mut commands: Commands) {
    commands.spawn((
        PerfOverlay,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.6, 1.0, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

pub fn toggle_perf_overlay(keys: Res<ButtonInput<KeyCode>>, mut show: ResMut<ShowPerfOverlay>) {
    if keys.just_pressed(KeyCode::F3) {
        show.0 = !show.0;
    }
}

pub fn update_perf_overlay(
    show: Res<ShowPerfOverlay>,
    stats: Res<FrameStats>,
    entities: Query<Entity>,
    mut overlay: Query<(&mut Text, &mut Visibility), With<PerfOverlay>>,
) {
    let Ok((mut text, mut vis)) = overlay.single_mut() else {
        return;
    };
    if !show.0 {
        *vis = Visibility::Hidden;
        return;
    }
    *vis = Visibility::Visible;
    let ms: Vec<f32> = stats.recent.iter().map(|s| s * 1000.0).collect();
    if let Some(s) = summarize(&ms) {
        text.0 = format!(
            "{:.0} fps  {:.1} ms (p95 {:.1}, max {:.1})  {} entities",
            1000.0 / s.mean_ms.max(0.001),
            s.mean_ms,
            s.p95_ms,
            s.max_ms,
            entities.iter().len()
        );
    }
}

/// Benchmark setup: worst-case visibility (all labels and the catalog starfield on).
/// Keys `IP_BENCH_KEYS` can press at startup (e.g. `IP_BENCH_KEYS=B,I`), to put UI panels
/// in the benchmark screenshot.
pub fn bench_key(name: &str) -> Option<KeyCode> {
    Some(match name {
        "B" => KeyCode::KeyB,
        "I" => KeyCode::KeyI,
        "F3" => KeyCode::F3,
        "Esc" => KeyCode::Escape,
        _ => return None,
    })
}

pub fn bench_setup(
    mut bg: ResMut<ShowCatalogStars>,
    mut labels: ResMut<ShowStarLabels>,
    mut overlay: ResMut<ShowPerfOverlay>,
    mut keys: EventWriter<KeyboardInput>,
) {
    bg.0 = true;
    labels.0 = true;
    overlay.0 = true;
    let names = std::env::var("IP_BENCH_KEYS").unwrap_or_default();
    for key_code in names.split(',').filter_map(|n| bench_key(n.trim())) {
        keys.write(KeyboardInput {
            key_code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
    }
}

/// Benchmark tour: go-to flights spanning planet, interplanetary and interstellar scales,
/// so every run exercises adaptive speed, rebasing at high speed, and label projection.
pub const BENCH_TOUR: &[&str] = &["Earth", "Jupiter", "Sirius", "Deneb", "Sun"];

pub fn bench_drive_camera(
    mut bench: ResMut<Bench>,
    catalog: Res<ObjectCatalog>,
    mut search: ResMut<Search>,
    mut goto: ResMut<GoTo>,
    mut cam: Query<(&mut WorldPos, &Transform), With<FlyCam>>,
    targets: Query<(&WorldPos, &BodyRadius), Without<FlyCam>>,
) {
    let Ok((mut look_from, cam_t)) = cam.single_mut() else {
        return;
    };
    if let Some(name) = bench.look.take() {
        bench.tour.clear();
        let Some(&(_, entity)) = catalog.0.iter().find(|(n, _)| *n == name) else {
            return;
        };
        let Ok((target_pos, _)) = targets.get(entity) else {
            return;
        };
        // Stand 1 AU from the Sun on the target's side, so the Sun is behind the camera.
        // (1 AU of parallax is invisible at stellar distances.)
        look_from.0 = target_pos.0.normalize_or_zero() * 1.0;
        search.target = Some(entity);
        search.target_name = Some(name);
        search.should_orient = true;
        return;
    }
    if goto.is_active() {
        return;
    }
    let cam_pos = *look_from;
    if bench.tour.is_empty() {
        return;
    }
    let name = bench.tour[bench.tour_idx % bench.tour.len()].clone();
    bench.tour_idx += 1;
    let Some(&(_, entity)) = catalog.0.iter().find(|(n, _)| *n == name) else {
        return;
    };
    let Ok((target_pos, radius)) = targets.get(entity) else {
        return;
    };
    search.target = Some(entity);
    search.target_name = Some(name);
    goto.0 = Some(Flight::new(
        entity,
        cam_pos.0,
        cam_t.rotation,
        target_pos.0,
        radius.0,
    ));
}

/// Where bench mode saves its mid-run screenshot, for checking a change visually.
pub const BENCH_SCREENSHOT: &str = "target/ip_bench.png";

pub fn bench_record(
    mut commands: Commands,
    mut bench: ResMut<Bench>,
    time: Res<Time>,
    entities: Query<Entity>,
    mut exit: EventWriter<AppExit>,
) {
    // Skip the first second: shader compilation and asset loading aren't steady state.
    let started = *bench.started.get_or_insert_with(Instant::now);
    let elapsed = started.elapsed().as_secs_f32();
    if elapsed < 1.0 {
        return;
    }
    bench.frame_ms.push(time.delta_secs() * 1000.0);
    if !bench.screenshot_taken && elapsed >= 1.0 + bench.duration_secs / 2.0 {
        bench.screenshot_taken = true;
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(BENCH_SCREENSHOT));
    }
    if elapsed < bench.duration_secs + 1.0 {
        return;
    }
    if let Some(s) = summarize(&bench.frame_ms) {
        println!(
            "IP_BENCH frames={} mean={:.2}ms p50={:.2}ms p95={:.2}ms max={:.2}ms hitches={} entities={}",
            s.frames,
            s.mean_ms,
            s.p50_ms,
            s.p95_ms,
            s.max_ms,
            s.hitches,
            entities.iter().len()
        );
        if s.p50_ms > 100.0 {
            println!(
                "IP_BENCH warning: median frame > 100 ms. If the display was blanked or locked, the \
                 compositor throttles presentation and these numbers are invalid. Wake it and rerun."
            );
        }
    }
    exit.write(AppExit::Success);
}

pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FrameStats>()
            .init_resource::<ShowPerfOverlay>()
            .add_systems(Startup, setup_perf_overlay)
            .add_systems(
                Update,
                (record_frame_time, toggle_perf_overlay, update_perf_overlay),
            );
        if let Some(bench) = Bench::from_env() {
            app.insert_resource(bench)
                .add_systems(Startup, bench_setup)
                .add_systems(
                    Update,
                    (
                        bench_drive_camera
                            .in_set(NavSet::Input)
                            .before(crate::goto::update_goto),
                        bench_record,
                    ),
                );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarize_empty_is_none() {
        assert_eq!(summarize(&[]), None);
    }

    #[test]
    fn test_summarize_percentiles_and_hitches() {
        let mut ms = vec![16.0; 99];
        ms.push(40.0);
        let s = summarize(&ms).unwrap();
        assert_eq!(s.frames, 100);
        assert_eq!(s.p50_ms, 16.0);
        assert_eq!(s.p95_ms, 16.0);
        assert_eq!(s.max_ms, 40.0);
        assert_eq!(s.hitches, 1);
    }

    #[test]
    fn test_frame_stats_ring_buffer_is_bounded() {
        let mut app = crate::testing::headless_app();
        app.init_resource::<FrameStats>()
            .add_systems(Update, record_frame_time);
        for _ in 0..(RECENT_FRAMES + 50) {
            app.update();
        }
        let stats = app.world().resource::<FrameStats>();
        assert_eq!(stats.recent.len(), RECENT_FRAMES);
    }
}
