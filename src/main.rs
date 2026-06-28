use bevy::prelude::*;

mod camera;
mod constants;
mod menu;
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
        .insert_resource(stars::ExposureSettings::default())
        .insert_resource(menu::MenuOpen::default())
        .insert_resource(menu::ShowBackgroundStars::default())
        .add_systems(
            Startup,
            (
                camera::setup_camera,
                solar_system::setup_solar_system,
                stars::setup_stars,
                search::setup_search_ui,
                ui::setup_distance_label,
                ui::setup_speed_label,
                ui::setup_exposure_label,
                navigation::setup_target_reticle,
                menu::setup_pause_menu,
            ),
        )
        .add_systems(
            Update,
            (
                menu::toggle_menu,
                menu::update_pause_menu_visibility,
                menu::handle_menu_buttons,
                stars::update_background_star_visibility,
                speed::cycle_speed,
                stars::adjust_exposure,
                search::handle_search,
                navigation::orient_camera_to_target,
                navigation::update_target_reticle,
                search::update_search_ui,
                ui::update_distance_label,
                ui::update_speed_label,
                ui::update_exposure_label,
                stars::update_star_brightness,
                camera::fly_camera,
                camera::mouse_look,
                navigation::draw_offscreen_arrow,
            ),
        )
        .run();
}
