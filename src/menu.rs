use bevy::{app::AppExit, prelude::*, window::CursorGrabMode};
use crate::camera::CursorLocked;
use crate::search::Search;

#[derive(Resource, Default)]
pub struct MenuOpen(pub bool);

/// Whether randomly generated background stars are rendered. Off by default.
#[derive(Resource, Default)]
pub struct ShowBackgroundStars(pub bool);

#[derive(Component)]
pub struct PauseMenu;

#[derive(Component)]
pub struct QuitButton;

#[derive(Component)]
pub struct BgStarsButton;

#[derive(Component)]
pub struct BgStarsLabel;

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
                TextFont { font_size: 30.0, ..default() },
                TextColor(Color::WHITE),
            ));

            p.spawn((
                BgStarsButton,
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
                    BgStarsLabel,
                    Text::new("[ ] Background stars"),
                    TextFont { font_size: 17.0, ..default() },
                    TextColor(Color::srgb(0.85, 0.85, 0.85)),
                ));
            });

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
                    TextFont { font_size: 17.0, ..default() },
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
) {
    if search.active {
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
        *vis = if menu_open.0 { Visibility::Visible } else { Visibility::Hidden };
    }
}

pub fn handle_menu_buttons(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor, Option<&QuitButton>, Option<&BgStarsButton>),
        (Changed<Interaction>, With<Button>),
    >,
    mut bg_stars: ResMut<ShowBackgroundStars>,
    mut exit: EventWriter<AppExit>,
    menu_open: Res<MenuOpen>,
    mut label_query: Query<&mut Text, With<BgStarsLabel>>,
) {
    if !menu_open.0 {
        return;
    }
    for (interaction, mut color, quit, bg_btn) in interaction_query.iter_mut() {
        match *interaction {
            Interaction::Pressed => {
                if quit.is_some() {
                    exit.write(AppExit::Success);
                }
                if bg_btn.is_some() {
                    bg_stars.0 = !bg_stars.0;
                    if let Ok(mut text) = label_query.single_mut() {
                        text.0 = if bg_stars.0 {
                            "[x] Background stars".to_string()
                        } else {
                            "[ ] Background stars".to_string()
                        };
                    }
                }
            }
            Interaction::Hovered => {
                *color = BackgroundColor(if quit.is_some() {
                    Color::srgb(0.60, 0.12, 0.12)
                } else {
                    Color::srgb(0.26, 0.26, 0.38)
                });
            }
            Interaction::None => {
                *color = BackgroundColor(if quit.is_some() {
                    Color::srgb(0.40, 0.08, 0.08)
                } else {
                    Color::srgb(0.18, 0.18, 0.28)
                });
            }
        }
    }
}
