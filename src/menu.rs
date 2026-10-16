use crate::camera::CursorLocked;
use crate::search::Search;
use bevy::{app::AppExit, prelude::*, window::CursorGrabMode};

#[derive(Resource, Default)]
pub struct MenuOpen(pub bool);

pub const CATALOG_STARS_LABEL: &str = "Catalog stars (HYG)";

/// Attribution required by the CC BY-SA 4.0 license of the embedded star data.
pub const DATA_CREDIT: &str = "Star data: HYG Database v4.4 (c) astronexus.com, CC BY-SA 4.0\nOrbits: NASA/JPL Solar System Dynamics";

/// Whether the HYG catalog starfield (~109k real stars) is rendered. On by default.
#[derive(Resource)]
pub struct ShowCatalogStars(pub bool);

impl Default for ShowCatalogStars {
    fn default() -> Self {
        Self(true)
    }
}

/// Whether catalog star name labels are shown. Off by default.
#[derive(Resource, Default)]
pub struct ShowStarLabels(pub bool);

#[derive(Component)]
pub struct PauseMenu;

#[derive(Component)]
pub struct QuitButton;

#[derive(Component)]
pub struct CatalogStarsButton;

#[derive(Component)]
pub struct CatalogStarsLabel;

#[derive(Component)]
pub struct StarLabelsButton;

#[derive(Component)]
pub struct StarLabelsLabel;

fn checkbox_button(
    p: &mut ChildSpawnerCommands,
    marker: impl Bundle,
    label_marker: impl Bundle,
    text: &str,
) {
    p.spawn((
        marker,
        Button,
        Node {
            width: Val::Percent(100.0),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(10.0)),
            justify_content: JustifyContent::FlexStart,
            ..default()
        },
        BackgroundColor(Color::srgb(0.18, 0.18, 0.28)),
    ))
    .with_children(|b| {
        b.spawn((
            label_marker,
            Text::new(text),
            TextFont {
                font_size: 17.0,
                ..default()
            },
            TextColor(Color::srgb(0.85, 0.85, 0.85)),
        ));
    });
}

pub fn setup_pause_menu(mut commands: Commands) {
    commands
        .spawn((
            PauseMenu,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(35.0),
                top: Val::Percent(28.0),
                width: Val::Percent(30.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(14.0),
                padding: UiRect::all(Val::Px(28.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.10, 0.94)),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("PAUSED"),
                TextFont {
                    font_size: 30.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));

            checkbox_button(
                p,
                CatalogStarsButton,
                CatalogStarsLabel,
                &checkbox_text(CATALOG_STARS_LABEL, true),
            );
            checkbox_button(p, StarLabelsButton, StarLabelsLabel, "[ ] Star labels");
            p.spawn((
                Text::new(DATA_CREDIT),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.6, 0.7)),
            ));

            p.spawn((
                QuitButton,
                Button,
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(10.0)),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.40, 0.08, 0.08)),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("Quit"),
                    TextFont {
                        font_size: 17.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
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
    if search.active || browser.open {
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

type MenuButtonQuery<'a> = (
    &'a Interaction,
    &'a mut BackgroundColor,
    Option<&'a QuitButton>,
    Option<&'a CatalogStarsButton>,
    Option<&'a StarLabelsButton>,
);

pub fn handle_menu_buttons(
    mut interaction_query: Query<MenuButtonQuery, (Changed<Interaction>, With<Button>)>,
    mut bg_stars: ResMut<ShowCatalogStars>,
    mut star_labels: ResMut<ShowStarLabels>,
    mut exit: EventWriter<AppExit>,
    menu_open: Res<MenuOpen>,
    mut bg_label_query: Query<&mut Text, (With<CatalogStarsLabel>, Without<StarLabelsLabel>)>,
    mut sl_label_query: Query<&mut Text, (With<StarLabelsLabel>, Without<CatalogStarsLabel>)>,
) {
    if !menu_open.0 {
        return;
    }
    for (interaction, mut color, quit, bg_btn, sl_btn) in interaction_query.iter_mut() {
        let is_quit = quit.is_some();
        match *interaction {
            Interaction::Pressed => {
                if is_quit {
                    exit.write(AppExit::Success);
                }
                if bg_btn.is_some() {
                    bg_stars.0 = !bg_stars.0;
                    if let Ok(mut text) = bg_label_query.single_mut() {
                        text.0 = checkbox_text(CATALOG_STARS_LABEL, bg_stars.0);
                    }
                }
                if sl_btn.is_some() {
                    star_labels.0 = !star_labels.0;
                    if let Ok(mut text) = sl_label_query.single_mut() {
                        text.0 = checkbox_text("Star labels", star_labels.0);
                    }
                }
            }
            Interaction::Hovered => {
                *color = BackgroundColor(if is_quit {
                    Color::srgb(0.60, 0.12, 0.12)
                } else {
                    Color::srgb(0.26, 0.26, 0.38)
                });
            }
            Interaction::None => {
                *color = BackgroundColor(if is_quit {
                    Color::srgb(0.40, 0.08, 0.08)
                } else {
                    Color::srgb(0.18, 0.18, 0.28)
                });
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
    fn test_star_labels_off_by_default() {
        assert!(!ShowStarLabels::default().0);
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
