use crate::camera::CursorLocked;
use crate::dust::DustSettings;
use crate::labels::LabelSettings;
use crate::nebulae::ShowNebulae;
use crate::search::Search;
use crate::sky::SkySettings;
use crate::stars::ExposureSettings;
use bevy::{app::AppExit, prelude::*, window::CursorGrabMode};

#[derive(Resource, Default)]
pub struct MenuOpen(pub bool);

pub const CATALOG_STARS_LABEL: &str = "Catalog stars (HYG)";

/// Attribution required by the CC BY-SA 4.0 licenses of the embedded star and constellation data.
pub const DATA_CREDIT: &str = "Star data: HYG Database v4.4 (c) astronexus.com, CC BY-SA 4.0\nConstellation figures: Stellarium (modern sky culture), CC BY-SA 4.0\nExoplanets: NASA Exoplanet Archive (Caltech, under contract with NASA)\nNebulae: Sharpless, van den Bergh, Mendez-Delgado+ 2022 et al. via VizieR (CDS)\n3D dust: Edenhofer et al. 2024 (CC BY 4.0)\nOrbits: NASA/JPL Solar System Dynamics";

/// Whether the HYG catalog starfield (~109k real stars) is rendered. On by default.
#[derive(Resource)]
pub struct ShowCatalogStars(pub bool);

impl Default for ShowCatalogStars {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Component)]
pub struct PauseMenu;

/// A pause-menu button. Its text is regenerated from the settings whenever they change, so it
/// stays right however the setting was changed (menu click, hotkey, bench setup).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    CatalogStars,
    Nebulae,
    Labels,
    PlanetLabels,
    MoonLabels,
    SmallBodyLabels,
    NebulaLabels,
    StarLabels,
    StarMagLimit,
    Constellations,
    EquatorialGrid,
    EclipticGrid,
    GalacticGrid,
    AutoExposure,
    Dust,
    Quit,
}

/// The settings the menu edits.
pub struct MenuSettings<'a> {
    pub stars: &'a mut ShowCatalogStars,
    pub labels: &'a mut LabelSettings,
    pub sky: &'a mut SkySettings,
    pub exposure: &'a mut ExposureSettings,
    pub nebulae: &'a mut ShowNebulae,
    pub dust: &'a mut DustSettings,
}

impl MenuItem {
    pub const TOGGLES: [MenuItem; 15] = [
        MenuItem::CatalogStars,
        MenuItem::Nebulae,
        MenuItem::Labels,
        MenuItem::PlanetLabels,
        MenuItem::MoonLabels,
        MenuItem::SmallBodyLabels,
        MenuItem::NebulaLabels,
        MenuItem::StarLabels,
        MenuItem::StarMagLimit,
        MenuItem::Constellations,
        MenuItem::EquatorialGrid,
        MenuItem::EclipticGrid,
        MenuItem::GalacticGrid,
        MenuItem::AutoExposure,
        MenuItem::Dust,
    ];

    pub fn text(
        self,
        stars: &ShowCatalogStars,
        labels: &LabelSettings,
        sky: &SkySettings,
        exposure: &ExposureSettings,
        nebulae: &ShowNebulae,
        dust: &DustSettings,
    ) -> String {
        match self {
            MenuItem::CatalogStars => checkbox_text(CATALOG_STARS_LABEL, stars.0),
            MenuItem::Nebulae => checkbox_text("Nebulae", nebulae.0),
            MenuItem::Labels => checkbox_text("Labels (L)", labels.enabled),
            MenuItem::PlanetLabels => checkbox_text("  Planet labels", labels.planets),
            MenuItem::MoonLabels => checkbox_text("  Moon labels", labels.moons),
            MenuItem::SmallBodyLabels => {
                checkbox_text("  Asteroid and comet labels", labels.small_bodies)
            }
            MenuItem::NebulaLabels => checkbox_text("  Nebula labels", labels.nebulae),
            MenuItem::StarLabels => checkbox_text("  Star labels", labels.stars),
            MenuItem::StarMagLimit => {
                format!("  Label stars brighter than mag {}", labels.star_mag_limit)
            }
            MenuItem::Constellations => checkbox_text("Constellations (C)", sky.constellations),
            MenuItem::EquatorialGrid => checkbox_text("Equatorial grid", sky.equatorial_grid),
            MenuItem::EclipticGrid => checkbox_text("Ecliptic grid", sky.ecliptic_grid),
            MenuItem::GalacticGrid => checkbox_text("Galactic grid", sky.galactic_grid),
            MenuItem::AutoExposure => checkbox_text("Auto exposure (\\)", exposure.auto),
            MenuItem::Dust => checkbox_text("Interstellar dust (dims, reddens)", dust.enabled),
            MenuItem::Quit => "Quit".to_string(),
        }
    }

