use bevy::prelude::*;
use coords::NavSet;

mod browser;
mod camera;
mod constants;
mod coords;
mod goto;
mod hyg;
mod info;
mod menu;
mod navigation;
mod orbit;
mod perf;
mod search;
mod solar_system;
mod speed;
mod starfield;
mod stars;
#[cfg(test)]
mod testing;
mod ui;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "IP - Intergalactic Planetary".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            coords::CoordsPlugin,
            goto::GoToPlugin,
            info::InfoPlugin,
            browser::BrowserPlugin,
            starfield::StarfieldPlugin,
            perf::PerfPlugin,
        ))
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(camera::CursorLocked(true))
        .insert_resource(camera::ScreenshotCounter::default())
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 200.0,
            ..default()
        })
        .insert_resource(search::Search::default())
        .insert_resource(search::ObjectCatalog::default())
        .insert_resource(speed::SpeedState::default())
        .init_resource::<speed::CurrentSpeed>()
        .insert_resource(stars::ExposureSettings::default())
        .insert_resource(menu::MenuOpen::default())
        .insert_resource(menu::ShowCatalogStars::default())
        .insert_resource(hyg::HygCatalog::embedded())
        .insert_resource(menu::ShowStarLabels::default())
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
                stars::adjust_exposure,
                search::handle_search,
                search::update_search_ui,
                camera::take_screenshot,
            ),
        )
        // Camera movement in world space. GoToPlugin adds its systems to this set too.
        .add_systems(
            Update,
            (
                speed::cycle_speed,
                speed::update_current_speed,
                navigation::orient_camera_to_target,
                camera::mouse_look,
                camera::fly_camera.after(goto::update_goto),
            )
                .in_set(NavSet::Input)
                .after(search::handle_search),
        )
        // Everything that reads final positions for this frame.
        .add_systems(
            Update,
            (
                stars::update_star_brightness,
                solar_system::update_body_visual_size,
                stars::update_star_labels,
                navigation::update_target_reticle,
                navigation::draw_offscreen_arrow,
                ui::update_distance_label,
                ui::update_speed_label,
                ui::update_exposure_label,
            )
                .in_set(NavSet::View),
        )
        .run();
}
