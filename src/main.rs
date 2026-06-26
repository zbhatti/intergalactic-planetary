use bevy::prelude::*;

mod camera;
mod constants;
mod navigation;
mod search;
mod solar_system;
mod speed;
mod stars;
mod ui;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Space Engine".into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(camera::CursorLocked(true))
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 200.0,
            ..default()
        })
        .insert_resource(search::Search::default())
        .insert_resource(search::ObjectCatalog::default())
        .insert_resource(speed::SpeedState::default())
        .add_systems(
            Startup,
            (
                camera::setup_camera,
                solar_system::setup_solar_system,
                stars::setup_stars,
                search::setup_search_ui,
                ui::setup_distance_label,
                ui::setup_speed_label,
            ),
        )
        .add_systems(
            Update,
            (
                camera::toggle_cursor,
                speed::cycle_speed,
                search::handle_search,
                navigation::orient_camera_to_target,
                search::update_search_ui,
                ui::update_distance_label,
                ui::update_speed_label,
                camera::fly_camera,
                camera::mouse_look,
                navigation::draw_direction_ring,
                navigation::draw_offscreen_arrow,
            ),
        )
        .run();
}
