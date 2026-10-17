//! Saved locations (K): camera position, view direction and selection, kept in a small text
//! file in the user's data directory so they survive restarts.
//!
//! The first row saves the current view (named after the selection); Enter on a saved row
//! jumps there, Delete removes it. Positions are heliocentric f64 AU, written with Rust's
//! shortest round-trip float formatting, so a jump lands exactly where the save was.

use crate::browser::{
    Browser, BrowserActivate, BrowserDelete, BuildRows, PanelKind, Row, RowAction,
};
use crate::camera::FlyCam;
use crate::coords::WorldPos;
use crate::goto::GoTo;
use crate::search::{ObjectCatalog, Search};
use crate::ui::format_distance;
use bevy::math::DVec3;
use bevy::prelude::*;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "locations.tsv";

#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    pub name: String,
    pub pos: DVec3,
    pub yaw: f32,
    pub pitch: f32,
    /// Name of the selected object, re-selected on jump if it still exists.
    pub target: Option<String>,
}

/// One tab-separated line per location: name, x, y, z, yaw, pitch, target.
pub fn format_locations(list: &[Location]) -> String {
    let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
    list.iter()
        .map(|l| {
            format!(
                "{}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{}\n",
                clean(&l.name),
                l.pos.x,
                l.pos.y,
                l.pos.z,
                l.yaw,
                l.pitch,
                l.target.as_deref().map(clean).unwrap_or_default()
            )
        })
        .collect()
}

/// Parses the file, skipping malformed lines (a hand-edited file shouldn't lose the rest).
pub fn parse_locations(text: &str) -> Vec<Location> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            let [name, x, y, z, yaw, pitch, target] = f[..] else {
                return None;
            };
            Some(Location {
                name: name.to_string(),
                pos: DVec3::new(x.parse().ok()?, y.parse().ok()?, z.parse().ok()?),
                yaw: yaw.parse().ok()?,
                pitch: pitch.parse().ok()?,
                target: (!target.is_empty()).then(|| target.to_string()),
            })
        })
        .collect()
}

/// Per-user data directory: `IP_DATA_DIR` if set, else the platform's usual place.
pub fn data_dir() -> Option<PathBuf> {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = env("IP_DATA_DIR") {
        return Some(dir);
    }
    let base = if cfg!(target_os = "windows") {
        env("APPDATA")?
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library/Application Support")
    } else {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local/share")))?
    };
    Some(base.join("intergalactic-planetary"))
}

#[derive(Resource, Default)]
pub struct Locations {
    pub list: Vec<Location>,
    /// Where to persist; `None` keeps them in memory only (tests).
    pub path: Option<PathBuf>,
}

impl Locations {
    pub fn load(path: Option<PathBuf>) -> Self {
        let list = path
            .as_deref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| parse_locations(&t))
            .unwrap_or_default();
        Self { list, path }
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Err(e) = write_file(path, &format_locations(&self.list)) {
            warn!("couldn't save locations to {}: {e}", path.display());
        }
    }
}

