//! Renders the HYG catalog as camera-facing billboards: one mesh per spatial chunk, one draw
//! each, with brightness and size computed in the vertex shader from the same model as the
//! sphere stars. No per-star entities, so 100k stars cost no per-frame CPU work (see PERF.md).

use crate::camera::FlyCam;
use crate::constants::AU_PER_PC;
use crate::coords::{NavSet, Nearest, WorldPos};
use crate::hyg::{HygCatalog, HygStar};
use crate::menu::ShowCatalogStars;
use crate::stars::{
    BRIGHTNESS_SCALE, CuratedHygMatches, ExposureSettings, MAX_STAR_BRIGHTNESS, spectral_color,
};
use crate::zoom::{CameraFov, min_star_angle};
use bevy::asset::{load_internal_asset, weak_handle};
use bevy::math::{DVec3, IVec3};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::mesh::{
    Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology,
};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError, VertexFormat,
};
use std::collections::{HashMap, HashSet};

const STARFIELD_SHADER: Handle<Shader> = weak_handle!("6a1b8a3e-3c0f-4b8e-9b55-2f0f6d7c9a10");

/// Per-vertex: billboard corner (xy) and visual luminosity (z).
pub const ATTRIBUTE_STAR: MeshVertexAttribute =
    MeshVertexAttribute::new("Star", 7_301_225_118, VertexFormat::Float32x4);

/// Chunk edge length, parsecs. Bigger chunks mean fewer draws; vertex positions are stored
/// relative to the chunk center in f32, so this also bounds their error (~4 AU at 200 pc).
pub const CHUNK_PC: f64 = 200.0;

/// Points fade out between these camera distances (AU).
pub const FADE_NEAR_AU: f32 = 50.0;
pub const FADE_FAR_AU: f32 = 200.0;

pub use params::StarfieldParams;

mod params {
    // The ShaderType derive generates per-field layout-check functions that rustc reports as
    // unused; they live outside the struct item, so the allow has to cover the module.
    #![allow(dead_code)]
    use bevy::render::render_resource::ShaderType;

    #[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
    pub struct StarfieldParams {
        pub exposure: f32,
        pub brightness_scale: f32,
        pub max_brightness: f32,
        pub min_angle: f32,
        pub au_per_pc: f32,
        pub min_dist_pc: f32,
        pub fade_near_au: f32,
        pub fade_far_au: f32,
        /// Render-space position (xyz) and radius (w, AU) of a sphere whose points are hidden
        /// (the star currently drawn as a close-range proxy). w = 0: nothing hidden.
        pub hide: bevy::math::Vec4,
    }
}

