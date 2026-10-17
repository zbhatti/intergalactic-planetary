use bevy::prelude::*;
use coords::NavSet;

mod auto_exposure;
mod browser;
mod camera;
mod constants;
mod coords;
mod dust;
mod exoplanets;
mod goto;
mod gpu_compat;
mod hyg;
mod hyg_bodies;
mod info;
mod labels;
mod locations;
mod menu;
mod navigation;
mod nebulae;
mod orbit;
mod perf;
mod search;
mod sky;
mod solar_system;
mod spectrum;
mod speed;
mod star_browser;
mod starfield;
mod stars;
#[cfg(test)]
mod testing;
mod ui;
mod zoom;

fn main() {
    // Exoplanet hosts HYG doesn't list are appended to the star catalog before anything reads it.
    let mut hyg = hyg::HygCatalog::embedded();
    let exo = exoplanets::load(&mut hyg);
    // Catalog brightnesses are observed (already dimmed by dust): de-redden them once, so the
    // dust between the camera and each star can be applied at runtime (see dust.rs).
    let dust = dust::DustGrid::embedded();
    dust::deredden_catalog(&mut hyg, &dust);
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "IP - Intergalactic Planetary".into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(bevy::render::RenderPlugin {
                    render_creation: gpu_compat::wgpu_settings().into(),
                    ..default()
                }),
        )
        .add_plugins((
            coords::CoordsPlugin,
            goto::GoToPlugin,
            info::InfoPlugin,
            browser::BrowserPlugin,
            dust::DustPlugin,
            starfield::StarfieldPlugin,
            hyg_bodies::HygBodiesPlugin,
            perf::PerfPlugin,
        ))
        // Bevy's plugin tuples hold at most 15.
        .add_plugins((
            zoom::ZoomPlugin,
            labels::LabelsPlugin,
            sky::SkyPlugin,
            auto_exposure::AutoExposurePlugin,
            star_browser::StarBrowserPlugin,
            locations::LocationsPlugin,
            exoplanets::ExoplanetsPlugin,
            nebulae::NebulaePlugin,
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
        .insert_resource(hyg)
        .insert_resource(exo)
        .insert_resource(dust::Dust(dust))
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
                menu::sync_menu_text,
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