    pub fn apply(self, s: MenuSettings) {
        let MenuSettings {
            stars,
            labels,
            sky,
            exposure,
            nebulae,
            dust,
        } = s;
        match self {
            MenuItem::CatalogStars => stars.0 = !stars.0,
            MenuItem::Nebulae => nebulae.0 = !nebulae.0,
            MenuItem::Labels => labels.enabled = !labels.enabled,
            MenuItem::PlanetLabels => labels.planets = !labels.planets,
            MenuItem::MoonLabels => labels.moons = !labels.moons,
            MenuItem::SmallBodyLabels => labels.small_bodies = !labels.small_bodies,
            MenuItem::NebulaLabels => labels.nebulae = !labels.nebulae,
            MenuItem::StarLabels => labels.stars = !labels.stars,
            MenuItem::StarMagLimit => labels.star_mag_limit = labels.next_mag_limit(),
            MenuItem::Constellations => sky.constellations = !sky.constellations,
            MenuItem::EquatorialGrid => sky.equatorial_grid = !sky.equatorial_grid,
            MenuItem::EclipticGrid => sky.ecliptic_grid = !sky.ecliptic_grid,
            MenuItem::GalacticGrid => sky.galactic_grid = !sky.galactic_grid,
            MenuItem::AutoExposure => exposure.auto = !exposure.auto,
            MenuItem::Dust => dust.enabled = !dust.enabled,
            MenuItem::Quit => {}
        }
    }
}

/// On a button's text: which item it shows.
#[derive(Component)]
pub struct MenuItemText(pub MenuItem);

const BUTTON_BG: Color = Color::srgb(0.18, 0.18, 0.28);
const BUTTON_HOVER: Color = Color::srgb(0.26, 0.26, 0.38);
const QUIT_BG: Color = Color::srgb(0.40, 0.08, 0.08);
const QUIT_HOVER: Color = Color::srgb(0.60, 0.12, 0.12);

fn menu_button(p: &mut ChildSpawnerCommands, item: MenuItem) {
    let quit = item == MenuItem::Quit;
    p.spawn((
        item,
        Button,
        Node {
            width: Val::Percent(100.0),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
            justify_content: if quit {
                JustifyContent::Center
            } else {
                JustifyContent::FlexStart
            },
            ..default()
        },
        BackgroundColor(if quit { QUIT_BG } else { BUTTON_BG }),
    ))
    .with_children(|b| {
        b.spawn((
            MenuItemText(item),
            Text::new(""),
            TextFont {
                font_size: 15.0,
                ..default()
            },
            TextColor(if quit {
                Color::WHITE
            } else {
                Color::srgb(0.85, 0.85, 0.85)
            }),
        ));
    });
}

/// Toggles in the left column (catalog and labels); the rest go on the right (sky and camera).
const LEFT_COLUMN: usize = 9;