impl StarfieldParams {
    pub fn new(exposure: f32) -> Self {
        Self {
            exposure,
            brightness_scale: BRIGHTNESS_SCALE,
            max_brightness: MAX_STAR_BRIGHTNESS,
            min_angle: crate::stars::MIN_STAR_ANGLE,
            au_per_pc: AU_PER_PC as f32,
            min_dist_pc: crate::stars::MIN_BRIGHTNESS_DIST_PC,
            fade_near_au: FADE_NEAR_AU,
            fade_far_au: FADE_FAR_AU,
            hide: Vec4::ZERO,
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct StarfieldMaterial {
    #[uniform(0)]
    pub params: StarfieldParams,
}

impl Material for StarfieldMaterial {
    fn vertex_shader() -> ShaderRef {
        STARFIELD_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        STARFIELD_SHADER.into()
    }

    /// Starlight adds up: overlapping stars brighten, and dim or hidden points contribute
    /// nothing instead of drawing black disks over whatever is behind them.
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
            ATTRIBUTE_STAR.at_shader_location(1),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(2),
        ])?];
        // Billboards are built in view space; don't depend on their winding.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[derive(Component)]
pub struct StarfieldChunk;

/// The one material all chunks share.
#[derive(Resource)]
pub struct StarfieldMaterialHandle(pub Handle<StarfieldMaterial>);

/// Render-space center (xyz) and radius (w) of the sphere whose points are hidden; written by
/// `hyg_bodies::update_star_proxy`, w = 0 when nothing is hidden.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct StarfieldHide(pub Vec4);

/// World positions of the rendered catalog stars (curated stars excluded), for the
/// nearest-object distance that drives adaptive speed.
#[derive(Resource, Default)]
pub struct StarPoints {
    pub positions: Vec<DVec3>,
    /// HYG catalog index of each entry in `positions`.
    pub indices: Vec<usize>,
    /// Nearest rendered catalog star to the camera: (HYG index, center distance AU).
    pub nearest: Option<(usize, f64)>,
    last_camera: Option<DVec3>,
}

/// Groups star indices into cubic chunks of `chunk_au` (AU), skipping `skip`.
pub fn chunk_stars(
    stars: &[HygStar],
    skip: &HashSet<usize>,
    chunk_au: f64,
) -> HashMap<IVec3, Vec<usize>> {
    let mut chunks: HashMap<IVec3, Vec<usize>> = HashMap::new();
    for (i, s) in stars.iter().enumerate() {
        if skip.contains(&i) {
            continue;
        }
        let key = (s.pos / chunk_au).floor().as_ivec3();
        chunks.entry(key).or_default().push(i);
    }
    chunks
}

/// Billboard mesh for one chunk: 4 vertices per star, positions relative to `center` (AU).
pub fn chunk_mesh(stars: &[HygStar], members: &[usize], center: DVec3) -> Mesh {
    const CORNERS: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
    let n = members.len();
    let mut positions = Vec::with_capacity(n * 4);
    let mut star_attr = Vec::with_capacity(n * 4);
    let mut colors = Vec::with_capacity(n * 4);
    let mut indices = Vec::with_capacity(n * 6);
    for (k, &i) in members.iter().enumerate() {
        let s = &stars[i];
        let local = (s.pos - center).as_vec3().to_array();
        let tint = spectral_color(s.class).to_f32_array();
        for c in CORNERS {
            positions.push(local);
            star_attr.push([c[0], c[1], s.lum, 0.0]);
            colors.push(tint);
        }
        let base = (k * 4) as u32;
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(ATTRIBUTE_STAR, star_attr)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

pub fn setup_starfield(
    mut commands: Commands,
    catalog: Res<HygCatalog>,
    curated: Res<CuratedHygMatches>,
    exposure: Res<ExposureSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StarfieldMaterial>>,
    mut points: ResMut<StarPoints>,
) {
    let skip: HashSet<usize> = curated.0.iter().flatten().copied().collect();
    let chunk_au = CHUNK_PC * AU_PER_PC;
    let material = materials.add(StarfieldMaterial {
        params: StarfieldParams::new(exposure.factor()),
    });
    let chunks = chunk_stars(&catalog.stars, &skip, chunk_au);
    commands.insert_resource(StarfieldMaterialHandle(material.clone()));
    for (key, members) in &chunks {
        let center = (key.as_dvec3() + DVec3::splat(0.5)) * chunk_au;
        commands.spawn((
            StarfieldChunk,
            Mesh3d(meshes.add(chunk_mesh(&catalog.stars, members, center))),
            MeshMaterial3d(material.clone()),
            Transform::default(),
            WorldPos(center),
        ));
    }
    points.indices = (0..catalog.stars.len())
        .filter(|i| !skip.contains(i))
        .collect();
    points.positions = points
        .indices
        .iter()
        .map(|&i| catalog.stars[i].pos)
        .collect();
    info!(
        "starfield: {} stars in {} chunks",
        points.positions.len(),
        chunks.len()
    );
}

/// Keeps the shared uniform in step with exposure, zoom and the hidden proxy star. Writes (and
/// so re-uploads) the material only when a value actually changed.
pub fn sync_starfield_params(
    handle: Option<Res<StarfieldMaterialHandle>>,
    exposure: Res<ExposureSettings>,
    fov: Res<CameraFov>,
    hide: Res<StarfieldHide>,
    mut materials: ResMut<Assets<StarfieldMaterial>>,
) {
    let Some(handle) = handle else { return };
    let Some(current) = materials.get(&handle.0).map(|m| m.params) else {
        return;
    };
    let wanted = StarfieldParams {
        exposure: exposure.factor(),
        min_angle: min_star_angle(fov.0),
        hide: hide.0,
        ..current
    };
    if wanted != current
        && let Some(mat) = materials.get_mut(&handle.0)
    {
        mat.params = wanted;
    }
}

pub fn update_starfield_visibility(
    show: Res<ShowCatalogStars>,
    mut chunks: Query<&mut Visibility, With<StarfieldChunk>>,
) {
    if !show.is_changed() {
        return;
    }
    let vis = if show.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut chunks {
        *v = vis;
    }
}

/// Lowers `Nearest` to the closest catalog star. Rescans only when the camera moved.
pub fn nearest_catalog_star(
    cam: Query<&WorldPos, With<FlyCam>>,
    mut points: ResMut<StarPoints>,
    mut nearest: ResMut<Nearest>,
) {
    let Ok(cam) = cam.single() else { return };
    if points.last_camera != Some(cam.0) {
        let best = points
            .positions
            .iter()
            .enumerate()
            .map(|(k, p)| (k, p.distance_squared(cam.0)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        points.nearest = best.map(|(k, d2)| (points.indices[k], d2.sqrt()));
        points.last_camera = Some(cam.0);
    }
    if let Some((_, d)) = points.nearest
        && d < nearest.surface_dist
    {
        nearest.surface_dist = d;
    }
}

pub struct StarfieldPlugin;

impl Plugin for StarfieldPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, STARFIELD_SHADER, "starfield.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<StarfieldMaterial> {
            prepass_enabled: false,
            shadows_enabled: false,
            ..default()
        })
        .init_resource::<StarPoints>()
        .init_resource::<StarfieldHide>()
        .add_systems(Startup, setup_starfield.after(crate::stars::setup_stars))
        .add_systems(
            Update,
            (
                nearest_catalog_star
                    .in_set(NavSet::Rebase)
                    .after(crate::coords::update_nearest)
                    .before(crate::coords::rebase_origin),
                (sync_starfield_params, update_starfield_visibility).in_set(NavSet::View),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn star(pos: DVec3) -> HygStar {
        HygStar {
            pos,
            lum: 1.0,
            hip: 0,
            hyg_id: 0,
            class: 'G',
        }
    }

    #[test]
    fn test_chunking_groups_and_skips() {
        let stars = vec![
            star(DVec3::new(1.0, 1.0, 1.0)),
            star(DVec3::new(2.0, 2.0, 2.0)),
            star(DVec3::new(-1.0, 1.0, 1.0)),
            star(DVec3::new(3.0, 3.0, 3.0)),
        ];
        let skip = HashSet::from([3]);
        let chunks = chunk_stars(&stars, &skip, 10.0);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[&IVec3::new(0, 0, 0)], vec![0, 1]);
        assert_eq!(chunks[&IVec3::new(-1, 0, 0)], vec![2]);
    }

    #[test]
    fn test_chunk_mesh_layout() {
        let stars = vec![
            star(DVec3::new(5.0, 0.0, 0.0)),
            star(DVec3::new(6.0, 0.0, 0.0)),
        ];
        let mesh = chunk_mesh(&stars, &[0, 1], DVec3::new(5.0, 0.0, 0.0));
        assert_eq!(mesh.count_vertices(), 8);
        let Some(Indices::U32(idx)) = mesh.indices() else {
            panic!("expected u32 indices (u16 overflows past 16383 stars)")
        };
        assert_eq!(idx.len(), 12);
        assert_eq!(&idx[6..], &[4, 5, 6, 4, 6, 7]);
    }

    #[test]
    fn test_real_catalog_chunk_count_is_modest() {
        // Each chunk is one draw call; keep the count small for the iGPU.
        let cat = HygCatalog::embedded();
        let chunks = chunk_stars(&cat.stars, &HashSet::new(), CHUNK_PC * AU_PER_PC);
        assert!(chunks.len() < 1500, "{} chunks", chunks.len());
        let total: usize = chunks.values().map(Vec::len).sum();
        assert_eq!(total, cat.stars.len());
    }

    #[test]
    fn test_starfield_uniform_follows_exposure_zoom_and_hide() {
        let mut app = crate::testing::headless_app();
        app.add_plugins(AssetPlugin::default())
            .init_asset::<StarfieldMaterial>()
            .init_resource::<ExposureSettings>()
            .init_resource::<CameraFov>()
            .init_resource::<StarfieldHide>()
            .add_systems(Update, sync_starfield_params);
        let handle = app
            .world_mut()
            .resource_mut::<Assets<StarfieldMaterial>>()
            .add(StarfieldMaterial {
                params: StarfieldParams::new(1.0),
            });
        app.insert_resource(StarfieldMaterialHandle(handle.clone()));
        app.world_mut().resource_mut::<ExposureSettings>().ev = 3;
        *app.world_mut().resource_mut::<CameraFov>() = CameraFov(0.01);
        app.world_mut().resource_mut::<StarfieldHide>().0 = Vec4::new(1.0, 2.0, 3.0, 50.0);
        app.update();
        let params = app
            .world()
            .resource::<Assets<StarfieldMaterial>>()
            .get(&handle)
            .unwrap()
            .params;
        assert_eq!(params.exposure, 8.0);
        assert_eq!(params.min_angle, min_star_angle(0.01));
        assert_eq!(params.hide.w, 50.0);
    }

    #[test]
    #[ignore]
    fn perf_nearest_catalog_star() {
        let cat = HygCatalog::embedded();
        let positions: Vec<DVec3> = cat.stars.iter().map(|s| s.pos).collect();
        let start = std::time::Instant::now();
        let iters = 50;
        for k in 0..iters {
            let cam = DVec3::splat(k as f64);
            std::hint::black_box(
                positions
                    .iter()
                    .map(|p| p.distance_squared(cam))
                    .fold(f64::INFINITY, f64::min),
            );
        }
        println!(
            "perf_nearest_catalog_star n={}: {:?}",
            positions.len(),
            start.elapsed() / iters
        );
    }
}