fn write_file(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Write then rename, so a crash mid-write can't truncate the saved list.
    let tmp = path.with_extension("tsv.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}

pub fn build_location_rows(mut browser: ResMut<Browser>, locations: Res<Locations>) {
    if !browser.dirty || !browser.is(PanelKind::Locations) {
        return;
    }
    let mut rows = vec![Row {
        action: RowAction::SaveLocation,
        depth: 0,
        text: "+ Save current view".to_string(),
    }];
    rows.extend(locations.list.iter().enumerate().map(|(i, l)| Row {
        action: RowAction::GoToLocation(i),
        depth: 0,
        text: l.name.clone(),
    }));
    browser.set_rows(rows);
}

/// Name for a new location: the selection and the distance to it, else a number.
pub fn location_name(target: Option<(&str, f64)>, count: usize) -> String {
    match target {
        Some((name, dist)) => format!("{name} at {}", format_distance(dist)),
        None => format!("Location {}", count + 1),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn handle_location_rows(
    mut activate: EventReader<BrowserActivate>,
    mut delete: EventReader<BrowserDelete>,
    mut locations: ResMut<Locations>,
    mut search: ResMut<Search>,
    mut goto: ResMut<GoTo>,
    objects: Res<ObjectCatalog>,
    mut cam: Query<(&mut WorldPos, &mut Transform, &mut FlyCam)>,
    positions: Query<&WorldPos, Without<FlyCam>>,
) {
    let Ok((mut pos, mut transform, mut fly)) = cam.single_mut() else {
        return;
    };
    for ev in activate.read() {
        match ev.0 {
            RowAction::SaveLocation => {
                let target = search.target.zip(search.target_name.clone());
                let target_dist = target.as_ref().and_then(|(e, name)| {
                    Some((name.as_str(), positions.get(*e).ok()?.0.distance(pos.0)))
                });
                let name = location_name(target_dist, locations.list.len());
                locations.list.push(Location {
                    name,
                    pos: pos.0,
                    yaw: fly.yaw,
                    pitch: fly.pitch,
                    target: target.map(|(_, n)| n),
                });
                locations.save();
            }
            RowAction::GoToLocation(i) => {
                let Some(l) = locations.list.get(i).cloned() else {
                    continue;
                };
                goto.cancel();
                pos.0 = l.pos;
                fly.yaw = l.yaw;
                fly.pitch = l.pitch;
                transform.rotation = fly.rotation();
                let found = l
                    .target
                    .as_ref()
                    .and_then(|n| objects.0.iter().find(|(name, _)| name == n));
                search.target = found.map(|&(_, e)| e);
                search.target_name = found.map(|(n, _)| n.clone());
            }
            _ => {}
        }
    }
    for ev in delete.read() {
        if let RowAction::GoToLocation(i) = ev.0
            && i < locations.list.len()
        {
            locations.list.remove(i);
            locations.save();
        }
    }
}

pub struct LocationsPlugin;

impl Plugin for LocationsPlugin {
    fn build(&self, app: &mut App) {
        let path = data_dir().map(|d| d.join(FILE_NAME));
        app.insert_resource(Locations::load(path)).add_systems(
            Update,
            (build_location_rows, handle_location_rows).in_set(BuildRows),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Location> {
        vec![
            Location {
                name: "Saturn at 0.0016 AU".into(),
                pos: DVec3::new(1.234_567_890_123_456_7e6, -0.1, 9.000_000_000_000_002),
                yaw: -91.25,
                pitch: 12.5,
                target: Some("Saturn".into()),
            },
            Location {
                name: "Location 2".into(),
                pos: DVec3::new(0.0, 0.002, 0.01),
                yaw: 0.0,
                pitch: 0.0,
                target: None,
            },
        ]
    }

    #[test]
    fn test_round_trip_is_exact() {
        let list = sample();
        assert_eq!(parse_locations(&format_locations(&list)), list);
    }

    #[test]
    fn test_malformed_lines_are_skipped() {
        let mut text = format_locations(&sample());
        text.insert_str(0, "garbage line\nx\t1\t2\n");
        assert_eq!(parse_locations(&text).len(), 2);
    }

    #[test]
    fn test_tabs_in_names_dont_break_the_file() {
        let mut list = sample();
        list[0].name = "a\tb\nc".into();
        let back = parse_locations(&format_locations(&list));
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].name, "a b c");
    }

    #[test]
    fn test_location_names() {
        assert_eq!(location_name(None, 2), "Location 3");
        assert!(location_name(Some(("Saturn", 0.0016)), 0).starts_with("Saturn at "));
    }

    #[test]
    fn test_saves_to_disk_and_loads_back() {
        let dir = std::env::temp_dir().join(format!("ip-locations-test-{}", std::process::id()));
        let path = dir.join(FILE_NAME);
        let mut locs = Locations {
            list: sample(),
            path: Some(path.clone()),
        };
        locs.save();
        assert_eq!(Locations::load(Some(path.clone())).list, sample());
        locs.list.pop();
        locs.save();
        assert_eq!(Locations::load(Some(path)).list.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn app() -> (App, Entity, Entity) {
        use crate::browser::BrowserActivate;
        let mut app = crate::testing::headless_app();
        app.init_resource::<Search>()
            .init_resource::<GoTo>()
            .init_resource::<ObjectCatalog>()
            .init_resource::<Browser>()
            .insert_resource(Locations::default())
            .add_event::<BrowserActivate>()
            .add_event::<BrowserDelete>()
            .add_systems(Update, (build_location_rows, handle_location_rows));
        let saturn = app
            .world_mut()
            .spawn(WorldPos(DVec3::new(9.0, 0.0, 0.0)))
            .id();
        app.world_mut()
            .resource_mut::<ObjectCatalog>()
            .0
            .push(("Saturn".into(), saturn));
        let cam = app
            .world_mut()
            .spawn((
                WorldPos(DVec3::new(9.001, 0.0, 0.0)),
                Transform::default(),
                FlyCam::default(),
            ))
            .id();
        (app, cam, saturn)
    }

    #[test]
    fn test_save_then_jump_back_restores_the_view_and_selection() {
        let (mut app, cam, saturn) = app();
        {
            let mut s = app.world_mut().resource_mut::<Search>();
            s.target = Some(saturn);
            s.target_name = Some("Saturn".into());
        }
        app.world_mut().get_mut::<FlyCam>(cam).unwrap().yaw = 33.0;
        app.world_mut()
            .send_event(BrowserActivate(RowAction::SaveLocation));
        app.update();
        let saved = app.world().resource::<Locations>().list.clone();
        assert_eq!(saved.len(), 1);
        assert!(saved[0].name.starts_with("Saturn at "), "{}", saved[0].name);

        // Wander off and deselect, then jump back.
        app.world_mut().get_mut::<WorldPos>(cam).unwrap().0 = DVec3::splat(1e9);
        app.world_mut().get_mut::<FlyCam>(cam).unwrap().yaw = 0.0;
        app.world_mut().resource_mut::<Search>().target = None;
        app.world_mut()
            .send_event(BrowserActivate(RowAction::GoToLocation(0)));
        app.update();
        assert_eq!(
            app.world().get::<WorldPos>(cam).unwrap().0,
            DVec3::new(9.001, 0.0, 0.0)
        );
        assert_eq!(app.world().get::<FlyCam>(cam).unwrap().yaw, 33.0);
        assert_eq!(app.world().resource::<Search>().target, Some(saturn));

        app.world_mut()
            .send_event(BrowserDelete(RowAction::GoToLocation(0)));
        app.update();
        assert!(app.world().resource::<Locations>().list.is_empty());
    }
}
