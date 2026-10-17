//! Catalogued nebulae: Sharpless H II regions (emission) and van den Bergh reflection nebulae,
//! with colours computed from their visible spectra (`spectrum.rs`).
//!
//! Data: `data/nebulae.tsv` from `tools/build_nebulae.py` (Sharpless 1959, van den Bergh 1966,
//! distances from Gaia parallaxes and other published catalogs; regions without a distance are
//! left out, not guessed).
//!
//! Each nebula is an emissive sphere ray-traced in `nebula.wgsl`, at its catalogued angular size
//! and distance. Brightness is the catalog's surface brightness on the same scale as the stars, so
//! at the default exposure nebulae are as faint as they are to the eye; raise the exposure
//! (`]`) as a long-exposure camera would. Surface brightness is estimated from the catalogs'
//! brightness classes (approximate).

use crate::constants::AU_PER_PC;
use crate::coords::{BodyRadius, NavSet, NotASurface, WorldPos};
use crate::hyg::HygCatalog;
use crate::hyg_bodies::class_temperature;
use crate::info::{BodyInfo, BodyKind, format_sig};
use crate::labels::spawn_label;
use crate::search::ObjectCatalog;
use crate::sky::SkyFrame;
use crate::spectrum::{DUST_SCATTER_EXP, HII_LINES, lines_rgb, scattered_starlight_rgb};
use crate::stars::{BRIGHTNESS_SCALE, ExposureSettings};
use crate::zoom::{CameraFov, min_star_angle};
use bevy::asset::{load_internal_asset, weak_handle};
use bevy::math::DVec3;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::mesh::{
    Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology,
};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    AsBindGroup, Face, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError,
    VertexFormat,
};

static NEBULAE_TSV: &str = include_str!("../data/nebulae.tsv");

const NEBULA_SHADER: Handle<Shader> = weak_handle!("3f0e2c71-9a41-4d8e-8f6b-6a2f1d7b4c55");

/// Per-vertex: linear colour x central surface brightness (rgb), radius in AU (a).
pub const ATTRIBUTE_NEBULA: MeshVertexAttribute =
    MeshVertexAttribute::new("Nebula", 7_301_225_119, VertexFormat::Float32x4);

