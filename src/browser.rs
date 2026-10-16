//! Solar system browser (B): the Sun -> planets/dwarf planets -> moons tree, ordered by
//! orbit size. Up/Down move, Enter selects and turns to face it, B or Esc closes.

use crate::info::BodyInfo;
use crate::search::Search;
use crate::solar_system::OrbitOf;
use bevy::prelude::*;

/// One browser row: entity, tree depth, display name.
pub type Row = (Entity, usize, String);

/// Depth-first tree from (entity, name, orbit) triples: roots (no orbit) first, children of
/// each node ordered by semi-major axis.
pub fn build_tree(bodies: &[(Entity, String, Option<OrbitOf>)]) -> Vec<Row> {
    fn visit(
        node: Entity,
        depth: usize,
        bodies: &[(Entity, String, Option<OrbitOf>)],
        out: &mut Vec<Row>,
    ) {
        let Some((_, name, _)) = bodies.iter().find(|(e, _, _)| *e == node) else {
            return;
        };
        out.push((node, depth, name.clone()));
        let mut children: Vec<_> = bodies
            .iter()
            .filter_map(|(e, _, o)| o.filter(|o| o.primary == node).map(|o| (*e, o.a_au)))
            .collect();
        children.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (child, _) in children {
            visit(child, depth + 1, bodies, out);
        }
    }
    let mut out = Vec::new();
    // Roots: bodies that something orbits but that orbit nothing (the Sun).
    for (e, _, orbit) in bodies {
        if orbit.is_none()
            && bodies
                .iter()
                .any(|(_, _, o)| o.is_some_and(|o| o.primary == *e))
        {
            visit(*e, 0, bodies, &mut out);
        }
    }
    out
}

#[derive(Resource, Default)]
pub struct Browser {
    pub open: bool,
    pub idx: usize,
    pub rows: Vec<Row>,
}

#[derive(Component)]
pub struct BrowserPanel;

/// Rows shown at once; the list scrolls to keep the cursor visible.
const VISIBLE_ROWS: usize = 24;

pub fn setup_browser_ui(mut commands: Commands) {
    commands.spawn((
        BrowserPanel,
        Text::new(""),
        TextFont {
            font_size: 15.0,
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.92, 1.0)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(36.0),
            left: Val::Px(16.0),
            padding: UiRect::all(Val::Px(10.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.1, 0.94)),
        Visibility::Hidden,
        // Panels draw above world-space object labels.
        GlobalZIndex(10),
    ));
}

pub fn browser_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut browser: ResMut<Browser>,
    mut search: ResMut<Search>,
    menu_open: Res<crate::menu::MenuOpen>,
    bodies: Query<(Entity, &BodyInfo, Option<&OrbitOf>)>,
) {
    if search.active || menu_open.0 {
        return;
    }
    if !browser.open {
        if keys.just_pressed(KeyCode::KeyB) {
            let list: Vec<_> = bodies
                .iter()
                .map(|(e, info, o)| (e, info.name.clone(), o.copied()))
                .collect();
            browser.rows = build_tree(&list);
            // Start on the current selection if it's in the tree.
            browser.idx = search
                .target
                .and_then(|t| browser.rows.iter().position(|r| r.0 == t))
                .unwrap_or(0);
            browser.open = true;
        }
        return;
    }
    if keys.just_pressed(KeyCode::KeyB) || keys.just_pressed(KeyCode::Escape) {
        browser.open = false;
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        browser.idx = browser.idx.saturating_sub(1);
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        browser.idx = (browser.idx + 1).min(browser.rows.len().saturating_sub(1));
    } else if keys.just_pressed(KeyCode::Enter)
        && let Some((e, _, name)) = browser.rows.get(browser.idx).cloned()
    {
        search.target = Some(e);
        search.target_name = Some(name);
        search.should_orient = true;
        browser.open = false;
    }
}

/// Rows `[start, end)` to show so that `idx` stays visible.
pub fn visible_window(len: usize, idx: usize, rows: usize) -> (usize, usize) {
    let start = idx.saturating_sub(rows / 2).min(len.saturating_sub(rows));
    (start, (start + rows).min(len))
}

pub fn update_browser_ui(
    browser: Res<Browser>,
    mut panel: Query<(&mut Text, &mut Visibility), With<BrowserPanel>>,
) {
    if !browser.is_changed() {
        return;
    }
    let Ok((mut text, mut vis)) = panel.single_mut() else {
        return;
    };
    if !browser.open {
        vis.set_if_neq(Visibility::Hidden);
        return;
    }
    let (start, end) = visible_window(browser.rows.len(), browser.idx, VISIBLE_ROWS);
    let mut lines = vec!["Solar system  [Up/Down, Enter, B]".to_string()];
    for (i, (_, depth, name)) in browser.rows[start..end].iter().enumerate() {
        let cursor = if start + i == browser.idx { ">" } else { " " };
        lines.push(format!("{cursor} {}{name}", "  ".repeat(*depth)));
    }
    text.0 = lines.join("\n");
    vis.set_if_neq(Visibility::Visible);
}

