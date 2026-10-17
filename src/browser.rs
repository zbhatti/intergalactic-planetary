//! The list panel: solar system browser (B), star browser (N, `star_browser.rs`) and saved
//! locations (K, `locations.rs`). Up/Down move, Enter acts on the row, the panel's key or Esc
//! closes.
//!
//! The solar system tree is the Sun -> planets/dwarf planets -> moons, ordered by orbit size.

use crate::info::BodyInfo;
use crate::search::Search;
use crate::solar_system::OrbitOf;
use bevy::prelude::*;

/// One solar system tree node: entity, tree depth, display name.
pub type TreeNode = (Entity, usize, String);

/// Depth-first tree from (entity, name, orbit) triples: roots (no orbit) first, children of
/// each node ordered by semi-major axis.
pub fn build_tree(bodies: &[(Entity, String, Option<OrbitOf>)]) -> Vec<TreeNode> {
    fn visit(
        node: Entity,
        depth: usize,
        bodies: &[(Entity, String, Option<OrbitOf>)],
        out: &mut Vec<TreeNode>,
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

/// Which list the panel shows. Each has its own hotkey (B, N, K); pressing it again or Esc
/// closes the panel, and another panel's key switches to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelKind {
    SolarSystem,
    Stars(StarSort),
    Locations,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StarSort {
    Nearest,
    Brightest,
}

impl PanelKind {
    fn key(self) -> KeyCode {
        match self {
            PanelKind::SolarSystem => KeyCode::KeyB,
            PanelKind::Stars(_) => KeyCode::KeyN,
            PanelKind::Locations => KeyCode::KeyK,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            PanelKind::SolarSystem => "Solar system  [Up/Down, Enter, B]",
            PanelKind::Stars(StarSort::Nearest) => "Nearest stars  [Tab: brightest, Enter, N]",
            PanelKind::Stars(StarSort::Brightest) => {
                "Brightest stars from here  [Tab: nearest, Enter, N]"
            }
            PanelKind::Locations => "Locations  [Enter: go/save, Del: delete, K]",
        }
    }
}

/// What Enter (or Delete) on a row does; handled by the module that built the rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowAction {
    /// Select an entity and turn to face it.
    Select(Entity),
    /// Select HYG catalog star `i` (promoted to an entity if needed) and turn to face it.
    SelectStar(usize),
    /// Save the current view as a location.
    SaveLocation,
    /// Jump to saved location `i`.
    GoToLocation(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub action: RowAction,
    pub depth: usize,
    pub text: String,
}

/// The list panel, and the keyboard focus: while it's open, search and the pause menu ignore
/// Enter/Esc (see `is_open`).
#[derive(Resource, Default)]
pub struct Browser {
    pub open: Option<PanelKind>,
    pub idx: usize,
    pub rows: Vec<Row>,
    /// Set when `open` changes or the content must be recomputed; the panel's row builder
    /// fills `rows` and clears it.
    pub dirty: bool,
}

impl Browser {
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn is(&self, kind: PanelKind) -> bool {
        self.open == Some(kind)
    }

    /// Replaces the rows, keeping the cursor in range.
    pub fn set_rows(&mut self, rows: Vec<Row>) {
        self.rows = rows;
        self.idx = self.idx.min(self.rows.len().saturating_sub(1));
        self.dirty = false;
    }
}

/// Enter on a row.
#[derive(Event, Clone, Copy, Debug)]
pub struct BrowserActivate(pub RowAction);

/// Delete on a row.
#[derive(Event, Clone, Copy, Debug)]
pub struct BrowserDelete(pub RowAction);

#[derive(Component)]
pub struct BrowserPanel;

/// Rows shown at once; the list scrolls to keep the cursor visible.
const VISIBLE_ROWS: usize = 24;

pub fn setup_browser_ui(mut commands: Commands) {
    commands.spawn((
        BrowserPanel,
        crate::labels::LabelBlocker,
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

const PANEL_KEYS: [PanelKind; 3] = [
    PanelKind::SolarSystem,
    PanelKind::Stars(StarSort::Nearest),
    PanelKind::Locations,
];

pub fn browser_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut browser: ResMut<Browser>,
    search: Res<Search>,
    menu_open: Res<crate::menu::MenuOpen>,
    mut activate: EventWriter<BrowserActivate>,
    mut delete: EventWriter<BrowserDelete>,
) {
    if search.active || menu_open.0 {
        return;
    }
    if let Some(kind) = PANEL_KEYS.into_iter().find(|k| keys.just_pressed(k.key())) {
        let same = browser.open.is_some_and(|o| o.key() == kind.key());
        browser.open = if same { None } else { Some(kind) };
        browser.idx = 0;
        browser.rows.clear();
        browser.dirty = browser.open.is_some();
        return;
    }
    let Some(open) = browser.open else {
        return;
    };
    let current = browser.rows.get(browser.idx).map(|r| r.action);
    if keys.just_pressed(KeyCode::Escape) {
        browser.open = None;
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        browser.idx = browser.idx.saturating_sub(1);
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        browser.idx = (browser.idx + 1).min(browser.rows.len().saturating_sub(1));
    } else if keys.just_pressed(KeyCode::Tab)
        && let PanelKind::Stars(sort) = open
    {
        browser.open = Some(PanelKind::Stars(match sort {
            StarSort::Nearest => StarSort::Brightest,
            StarSort::Brightest => StarSort::Nearest,
        }));
        browser.idx = 0;
        browser.dirty = true;
    } else if keys.just_pressed(KeyCode::Enter)
        && let Some(action) = current
    {
        activate.write(BrowserActivate(action));
        // Saving keeps the list open to show the new entry; everything else closes it.
        if action == RowAction::SaveLocation {
            browser.dirty = true;
        } else {
            browser.open = None;
        }
    // Backspace too: a Mac's "delete" key is Backspace (Delete needs Fn).
    } else if keys.any_just_pressed([KeyCode::Delete, KeyCode::Backspace])
        && let Some(action) = current
    {
        delete.write(BrowserDelete(action));
        browser.dirty = true;
    }
}

/// Fills the solar system tree when that panel opens.
pub fn build_solar_system_rows(
    mut browser: ResMut<Browser>,
    search: Res<Search>,
    bodies: Query<(Entity, &BodyInfo, Option<&OrbitOf>)>,
) {
    if !browser.dirty || !browser.is(PanelKind::SolarSystem) {
        return;
    }
    let list: Vec<_> = bodies
        .iter()
        .map(|(e, info, o)| (e, info.name.clone(), o.copied()))
        .collect();
    let rows: Vec<Row> = build_tree(&list)
        .into_iter()
        .map(|(e, depth, name)| Row {
            action: RowAction::Select(e),
            depth,
            text: name,
        })
        .collect();
    // Start on the current selection if it's in the tree.
    browser.idx = search
        .target
        .and_then(|t| rows.iter().position(|r| r.action == RowAction::Select(t)))
        .unwrap_or(0);
    browser.set_rows(rows);
}

/// Selects an entity row.
pub fn select_entity_row(
    mut events: EventReader<BrowserActivate>,
    mut search: ResMut<Search>,
    infos: Query<&BodyInfo>,
) {
    for ev in events.read() {
        if let RowAction::Select(e) = ev.0 {
            search.target = Some(e);
            search.target_name = infos.get(e).ok().map(|i| i.name.clone());
            search.should_orient = true;
        }
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
    let Some(kind) = browser.open else {
        vis.set_if_neq(Visibility::Hidden);
        return;
    };
    let (start, end) = visible_window(browser.rows.len(), browser.idx, VISIBLE_ROWS);
    let mut lines = vec![kind.title().to_string()];
    for (i, row) in browser.rows[start..end].iter().enumerate() {
        let cursor = if start + i == browser.idx { ">" } else { " " };
        lines.push(format!("{cursor} {}{}", "  ".repeat(row.depth), row.text));
    }
    if browser.rows.is_empty() {
        lines.push("  (empty)".to_string());
    }
    let wanted = lines.join("\n");
    if text.0 != wanted {
        text.0 = wanted;
    }
    vis.set_if_neq(Visibility::Visible);
}

/// Systems that fill rows for a panel: run between input and the UI update.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct BuildRows;

pub struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Browser>()
            .add_event::<BrowserActivate>()
            .add_event::<BrowserDelete>()
            .add_systems(Startup, setup_browser_ui)
            .configure_sets(
                Update,
                BuildRows.after(browser_input).before(update_browser_ui),
            )
            .add_systems(
                Update,
                (
                    // Runs after the systems that also read Esc/Enter, so they see the
                    // browser still open this frame and ignore the key.
                    browser_input
                        .after(crate::menu::toggle_menu)
                        .after(crate::search::handle_search),
                    (build_solar_system_rows, select_entity_row).in_set(BuildRows),
                    update_browser_ui,
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{headless_app, press_key, release_key, run_frames};

    /// The browser's systems without its UI (no `Text` rendering headless).
    struct BrowserPluginHeadless;

    impl Plugin for BrowserPluginHeadless {
        fn build(&self, app: &mut App) {
            app.init_resource::<Browser>()
                .add_event::<BrowserActivate>()
                .add_event::<BrowserDelete>()
                .add_systems(
                    Update,
                    (browser_input, (build_solar_system_rows, select_entity_row)).chain(),
                );
        }
    }

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
            .add_plugins(BrowserPluginHeadless);
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
        assert!(app.world().resource::<Browser>().is(PanelKind::SolarSystem));
        tap(&mut app, KeyCode::ArrowDown); // Mercury
        tap(&mut app, KeyCode::ArrowDown); // Venus
        tap(&mut app, KeyCode::Enter);

        let search = app.world().resource::<Search>();
        assert_eq!(search.target, Some(venus));
        assert!(search.should_orient);
        assert!(!app.world().resource::<Browser>().is_open());
    }

    #[test]
    fn test_panel_keys_switch_and_close() {
        let mut app = headless_app();
        app.init_resource::<Search>()
            .init_resource::<crate::menu::MenuOpen>()
            .add_plugins(BrowserPluginHeadless);
        tap(&mut app, KeyCode::KeyB);
        assert!(app.world().resource::<Browser>().is(PanelKind::SolarSystem));
        tap(&mut app, KeyCode::KeyK);
        assert!(app.world().resource::<Browser>().is(PanelKind::Locations));
        tap(&mut app, KeyCode::KeyN);
        tap(&mut app, KeyCode::Tab);
        assert!(
            app.world()
                .resource::<Browser>()
                .is(PanelKind::Stars(StarSort::Brightest))
        );
        // N closes the star panel whichever sort it shows.
        tap(&mut app, KeyCode::KeyN);
        assert!(!app.world().resource::<Browser>().is_open());
    }

    #[test]
    fn test_delete_and_backspace_send_delete() {
        // A Mac's "delete" key is Backspace; Delete needs Fn there.
        for key in [KeyCode::Delete, KeyCode::Backspace] {
            let mut app = headless_app();
            app.init_resource::<Search>()
                .init_resource::<crate::menu::MenuOpen>()
                .add_plugins(BrowserPluginHeadless);
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
            tap(&mut app, KeyCode::KeyB);
            press_key(&mut app, key);
            run_frames(&mut app, 1);
            let sent = app.world().resource::<Events<BrowserDelete>>().len();
            assert_eq!(sent, 1, "{key:?}");
        }
    }

    #[test]
    fn test_esc_closes_browser_without_opening_menu() {
        let mut app = headless_app();
        app.init_resource::<Search>()
            .init_resource::<crate::menu::MenuOpen>()
            .insert_resource(crate::camera::CursorLocked(true))
            .add_plugins(BrowserPluginHeadless)
            .add_systems(Update, crate::menu::toggle_menu.before(browser_input));
        tap(&mut app, KeyCode::KeyB);
        assert!(app.world().resource::<Browser>().is_open());
        tap(&mut app, KeyCode::Escape);
        assert!(!app.world().resource::<Browser>().is_open());
        assert!(!app.world().resource::<crate::menu::MenuOpen>().0);
        // A second Esc, with the browser closed, opens the menu as usual.
        tap(&mut app, KeyCode::Escape);
        assert!(app.world().resource::<crate::menu::MenuOpen>().0);
    }
}
