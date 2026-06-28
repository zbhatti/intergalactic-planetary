use bevy::{
    input::{
        keyboard::{Key, KeyboardInput},
        ButtonState,
    },
    prelude::*,
};
use crate::menu::MenuOpen;

#[derive(Resource, Default)]
pub struct ObjectCatalog(pub Vec<(String, Vec3)>);

#[derive(Resource, Default)]
pub struct Search {
    pub active: bool,
    pub text: String,
    pub selected_idx: usize,
    pub target: Option<Vec3>,
    pub target_name: Option<String>,
    pub should_orient: bool,
}

#[derive(Component)]
pub struct SearchPanel;

#[derive(Component)]
pub struct SearchInputText;

#[derive(Component)]
pub struct SearchResultsText;

pub fn setup_search_ui(mut commands: Commands) {
    commands
        .spawn((
            SearchPanel,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(40.0),
                left: Val::Percent(30.0),
                width: Val::Percent(40.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.12, 0.88)),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                SearchInputText,
                Text::new(""),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            p.spawn((
                SearchResultsText,
                Text::new(""),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.85, 1.0)),
            ));
        });
}

pub fn handle_search(
    mut key_events: EventReader<KeyboardInput>,
    mut search: ResMut<Search>,
    catalog: Res<ObjectCatalog>,
    menu_open: Res<MenuOpen>,
) {
    for event in key_events.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }

        if !search.active {
            if event.logical_key == Key::Enter && !menu_open.0 {
                search.active = true;
                search.text.clear();
                search.selected_idx = 0;
            }
            continue;
        }

        match &event.logical_key {
            Key::Enter => {
                let matches = catalog_matches(&catalog, &search.text);
                if let Some(&(name, pos)) = matches.get(search.selected_idx) {
                    search.target = Some(pos);
                    search.target_name = Some(name.to_string());
                    search.should_orient = true;
                }
                search.active = false;
            }
            Key::Escape => {
                search.active = false;
            }
            Key::ArrowUp => {
                search.selected_idx = search.selected_idx.saturating_sub(1);
            }
            Key::ArrowDown => {
                let count = catalog_matches(&catalog, &search.text).len();
                if search.selected_idx + 1 < count {
                    search.selected_idx += 1;
                }
            }
            Key::Backspace => {
                search.text.pop();
                search.selected_idx = 0;
            }
            Key::Character(c) => {
                search.text.push_str(c.as_str());
                search.selected_idx = 0;
            }
            _ => {}
        }
    }
}

pub fn catalog_matches<'a>(catalog: &'a ObjectCatalog, text: &str) -> Vec<(&'a str, Vec3)> {
    let lower = text.to_lowercase();
    catalog
        .0
        .iter()
        .filter(|(name, _)| name.to_lowercase().contains(&lower))
        .map(|(name, pos)| (name.as_str(), *pos))
        .take(8)
        .collect()
}

pub fn update_search_ui(
    search: Res<Search>,
    catalog: Res<ObjectCatalog>,
    mut panel_query: Query<&mut Visibility, With<SearchPanel>>,
    mut input_query: Query<&mut Text, (With<SearchInputText>, Without<SearchResultsText>)>,
    mut results_query: Query<&mut Text, (With<SearchResultsText>, Without<SearchInputText>)>,
) {
    if let Ok(mut vis) = panel_query.single_mut() {
        *vis = if search.active {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if !search.active {
        return;
    }
    if let Ok(mut text) = input_query.single_mut() {
        text.0 = format!("> {}_", search.text);
    }
    if let Ok(mut text) = results_query.single_mut() {
        let matches = catalog_matches(&catalog, &search.text);
        text.0 = matches
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                if i == search.selected_idx {
                    format!(">  {}", name)
                } else {
                    format!("    {}", name)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Vec3;

    #[test]
    fn test_case_insensitive_match() {
        let catalog = ObjectCatalog(vec![
            ("Earth".to_string(), Vec3::ZERO),
            ("Mars".to_string(), Vec3::ZERO),
        ]);
        let results = catalog_matches(&catalog, "earth");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "Earth");
    }

    #[test]
    fn test_empty_query_returns_all_up_to_8() {
        let entries: Vec<(String, Vec3)> = (0..10)
            .map(|i| (format!("Star{}", i), Vec3::ZERO))
            .collect();
        let catalog = ObjectCatalog(entries);
        let results = catalog_matches(&catalog, "");
        assert_eq!(results.len(), 8);
    }

    #[test]
    fn test_no_match_returns_empty() {
        let catalog = ObjectCatalog(vec![
            ("Earth".to_string(), Vec3::ZERO),
        ]);
        let results = catalog_matches(&catalog, "xyznotfound");
        assert!(results.is_empty());
    }

    #[test]
    fn test_caps_at_8_results() {
        let entries: Vec<(String, Vec3)> = (0..12)
            .map(|i| (format!("Star{}", i), Vec3::ZERO))
            .collect();
        let catalog = ObjectCatalog(entries);
        let results = catalog_matches(&catalog, "star");
        assert_eq!(results.len(), 8);
    }
}