pub struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Browser>()
            .add_systems(Startup, setup_browser_ui)
            .add_systems(
                Update,
                (
                    // Runs after the systems that also read Esc/Enter, so they see the
                    // browser still open this frame and ignore the key.
                    browser_input
                        .after(crate::menu::toggle_menu)
                        .after(crate::search::handle_search),
                    update_browser_ui,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{headless_app, press_key, release_key, run_frames};

    fn e(i: u32) -> Entity {
        Entity::from_raw(i)
    }

    fn orbit(primary: u32, a_au: f64) -> Option<OrbitOf> {
        Some(OrbitOf {
            primary: e(primary),
            a_au,
        })
    }

    #[test]
    fn test_tree_order_and_depth() {
        let bodies = vec![
            (e(5), "Moon".to_string(), orbit(3, 0.0026)),
            (e(4), "Jupiter".to_string(), orbit(1, 5.2)),
            (e(3), "Earth".to_string(), orbit(1, 1.0)),
            (e(1), "Sun".to_string(), None),
            (e(6), "Io".to_string(), orbit(4, 0.0028)),
            (e(7), "Ganymede".to_string(), orbit(4, 0.0072)),
            (e(9), "Sirius".to_string(), None), // orbits nothing, nothing orbits it
        ];
        let tree = build_tree(&bodies);
        let names: Vec<(usize, &str)> = tree.iter().map(|(_, d, n)| (*d, n.as_str())).collect();
        assert_eq!(
            names,
            vec![
                (0, "Sun"),
                (1, "Earth"),
                (2, "Moon"),
                (1, "Jupiter"),
                (2, "Io"),
                (2, "Ganymede"),
            ]
        );
    }

    #[test]
    fn test_visible_window_keeps_cursor_in_view() {
        assert_eq!(visible_window(10, 3, 24), (0, 10));
        assert_eq!(visible_window(100, 0, 24), (0, 24));
        assert_eq!(visible_window(100, 50, 24), (38, 62));
        assert_eq!(visible_window(100, 99, 24), (76, 100));
    }

    fn tap(app: &mut App, key: KeyCode) {
        press_key(app, key);
        run_frames(app, 1);
        release_key(app, key);
        run_frames(app, 1);
    }

    #[test]
    fn test_browser_select_flow_headless() {
        let mut app = headless_app();
        app.init_resource::<Search>()
            .init_resource::<crate::menu::MenuOpen>()
            .init_resource::<Browser>()
            .add_systems(Update, browser_input);
        let info = |n: &str| BodyInfo {
            name: n.into(),
            kind: crate::info::BodyKind::Planet,
            facts: vec![],
        };
        let sun = app.world_mut().spawn(info("Sun")).id();
        app.world_mut().spawn((
            info("Mercury"),
            OrbitOf {
                primary: sun,
                a_au: 0.39,
            },
        ));
        let venus = app
            .world_mut()
            .spawn((
                info("Venus"),
                OrbitOf {
                    primary: sun,
                    a_au: 0.72,
                },
            ))
            .id();

        tap(&mut app, KeyCode::KeyB);
        assert!(app.world().resource::<Browser>().open);
        tap(&mut app, KeyCode::ArrowDown); // Mercury
        tap(&mut app, KeyCode::ArrowDown); // Venus
        tap(&mut app, KeyCode::Enter);

        let search = app.world().resource::<Search>();
        assert_eq!(search.target, Some(venus));
        assert!(search.should_orient);
        assert!(!app.world().resource::<Browser>().open);
    }

    #[test]
    fn test_esc_closes_browser_without_opening_menu() {
        let mut app = headless_app();
        app.init_resource::<Search>()
            .init_resource::<crate::menu::MenuOpen>()
            .insert_resource(crate::camera::CursorLocked(true))
            .init_resource::<Browser>()
            .add_systems(
                Update,
                (
                    crate::menu::toggle_menu,
                    browser_input.after(crate::menu::toggle_menu),
                ),
            );
        tap(&mut app, KeyCode::KeyB);
        assert!(app.world().resource::<Browser>().open);
        tap(&mut app, KeyCode::Escape);
        assert!(!app.world().resource::<Browser>().open);
        assert!(!app.world().resource::<crate::menu::MenuOpen>().0);
        // A second Esc, with the browser closed, opens the menu as usual.
        tap(&mut app, KeyCode::Escape);
        assert!(app.world().resource::<crate::menu::MenuOpen>().0);
    }
}