pub fn setup_pause_menu(mut commands: Commands) {
    commands
        .spawn((
            PauseMenu,
            crate::labels::LabelBlocker,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(10.0),
                top: Val::Percent(6.0),
                width: Val::Percent(80.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(16.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.10, 0.94)),
            // Above the other panels (info, browser, search at 10).
            GlobalZIndex(20),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("PAUSED"),
                TextFont {
                    font_size: 26.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            // Two columns, so the menu (and the license credits below it) fit a 720 px window.
            p.spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(12.0),
                ..default()
            })
            .with_children(|row| {
                for items in [
                    &MenuItem::TOGGLES[..LEFT_COLUMN],
                    &MenuItem::TOGGLES[LEFT_COLUMN..],
                ] {
                    row.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        flex_grow: 1.0,
                        flex_basis: Val::Px(0.0),
                        row_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|col| {
                        for &item in items {
                            menu_button(col, item);
                        }
                    });
                }
            });
            p.spawn((
                Text::new(DATA_CREDIT),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.6, 0.7)),
            ));
            menu_button(p, MenuItem::Quit);
        });
}

pub fn toggle_menu(
    keys: Res<ButtonInput<KeyCode>>,
    search: Res<Search>,
    mut menu_open: ResMut<MenuOpen>,
    mut locked: ResMut<CursorLocked>,
    mut windows: Query<&mut Window>,
    browser: Res<crate::browser::Browser>,
) {
    if search.active || browser.is_open() {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        menu_open.0 = !menu_open.0;
        locked.0 = !menu_open.0;
        if let Ok(mut window) = windows.single_mut() {
            window.cursor_options.grab_mode = if locked.0 {
                CursorGrabMode::Locked
            } else {
                CursorGrabMode::None
            };
            window.cursor_options.visible = !locked.0;
        }
    }
}

