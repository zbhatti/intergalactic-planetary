//! Star browser (N): the nearest stars to the camera, or (Tab) the brightest as seen from
//! here, from the whole HYG catalog plus the Sun. Enter selects the star and turns to face it;
//! G then flies there. Computed once when the panel opens (a ~0.3 ms scan), not every frame.

use crate::browser::{Browser, BrowserActivate, BuildRows, PanelKind, Row, RowAction, StarSort};
use crate::camera::FlyCam;
use crate::coords::WorldPos;
use crate::hyg::HygCatalog;
use crate::hyg_bodies::{HygEntities, promote, star_name};
use crate::labels::apparent_magnitude;
use crate::search::{ObjectCatalog, Search};
use crate::ui::format_distance;
use bevy::math::DVec3;
use bevy::prelude::*;

/// Rows listed.
pub const STAR_ROWS: usize = 40;

/// A listed star: HYG index (`None` for the Sun), distance from the camera (AU), apparent
/// magnitude from the camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listed {
    pub index: Option<usize>,
    pub dist: f64,
    pub mag: f32,
}

/// The `n` nearest or brightest stars seen from `from`, best first. The Sun (visual
/// luminosity 1 at the origin) is included.
pub fn list_stars(catalog: &HygCatalog, from: DVec3, sort: StarSort, n: usize) -> Vec<Listed> {
    let sun_dist = from.length();
    let mut all: Vec<Listed> = catalog
        .stars
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let dist = s.pos.distance(from);
            Listed {
                index: Some(i),
                dist,
                mag: apparent_magnitude(s.lum, dist),
            }
        })
        .chain(std::iter::once(Listed {
            index: None,
            dist: sun_dist,
            mag: apparent_magnitude(1.0, sun_dist),
        }))
        .collect();
    let key = |l: &Listed| match sort {
        StarSort::Nearest => l.dist,
        StarSort::Brightest => l.mag as f64,
    };
    let n = n.min(all.len());
    if n == 0 {
        return Vec::new();
    }
    // Partial selection first: sorting all ~110k would cost several times more.
    all.select_nth_unstable_by(n - 1, |a, b| key(a).total_cmp(&key(b)));
    all.truncate(n);
    all.sort_by(|a, b| key(a).total_cmp(&key(b)));
    all
}

fn display_name(catalog: &HygCatalog, i: usize) -> String {
    let proper = catalog
        .names
        .iter()
        .find(|(k, _)| *k == i)
        .map(|(_, n)| n.as_str());
    star_name(catalog, i, proper)
}

pub fn row_text(name: &str, l: &Listed, class: char) -> String {
    format!(
        "{:<18} {:>11}  mag {:>5.1}  {}",
        name.chars().take(18).collect::<String>(),
        format_distance(l.dist),
        l.mag,
        class
    )
}

pub fn build_star_rows(
    mut browser: ResMut<Browser>,
    catalog: Res<HygCatalog>,
    cam: Query<&WorldPos, With<FlyCam>>,
) {
    let Some(PanelKind::Stars(sort)) = browser.open else {
        return;
    };
    if !browser.dirty {
        return;
    }
    let Ok(cam) = cam.single() else { return };
    let rows = list_stars(&catalog, cam.0, sort, STAR_ROWS)
        .into_iter()
        .map(|l| match l.index {
            Some(i) => Row {
                action: RowAction::SelectStar(i),
                depth: 0,
                text: row_text(&display_name(&catalog, i), &l, catalog.stars[i].class),
            },
            None => Row {
                action: RowAction::SelectStar(usize::MAX),
                depth: 0,
                text: row_text("Sun", &l, 'G'),
            },
        })
        .collect();
    browser.set_rows(rows);
}

/// `SelectStar(usize::MAX)` is the Sun (not in HYG).
pub fn select_star_row(
    mut commands: Commands,
    mut events: EventReader<BrowserActivate>,
    mut search: ResMut<Search>,
    catalog: Res<HygCatalog>,
    objects: Res<ObjectCatalog>,
    mut entities: ResMut<HygEntities>,
) {
    for ev in events.read() {
        let RowAction::SelectStar(i) = ev.0 else {
            continue;
        };
        let (entity, name) = if i == usize::MAX {
            let Some(&(_, sun)) = objects.0.iter().find(|(n, _)| n == "Sun") else {
                continue;
            };
            (sun, "Sun".to_string())
        } else {
            (
                promote(&mut commands, &catalog, &mut entities, i),
                display_name(&catalog, i),
            )
        };
        search.target = Some(entity);
        search.target_name = Some(name);
        search.should_orient = true;
    }
}

pub struct StarBrowserPlugin;

impl Plugin for StarBrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (build_star_rows, select_star_row).in_set(BuildRows));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::AU_PER_PC;

    #[test]
    fn test_nearest_stars_from_home() {
        let cat = HygCatalog::embedded();
        let list = list_stars(&cat, DVec3::new(1.0, 0.0, 0.0), StarSort::Nearest, 5);
        assert_eq!(list[0].index, None, "the Sun first");
        let names: Vec<String> = list[1..]
            .iter()
            .map(|l| display_name(&cat, l.index.unwrap()))
            .collect();
        assert_eq!(names[0], "Proxima Centauri", "{names:?}");
        assert!((list[1].dist / AU_PER_PC - 1.30).abs() < 0.01);
        assert!(names.contains(&"Rigil Kentaurus".to_string()), "{names:?}");
        assert!(list.windows(2).all(|w| w[0].dist <= w[1].dist));
    }

    #[test]
    fn test_brightest_stars_from_home() {
        let cat = HygCatalog::embedded();
        // From 10 AU out of the plane the Sun is still by far the brightest.
        let list = list_stars(&cat, DVec3::new(0.0, 10.0, 0.0), StarSort::Brightest, 3);
        assert_eq!(list[0].index, None);
        assert_eq!(display_name(&cat, list[1].index.unwrap()), "Sirius");
        assert!(list.windows(2).all(|w| w[0].mag <= w[1].mag));
    }

    #[test]
    fn test_from_far_away_the_sun_is_just_another_star() {
        let cat = HygCatalog::embedded();
        let sirius = cat.index_of_name("Sirius").unwrap();
        let from = cat.stars[sirius].pos + DVec3::new(0.0, 1.0, 0.0);
        let list = list_stars(&cat, from, StarSort::Nearest, 3);
        // Sirius first (its companion, Sirius B, is listed separately in HYG).
        assert_eq!(list[0].index, Some(sirius));
        // The Sun is 2.64 pc away: not among the three nearest.
        assert!(list.iter().all(|l| l.index.is_some()));
    }

    #[test]
    fn test_row_text_is_aligned_ascii() {
        let l = Listed {
            index: Some(0),
            dist: 1.3 * AU_PER_PC,
            mag: 11.05,
        };
        let a = row_text("Proxima Centauri", &l, 'M');
        let b = row_text("Sun", &l, 'G');
        assert!(a.is_ascii());
        assert_eq!(a.len(), b.len());
        assert!(a.contains("mag  11.1") || a.contains("mag  11.0"), "{a}");
    }
}