/// One square arcsecond in steradians.
const ARCSEC2_SR: f64 = 2.350_443_053_909_51e-11;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NebulaKind {
    Emission,
    Reflection,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NebulaRecord {
    pub kind: NebulaKind,
    pub name: String,
    /// Catalog designation when the display name is a common name (e.g. "Sh2-281" for M42).
    pub alias: Option<String>,
    pub glon: f64,
    pub glat: f64,
    pub dist_pc: f64,
    pub radius_arcmin: f64,
    /// Emission: Sharpless brightness class 1-3; reflection: van den Bergh 1 (vF) - 5 (vBr).
    pub class: u8,
    pub dist_source: String,
    /// Reflection: the illuminating star's Hipparcos number (0 if none).
    pub hip: u32,
    /// Reflection: van den Bergh's observed plate colour (vB .. vR), to check computed colours.
    pub observed_color: String,
}

pub fn parse_nebulae(text: &str) -> Result<Vec<NebulaRecord>, String> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            let [kind, name, glon, glat, dist, radius, class, src, extra] = f[..] else {
                return Err(format!("bad line: {line}"));
            };
            let num = |s: &str| {
                s.parse::<f64>()
                    .map_err(|_| format!("bad number {s:?} in {line}"))
            };
            Ok(NebulaRecord {
                kind: match kind {
                    "E" => NebulaKind::Emission,
                    "R" => NebulaKind::Reflection,
                    _ => return Err(format!("bad kind {kind:?}")),
                },
                name: name.split('|').next().unwrap_or(name).to_string(),
                alias: name.split_once('|').map(|(_, a)| a.to_string()),
                glon: num(glon)?,
                glat: num(glat)?,
                dist_pc: num(dist)?,
                radius_arcmin: num(radius)?,
                class: class.parse().map_err(|_| format!("bad class in {line}"))?,
                dist_source: src.to_string(),
                hip: extra
                    .split(':')
                    .next()
                    .and_then(|h| h.parse().ok())
                    .unwrap_or(0),
                observed_color: extra.split(':').nth(1).unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// Central surface brightness, V mag per square arcsecond, estimated from the catalog class.
/// Sharpless' classes span faint to bright H II regions (~23.5 to ~20.5); van den Bergh's blue
/// plate classes vF..vBr map to ~24..20. Approximate: bright cores (Orion's is ~17) are brighter.
pub fn surface_brightness_mag(kind: NebulaKind, class: u8) -> f64 {
    match kind {
        NebulaKind::Emission => 25.0 - 1.5 * class.clamp(1, 3) as f64,
        NebulaKind::Reflection => 25.0 - class.clamp(1, 5) as f64,
    }
}

/// Emissive brightness at EV 0, per min_angle^2, of a surface brightness of `mu` mag/arcsec^2.
/// Same calibration as the stars: a point of visual luminosity L at d pc has brightness
/// BRIGHTNESS_SCALE L / d^2 spread over a disk of angular radius min_angle. A V = 4.83 + 5 log(d/10)
/// star (L = 1) therefore has flux BRIGHTNESS_SCALE / 100 at 10 pc spread over pi min_angle^2.
pub fn surface_brightness_scale(mu: f64) -> f64 {
    let flux_per_arcsec2 = BRIGHTNESS_SCALE as f64 / 100.0 * 10f64.powf(-0.4 * (mu - 4.83));
    flux_per_arcsec2 * std::f64::consts::PI / ARCSEC2_SR
}

pub struct Nebula {
    pub record: NebulaRecord,
    pub pos: DVec3,
    pub radius_au: f64,
    pub color: [f32; 3],
    /// Teff (K) and name of the illuminating star, for reflection nebulae.
    pub star: Option<(f64, String)>,
}

pub fn build(records: Vec<NebulaRecord>, catalog: &HygCatalog) -> Vec<Nebula> {
    let gal = SkyFrame::galactic();
    records
        .into_iter()
        .map(|r| {
            let star = (r.hip != 0)
                .then(|| catalog.stars.iter().position(|s| s.hip == r.hip))
                .flatten();
            // Reflection nebulae sit at their star (same position as the starfield point).
            let pos = star.map_or_else(
                || gal.dir(r.glon, r.glat) * r.dist_pc * AU_PER_PC,
                |i| catalog.stars[i].pos,
            );
            let radius_au = pos.length() * (r.radius_arcmin / 60.0).to_radians().tan();
            let (color, star_info) = match r.kind {
                NebulaKind::Emission => (lines_rgb(&HII_LINES), None),
                NebulaKind::Reflection => {
                    let class = star.map_or(' ', |i| catalog.stars[i].class);
                    let teff = class_temperature(class);
                    let name = r
                        .name
                        .split_once('(')
                        .map(|(_, n)| n.trim_end_matches(')').to_string())
                        .unwrap_or_else(|| format!("HIP {}", r.hip));
                    (
                        scattered_starlight_rgb(teff, DUST_SCATTER_EXP),
                        Some((teff, format!("{name} (class {class})"))),
                    )
                }
            };
            Nebula {
                record: r,
                pos,
                radius_au,
                color,
                star: star_info,
            }
        })
        .collect()
}

pub fn nebula_facts(n: &Nebula) -> Vec<(&'static str, String)> {
    let r = &n.record;
    let mu = surface_brightness_mag(r.kind, r.class);
    let mut facts = vec![];
    if let Some(alias) = &r.alias {
        facts.push(("Catalog", alias.clone()));
    }
    facts.extend([
        (
            "Type",
            match r.kind {
                NebulaKind::Emission => "emission nebula (H II region)".to_string(),
                NebulaKind::Reflection => "reflection nebula".to_string(),
            },
        ),
        (
            "Distance from Sun",
            format!(
                "{} pc ({})",
                format_sig(r.dist_pc),
                match r.dist_source.as_str() {
                    "MD22" => "Gaia EDR3, Mendez-Delgado+ 2022",
                    "FB15" => "OB stars, Foster & Brunt 2015",
                    "WISE" => "Anderson+ 2014",
                    "HYG" => "its star, HYG",
                    other => other,
                }
            ),
        ),
        (
            "Size",
            format!(
                "{}' radius, ~{} pc",
                format_sig(r.radius_arcmin),
                format_sig(n.radius_au / AU_PER_PC)
            ),
        ),
        (
            "Surface brightness",
            format!("~{mu:.1} mag/arcsec2 (estimated from catalog class)"),
        ),
    ]);
    match r.kind {
        NebulaKind::Emission => {
            facts.push((
                "Spectrum",
                "emission lines (rel. H-beta); colour = what the eye sees".to_string(),
            ));
            let lines: Vec<String> = HII_LINES
                .iter()
                .take(7)
                .map(|l| format!("{} {:.0}nm {}", l.name, l.nm, format_sig(l.rel)))
                .collect();
            facts.push(("Lines", lines[..4].join(", ")));
            facts.push(("", lines[4..].join(", ")));
            facts.push((
                "Line ratios",
                "typical H II region; [O III] strength assumed".to_string(),
            ));
        }
        NebulaKind::Reflection => {
            if let Some((teff, star)) = &n.star {
                facts.push(("Lit by", star.clone()));
                facts.push((
                    "Spectrum",
                    format!("its starlight (~{teff:.0} K) scattered by dust: bluer than the star"),
                ));
            }
        }
    }
    facts
}

pub use params::NebulaParams;

mod params {
    #![allow(dead_code)]
    use bevy::render::render_resource::ShaderType;

    #[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
    pub struct NebulaParams {
        pub exposure: f32,
        pub min_angle: f32,
        pub _pad0: f32,
        pub _pad1: f32,
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct NebulaMaterial {
    #[uniform(0)]
    pub params: NebulaParams,
}

impl Material for NebulaMaterial {
    fn vertex_shader() -> ShaderRef {
        NEBULA_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        NEBULA_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            ATTRIBUTE_NEBULA.at_shader_location(1),
        ])?];
        // Back faces only: exactly one fragment per pixel from outside or inside the cube.
        descriptor.primitive.cull_mode = Some(Face::Front);
        Ok(())
    }
}

/// A cube of half-size `radius` around the origin, carrying the nebula's colour and radius.
pub fn nebula_mesh(radius: f32, rgb: [f32; 3]) -> Mesh {
    let r = radius;
    let positions: Vec<[f32; 3]> = (0..8)
        .map(|i| {
            [
                if i & 1 == 0 { -r } else { r },
                if i & 2 == 0 { -r } else { r },
                if i & 4 == 0 { -r } else { r },
            ]
        })
        .collect();
    // Outward-facing (counter-clockwise) triangles; the pipeline culls the front ones.
    let indices: Vec<u32> = vec![
        0, 2, 3, 0, 3, 1, // -z
        4, 5, 7, 4, 7, 6, // +z
        0, 1, 5, 0, 5, 4, // -y
        2, 6, 7, 2, 7, 3, // +y
        0, 4, 6, 0, 6, 2, // -x
        1, 3, 7, 1, 7, 5, // +x
    ];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(ATTRIBUTE_NEBULA, vec![[rgb[0], rgb[1], rgb[2], r]; 8])
    .with_inserted_indices(Indices::U32(indices))
}

/// Whether nebulae are drawn (Esc menu). On by default.
#[derive(Resource)]
pub struct ShowNebulae(pub bool);

impl Default for ShowNebulae {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Component)]
pub struct NebulaBody;

#[derive(Resource)]
pub struct NebulaMaterialHandle(pub Handle<NebulaMaterial>);

pub fn setup_nebulae(
    mut commands: Commands,
    catalog: Res<HygCatalog>,
    exposure: Res<ExposureSettings>,
    fov: Res<CameraFov>,
    mut objects: ResMut<ObjectCatalog>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    mut materials: Option<ResMut<Assets<NebulaMaterial>>>,
) {
    let records = parse_nebulae(NEBULAE_TSV).expect("embedded nebula data parses (tested)");
    let nebulae = build(records, &catalog);
    let material = materials.as_mut().map(|m| {
        m.add(NebulaMaterial {
            params: NebulaParams {
                exposure: exposure.factor(),
                min_angle: min_star_angle(fov.0),
                _pad0: 0.0,
                _pad1: 0.0,
            },
        })
    });
    for n in &nebulae {
        let mu = surface_brightness_mag(n.record.kind, n.record.class);
        let s = surface_brightness_scale(mu) as f32;
        let mut e = commands.spawn((
            NebulaBody,
            WorldPos(n.pos),
            BodyRadius(n.radius_au),
            NotASurface,
            BodyInfo {
                name: n.record.name.clone(),
                kind: BodyKind::Nebula,
                facts: nebula_facts(n),
            },
            Transform::default(),
            Visibility::default(),
        ));
        if let (Some(meshes), Some(material)) = (meshes.as_mut(), material.as_ref()) {
            e.insert((
                Mesh3d(meshes.add(nebula_mesh(n.radius_au as f32, n.color.map(|c| c * s)))),
                MeshMaterial3d(material.clone()),
            ));
        }
        let e = e.id();
        spawn_label(&mut commands, e, &n.record.name);
        objects.0.push((n.record.name.clone(), e));
        if let Some(alias) = &n.record.alias {
            objects.0.push((alias.clone(), e));
        }
    }
    if let Some(material) = material {
        commands.insert_resource(NebulaMaterialHandle(material));
    }
}

/// Keeps the shared uniform in step with exposure and zoom; writes only on change.
pub fn sync_nebula_params(
    handle: Option<Res<NebulaMaterialHandle>>,
    exposure: Res<ExposureSettings>,
    fov: Res<CameraFov>,
    mut materials: ResMut<Assets<NebulaMaterial>>,
) {
    let Some(handle) = handle else { return };
    let Some(current) = materials.get(&handle.0).map(|m| m.params) else {
        return;
    };
    let wanted = NebulaParams {
        exposure: exposure.factor(),
        min_angle: min_star_angle(fov.0),
        ..current
    };
    if wanted != current
        && let Some(m) = materials.get_mut(&handle.0)
    {
        m.params = wanted;
    }
}

pub fn update_nebula_visibility(
    show: Res<ShowNebulae>,
    mut q: Query<&mut Visibility, With<NebulaBody>>,
) {
    if !show.is_changed() {
        return;
    }
    let vis = if show.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut q {
        v.set_if_neq(vis);
    }
}

pub struct NebulaePlugin;

impl Plugin for NebulaePlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, NEBULA_SHADER, "nebula.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<NebulaMaterial> {
            prepass_enabled: false,
            shadows_enabled: false,
            ..default()
        })
        .init_resource::<ShowNebulae>()
        .add_systems(Startup, setup_nebulae)
        .add_systems(
            Update,
            (sync_nebula_params, update_nebula_visibility).in_set(NavSet::View),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded() -> Vec<Nebula> {
        build(parse_nebulae(NEBULAE_TSV).unwrap(), &HygCatalog::embedded())
    }

    fn find<'a>(all: &'a [Nebula], name: &str) -> &'a Nebula {
        all.iter()
            .find(|n| n.record.name.starts_with(name))
            .unwrap()
    }

    #[test]
    fn test_embedded_data() {
        let all = parse_nebulae(NEBULAE_TSV).unwrap();
        let e = all
            .iter()
            .filter(|n| n.kind == NebulaKind::Emission)
            .count();
        let r = all.len() - e;
        assert_eq!((e, r), (168, 52), "matches data/nebulae.tsv.manifest.txt");
        assert!(all.iter().all(|n| n.name.is_ascii() && n.dist_pc > 0.0));
    }

    #[test]
    fn test_orion_nebula_is_where_it_should_be() {
        // M42: RA 5h35m17s, Dec -5d23m (J2000), 0.41 kpc (Gaia, Mendez-Delgado+ 2022). Galactic
        // coordinates go through SkyFrame::galactic, so this also guards that frame.
        let all = loaded();
        let m42 = find(&all, "Orion Nebula");
        let expected =
            crate::stars::star_world_pos(5.0 + 35.0 / 60.0 + 17.0 / 3600.0, -5.39, 410.0);
        let sep = m42.pos.angle_between(expected).to_degrees();
        assert!(sep < 0.3, "M42 off by {sep} deg");
        assert!((m42.pos.length() / AU_PER_PC - 410.0).abs() < 1.0);
        // 30' radius at 410 pc: ~3.6 pc.
        assert!((m42.radius_au / AU_PER_PC - 3.58).abs() < 0.1);
    }

    #[test]
    fn test_reflection_nebulae_sit_at_their_star_and_are_bluer() {
        let cat = HygCatalog::embedded();
        let all = loaded();
        let merope = find(&all, "vdB 22");
        let star = cat.stars[cat.index_of_name("Merope").unwrap()].pos;
        assert!(merope.pos.distance(star) < 1e-6);
        // Merope is a B6 star; its nebula is blue.
        assert!(merope.color[2] > merope.color[0], "{:?}", merope.color);
        let lagoon = find(&all, "Lagoon");
        assert!(lagoon.color[0] > lagoon.color[1], "{:?}", lagoon.color);
    }

    #[test]
    fn test_computed_colours_agree_with_observed_plate_colours() {
        // van den Bergh recorded each reflection nebula's colour on the blue/red survey plates.
        // IP computes colours from the star's spectrum scattered by dust, independently: every
        // nebula he saw as red (around red giants like Antares) must come out redder than every
        // one he saw as very blue (around B stars like the Pleiades).
        let all = loaded();
        let ratio = |n: &Nebula| n.color[2] / n.color[0].max(1e-6);
        let of = |codes: &[&str]| -> Vec<f32> {
            all.iter()
                .filter(|n| codes.contains(&n.record.observed_color.as_str()))
                .map(ratio)
                .collect()
        };
        let red = of(&["R", "vR", "mR"]);
        let blue = of(&["vB"]);
        assert!(
            red.len() >= 3 && blue.len() >= 10,
            "{} red, {} blue",
            red.len(),
            blue.len()
        );
        let reddest_blue = blue.iter().copied().fold(f32::INFINITY, f32::min);
        let bluest_red = red.iter().copied().fold(0.0, f32::max);
        assert!(bluest_red < reddest_blue, "red {red:?} vs blue {blue:?}");
        let antares = find(&all, "vdB 107");
        assert!(antares.color[0] > antares.color[2], "{:?}", antares.color);
    }

    #[test]
    fn test_surface_brightness_calibration() {
        // A uniform patch of surface brightness mu covering exactly the min-angle disk has the flux
        // of a star of magnitude m = mu - 2.5 log10(area in arcsec^2): it must render equally bright.
        let mu = 21.0;
        let min_angle = crate::stars::MIN_STAR_ANGLE as f64;
        let area_arcsec2 = std::f64::consts::PI * min_angle * min_angle / ARCSEC2_SR;
        let m = mu - 2.5 * area_arcsec2.log10();
        // Brightness of a star of apparent magnitude m: BRIGHTNESS_SCALE L / d^2 with L/d^2
        // = 10^(-0.4 (m - 4.83)) / 100 (L = 1 at 10 pc gives m = 4.83).
        let star = BRIGHTNESS_SCALE as f64 / 100.0 * 10f64.powf(-0.4 * (m - 4.83));
        let patch = surface_brightness_scale(mu) * min_angle * min_angle;
        assert!((patch / star - 1.0).abs() < 1e-9, "{patch} vs {star}");
    }

    #[test]
    fn test_facts_are_ascii() {
        for n in loaded().iter().take(300) {
            for (k, v) in nebula_facts(n) {
                assert!(k.is_ascii() && v.is_ascii(), "{k}: {v}");
            }
        }
    }
}