pub fn update_pause_menu_visibility(
    menu_open: Res<MenuOpen>,
    mut menu_query: Query<&mut Visibility, With<PauseMenu>>,
) {
    if !menu_open.is_changed() {
        return;
    }
    if let Ok(mut vis) = menu_query.single_mut() {
        *vis = if menu_open.0 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// Rewrites the menu texts when a setting changes (and once at startup).
#[allow(clippy::too_many_arguments)]
pub fn sync_menu_text(
    stars: Res<ShowCatalogStars>,
    labels: Res<LabelSettings>,
    sky: Res<SkySettings>,
    exposure: Res<ExposureSettings>,
    nebulae: Res<ShowNebulae>,
    dust: Res<DustSettings>,
    mut texts: Query<(&MenuItemText, &mut Text)>,
    mut last_auto: Local<Option<bool>>,
) {
    // Exposure changes every frame while auto-exposure adapts; only the on/off matters here.
    let exposure_toggled = exposure.is_changed() && *last_auto != Some(exposure.auto);
    *last_auto = Some(exposure.auto);
    if !stars.is_changed()
        && !labels.is_changed()
        && !sky.is_changed()
        && !exposure_toggled
        && !nebulae.is_changed()
        && !dust.is_changed()
    {
        return;
    }
    for (item, mut text) in &mut texts {
        let wanted = item
            .0
            .text(&stars, &labels, &sky, &exposure, &nebulae, &dust);
        if text.0 != wanted {
            text.0 = wanted;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn handle_menu_buttons(
    mut buttons: Query<(&Interaction, &mut BackgroundColor, &MenuItem), Changed<Interaction>>,
    mut stars: ResMut<ShowCatalogStars>,
    mut labels: ResMut<LabelSettings>,
    mut sky: ResMut<SkySettings>,
    mut exposure: ResMut<ExposureSettings>,
    mut nebulae: ResMut<ShowNebulae>,
    mut dust: ResMut<DustSettings>,
    mut exit: EventWriter<AppExit>,
    menu_open: Res<MenuOpen>,
) {
    if !menu_open.0 {
        return;
    }
    for (interaction, mut color, &item) in &mut buttons {
        let quit = item == MenuItem::Quit;
        match *interaction {
            Interaction::Pressed => {
                if quit {
                    exit.write(AppExit::Success);
                }
                item.apply(MenuSettings {
                    stars: &mut stars,
                    labels: &mut labels,
                    sky: &mut sky,
                    exposure: &mut exposure,
                    nebulae: &mut nebulae,
                    dust: &mut dust,
                });
            }
            Interaction::Hovered => {
                *color = BackgroundColor(if quit { QUIT_HOVER } else { BUTTON_HOVER });
            }
            Interaction::None => {
                *color = BackgroundColor(if quit { QUIT_BG } else { BUTTON_BG });
            }
        }
    }
}

fn checkbox_text(label: &str, checked: bool) -> String {
    if checked {
        format!("[x] {}", label)
    } else {
        format!("[ ] {}", label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_stars_on_by_default() {
        assert!(ShowCatalogStars::default().0);
    }

    #[test]
    fn test_menu_items_toggle_their_settings() {
        let mut stars = ShowCatalogStars::default();
        let mut labels = LabelSettings::default();
        let mut sky = SkySettings::default();
        let mut exposure = ExposureSettings::default();
        let mut nebulae = ShowNebulae::default();
        let mut dust = DustSettings::default();
        assert_eq!(
            MenuItem::MoonLabels.text(&stars, &labels, &sky, &exposure, &nebulae, &dust),
            "[x]   Moon labels"
        );
        MenuItem::MoonLabels.apply(MenuSettings {
            stars: &mut stars,
            labels: &mut labels,
            sky: &mut sky,
            exposure: &mut exposure,
            nebulae: &mut nebulae,
            dust: &mut dust,
        });
        assert!(!labels.moons);
        assert_eq!(
            MenuItem::MoonLabels.text(&stars, &labels, &sky, &exposure, &nebulae, &dust),
            "[ ]   Moon labels"
        );
        MenuItem::CatalogStars.apply(MenuSettings {
            stars: &mut stars,
            labels: &mut labels,
            sky: &mut sky,
            exposure: &mut exposure,
            nebulae: &mut nebulae,
            dust: &mut dust,
        });
        MenuItem::GalacticGrid.apply(MenuSettings {
            stars: &mut stars,
            labels: &mut labels,
            sky: &mut sky,
            exposure: &mut exposure,
            nebulae: &mut nebulae,
            dust: &mut dust,
        });
        assert!(sky.galactic_grid);
        assert!(!stars.0);
        for item in MenuItem::TOGGLES {
            assert!(
                item.text(&stars, &labels, &sky, &exposure, &nebulae, &dust)
                    .is_ascii()
            );
        }
    }

    #[test]
    fn test_menu_text_follows_settings_changed_elsewhere() {
        // e.g. the L hotkey or bench setup: the menu must not show a stale checkbox.
        let mut app = crate::testing::headless_app();
        app.init_resource::<ShowCatalogStars>()
            .init_resource::<LabelSettings>()
            .init_resource::<SkySettings>()
            .init_resource::<ExposureSettings>()
            .init_resource::<ShowNebulae>()
            .init_resource::<DustSettings>()
            .add_systems(Update, sync_menu_text);
        let text = app
            .world_mut()
            .spawn((MenuItemText(MenuItem::Labels), Text::new("")))
            .id();
        app.update();
        assert_eq!(app.world().get::<Text>(text).unwrap().0, "[x] Labels (L)");
        app.world_mut().resource_mut::<LabelSettings>().enabled = false;
        app.update();
        assert_eq!(app.world().get::<Text>(text).unwrap().0, "[ ] Labels (L)");
    }

    #[test]
    fn test_menu_closed_by_default() {
        assert!(!MenuOpen::default().0);
    }

    #[test]
    fn test_checkbox_text_checked() {
        assert_eq!(checkbox_text("Star labels", true), "[x] Star labels");
    }

    #[test]
    fn test_checkbox_text_unchecked() {
        assert_eq!(checkbox_text("Star labels", false), "[ ] Star labels");
    }

    #[test]
    fn test_checkbox_text_is_ascii_only() {
        assert!(checkbox_text(CATALOG_STARS_LABEL, true).is_ascii());
        assert!(checkbox_text(CATALOG_STARS_LABEL, false).is_ascii());
        assert!(DATA_CREDIT.is_ascii());
    }
}
