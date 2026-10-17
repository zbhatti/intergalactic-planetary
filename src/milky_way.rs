//! The Milky Way's glow: light from the billions of stars too faint or too far to be in the
//! catalog, with the dust in front of it. Dark lanes such as the Great Rift in Cygnus and Aquila
//! are this dust in silhouette, so they only appear against this glow.
//!
//! **This is a model, not observed data** (unlike the catalogs and the dust map). A smooth
//! galaxy: thin and thick exponential discs normalized to the local V-band luminosity density
//! (0.056 L_sun/pc^3, column 24.4 L_sun/pc^2: Flynn et al. 2006), scale length 2.6 kpc and
//! R0 = 8.2 kpc (Bland-Hawthorn & Gerhard 2016), and an exponential bar/bulge of ~5e9 L_sun
//! tilted 27 degrees from the Sun-centre line. Dust: the Edenhofer map inside 1.25 kpc
//! (`dust.rs`), and beyond it a smooth exponential dust disc (~1 mag/kpc in the plane locally,
//! 100 pc scale height, 3.2 kpc scale length) that hides the Galactic centre.
//!
//! The catalog stars are already drawn as points, so their light is taken out: the
//! [`GalaxyModel::completeness`] curve is the fraction of the model's luminosity, per shell of
//! distance from the Sun, that HYG already holds (near 1 close to the Sun, falling with
//! distance). The glow is the rest.
//!
//! Each ray is marched outwards from the camera in f64 on the CPU, accumulating emission times
//! the transmission of the dust crossed so far, into a 384x192 Galactic-coordinate texture. A
//! background thread recomputes it when the camera moves more than [`RECOMPUTE_PC`]; a cube
//! around the camera draws it (`milky_way.wgsl`), depth-tested so planets hide it.
//!
//! Calibration check (test): from home, at l = 10..70 deg, Pioneer 10 measured ~280 S10 at
//! b = -8 deg and ~185 S10 at b = -14 deg after removing stars brighter than V = 7.5 (Toller 1981
//! via van der Kruit 2024, arXiv:2401.15518).

use crate::camera::FlyCam;
use crate::constants::AU_PER_PC;
use crate::coords::{NavSet, WorldPos};
use crate::dust::{Dust, DustGrid, DustSettings, channel_extinction_ratios};
use crate::hyg::HygCatalog;
use crate::nebulae::CUBE_INDICES;
use crate::sky::{SkyFrame, SkySettings};
use crate::spectrum::scattered_starlight_rgb;
use crate::stars::{BRIGHTNESS_SCALE, ExposureSettings};
use crate::zoom::{CameraFov, min_star_angle};
use bevy::asset::{load_internal_asset, weak_handle};
use bevy::math::{DMat3, DVec3};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, Face, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError,
    TextureDimension, TextureFormat,
};
use bevy::render::view::NoFrustumCulling;
use std::sync::Arc;
use std::thread::JoinHandle;

const MILKY_WAY_SHADER: Handle<Shader> = weak_handle!("5d1a7c3e-2b84-4f69-9e0d-71c4a8b3f912");

/// Sun-Galactic centre distance (pc) and the Sun's height above the plane.
const R0_PC: f64 = 8200.0;
const Z_SUN_PC: f64 = 20.0;
/// Local V-band luminosity density in the plane (L_sun/pc^3), Flynn et al. 2006.
const RHO_V_LOCAL: f64 = 0.056;
/// Thin and thick disc scale heights (pc), and the thick disc's share of the local density.
/// Together they give the local column of 24.4 L_sun/pc^2 (Flynn et al. 2006).
const THIN_HZ_PC: f64 = 190.0;
const THICK_HZ_PC: f64 = 900.0;
const THICK_FRACTION: f64 = 0.04;
const THIN_HR_PC: f64 = 2600.0;
const THICK_HR_PC: f64 = 2000.0;
const DISK_R_MAX_PC: f64 = 15_000.0;
/// Bar/bulge: total V luminosity (L_sun), exponential scale lengths along the bar, across it and
/// vertically (pc), and the angle between the bar's near end (at positive l) and the Sun-centre
/// line.
const BULGE_L_SUN: f64 = 5.0e9;
const BULGE_SCALES_PC: [f64; 3] = [700.0, 300.0, 250.0];
const BAR_ANGLE_DEG: f64 = 27.0;
/// Smooth dust disc used beyond the Edenhofer map: A_V per pc in the plane at R0, scale height,
/// scale length, and outer edge.
const DUST_A0_PER_PC: f64 = 1.0e-3;
const DUST_HZ_PC: f64 = 100.0;
const DUST_HR_PC: f64 = 3200.0;
const DUST_R_MAX_PC: f64 = 16_000.0;
/// Radius of the Edenhofer map's data (pc); the grid box's corners beyond it hold no data.
const DUST_MAP_RADIUS_PC: f64 = 1250.0;
/// Colour temperatures of the integrated light: the disc (B-V ~ 0.7) and the older, redder bulge.
const DISK_TEFF: f64 = 5000.0;
const BULGE_TEFF: f64 = 4300.0;

/// Completeness shells (pc) and how far they go; beyond, the catalog holds nothing of note.
const SHELL_PC: f64 = 25.0;
const SHELLS: usize = 120;

/// Ray march: fixed steps inside the dust map (two voxels; the trilinear sampling smooths
/// between them), then growing with distance.
const STEP_PC: f64 = 24.0;
const STEP_GROWTH: f64 = 0.06;
const STEP_MAX_PC: f64 = 400.0;
const MARCH_MAX_PC: f64 = 40_000.0;

/// Sky texture size (Galactic longitude x latitude), ~0.94 deg per texel: the dust map's 12 pc
/// voxels seen from ~700 pc, so finer would add cost without detail.
pub const TEX_W: usize = 384;
pub const TEX_H: usize = 192;
/// Camera motion (pc) that triggers a recompute, once the camera has settled: it has stayed
/// within [`SETTLE_PC`] for [`SETTLE_SECS`]. A long flight recomputes on arrival, not all the way,
/// and only if the glow would be visible (arriving at a bright star, auto-exposure hides it).
pub const RECOMPUTE_PC: f64 = 10.0;
/// A sky computed further away than this is too wrong to show: hidden until recomputed.
const STALE_PC: f64 = 100.0;
const SETTLE_PC: f64 = 0.5;
const SETTLE_SECS: f32 = 0.5;
/// Below this (linear, after exposure) a pixel is invisible after tonemapping (~1/255 in sRGB):
/// the shader skips it, and when even the brightest texel is below it the cube isn't drawn.
const VISIBLE_CUTOFF: f32 = 2.0e-4;
/// Radius (AU) of the cube drawn around the camera: beyond every object IP draws, well inside
/// the camera's far plane.
const SKY_CUBE_AU: f32 = 1.0e12;

/// The galaxy model and the catalog's completeness curve.
#[derive(Clone, Debug)]
pub struct GalaxyModel {
    /// Fraction of the model's luminosity HYG holds, per [`SHELL_PC`] shell from the Sun.
    pub completeness: Vec<f64>,
    gal_from_world: DMat3,
    disk_rgb: [f64; 3],
    bulge_rgb: [f64; 3],
    k_rgb: [f64; 3],
    /// Bar frame: unit vectors (galactocentric X, Y) along and across the bar.
    bar_along: [f64; 2],
    bar_across: [f64; 2],
    bulge_j0: f64,
    thin_j0: f64,
    thick_j0: f64,
}

/// Luminosity densities (L_sun/pc^3, V band) of the disc and the bulge at a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Emission {
    pub disk: f64,
    pub bulge: f64,
}

impl GalaxyModel {
    /// The model without the catalog subtraction (completeness 0 everywhere).
    pub fn bare() -> Self {
        let [a, b, c] = BULGE_SCALES_PC;
        let t = BAR_ANGLE_DEG.to_radians();
        let to_rgb = |teff| scattered_starlight_rgb(teff, 0.0).map(|v| v as f64);
        Self {
            completeness: vec![0.0; SHELLS],
            gal_from_world: SkyFrame::galactic().0.transpose(),
            disk_rgb: to_rgb(DISK_TEFF),
            bulge_rgb: to_rgb(BULGE_TEFF),
            k_rgb: channel_extinction_ratios().map(|k| k as f64),
            // Galactocentric X points from the centre towards the Sun's side (X = x - R0 < 0
            // at the Sun), Y towards l = 90: the near end is at -X, +Y.
            bar_along: [-t.cos(), t.sin()],
            bar_across: [t.sin(), t.cos()],
            bulge_j0: BULGE_L_SUN / (8.0 * std::f64::consts::PI * a * b * c),
            // Normalized at the Sun's galactocentric radius in the plane.
            thin_j0: RHO_V_LOCAL * (1.0 - THICK_FRACTION),
            thick_j0: RHO_V_LOCAL * THICK_FRACTION,
        }
    }

    /// The model with the light of HYG's stars taken out.
    pub fn new(catalog: &HygCatalog) -> Self {
        let mut m = Self::bare();
        m.completeness = m.measure_completeness(catalog);
        m
    }

    /// Galactic heliocentric position (pc) of a world position (AU).
    pub fn gal_pc(&self, world_au: DVec3) -> DVec3 {
        self.gal_from_world * world_au / AU_PER_PC
    }

    /// Model luminosity density at a Galactic heliocentric position (pc), before subtraction.
    pub fn emission(&self, g: DVec3) -> Emission {
        let x = g.x - R0_PC;
        let y = g.y;
        let z = g.z + Z_SUN_PC;
        let r = (x * x + y * y).sqrt();
        let disk = if r < DISK_R_MAX_PC {
            let dr = r - R0_PC;
            self.thin_j0 * (-dr / THIN_HR_PC - z.abs() / THIN_HZ_PC).exp()
                + self.thick_j0 * (-dr / THICK_HR_PC - z.abs() / THICK_HZ_PC).exp()
        } else {
            0.0
        };
        let [a, b, c] = BULGE_SCALES_PC;
        let u = (x * self.bar_along[0] + y * self.bar_along[1]) / a;
        let v = (x * self.bar_across[0] + y * self.bar_across[1]) / b;
        let w = z / c;
        let m = (u * u + v * v + w * w).sqrt();
        // Past ~15 scale lengths the bulge is < 1e-6 of its centre: skip the exp.
        let bulge = if m < 15.0 {
            self.bulge_j0 * (-m).exp()
        } else {
            0.0
        };
        Emission { disk, bulge }
    }

    /// Fraction of the model's light already drawn as catalog stars at `d_pc` from the Sun.
    pub fn completeness_at(&self, d_pc: f64) -> f64 {
        let s = d_pc / SHELL_PC - 0.5;
        if s <= 0.0 {
            return self.completeness[0];
        }
        let i = s.floor() as usize;
        if i + 1 >= self.completeness.len() {
            return 0.0;
        }
        let f = s - i as f64;
        self.completeness[i] * (1.0 - f) + self.completeness[i + 1] * f
    }

    /// Per shell: HYG's (intrinsic) luminosity over the model's, clamped to 0..1 and made
    /// non-increasing outwards (catalogs only lose stars with distance), so clumps such as the
    /// Gould Belt don't make holes or bright rings in the glow.
    fn measure_completeness(&self, catalog: &HygCatalog) -> Vec<f64> {
        let mut cat = vec![0.0; SHELLS];
        for s in catalog.stars.iter().filter(|s| s.dist_known) {
            let d = s.pos.length() / AU_PER_PC;
            if d < 1.0 {
                continue; // the Sun
            }
            let i = (d / SHELL_PC) as usize;
            if i < SHELLS {
                cat[i] += s.lum as f64;
            }
        }
        let dirs = fibonacci_sphere(2048);
        let mut out = Vec::with_capacity(SHELLS);
        let mut prev = 1.0f64;
        for (i, &l_cat) in cat.iter().enumerate() {
            let r = (i as f64 + 0.5) * SHELL_PC;
            let mean_j = dirs
                .iter()
                .map(|d| {
                    let e = self.emission(*d * r);
                    e.disk + e.bulge
                })
                .sum::<f64>()
                / dirs.len() as f64;
            let shell_vol = 4.0 / 3.0
                * std::f64::consts::PI
                * (((i + 1) as f64 * SHELL_PC).powi(3) - (i as f64 * SHELL_PC).powi(3));
            let c = (l_cat / (mean_j * shell_vol)).clamp(0.0, 1.0).min(prev);
            out.push(c);
            prev = c;
        }
        out
    }

    /// The unresolved part of the emission at `g` (pc).
    pub fn unresolved(&self, g: DVec3) -> Emission {
        let e = self.emission(g);
        Emission {
            disk: e.disk * (1.0 - self.completeness_at(g.length())),
            bulge: e.bulge,
        }
    }

    /// A_V per pc at `g`: the Edenhofer map where it has data, the smooth disc elsewhere.
    pub fn dust_density(&self, grid: &DustGrid, g: DVec3) -> f64 {
        if g.length() < DUST_MAP_RADIUS_PC {
            let t = (g - grid.min_pc) / grid.size_pc();
            if t.cmpge(DVec3::ZERO).all() && t.cmple(DVec3::ONE).all() {
                return grid.density(t);
            }
        }
        let x = g.x - R0_PC;
        let r = (x * x + g.y * g.y).sqrt();
        if r > DUST_R_MAX_PC {
            return 0.0;
        }
        DUST_A0_PER_PC * (-(r - R0_PC) / DUST_HR_PC - (g.z + Z_SUN_PC).abs() / DUST_HZ_PC).exp()
    }

    /// Surface brightness (linear RGB, IP flux per steradian: L_sun/pc^2 for a V-band source,
    /// the scale where a star's flux is L / d^2) seen from `from` (Galactic pc) along `dir`
    /// (Galactic unit vector). `grid` None means no dust.
    pub fn ray(&self, from: DVec3, dir: DVec3, grid: Option<&DustGrid>) -> [f64; 3] {
        let mut out = [0.0; 3];
        let mut av = 0.0;
        let mut s = 0.0;
        // Extinction in nepers per magnitude, per channel.
        let tau_k = self.k_rgb.map(|k| k * 0.4 * std::f64::consts::LN_10);
        while s < MARCH_MAX_PC {
            // Fine steps only where the dust map has structure to resolve.
            let here = from + dir * s;
            let in_map = grid.is_some_and(|g| {
                here.length() < DUST_MAP_RADIUS_PC && here.z.abs() < 0.5 * g.size_pc().z
            });
            let ds = if in_map {
                STEP_PC
            } else {
                (s * STEP_GROWTH).clamp(STEP_PC, STEP_MAX_PC)
            };
            let p = from + dir * (s + 0.5 * ds);
            let dav = grid.map_or(0.0, |g| self.dust_density(g, p) * ds);
            let e = self.unresolved(p);
            if e.disk + e.bulge > 0.0 {
                let a = av + 0.5 * dav;
                for c in 0..3 {
                    let t = (-a * tau_k[c]).exp();
                    out[c] += (e.disk * self.disk_rgb[c] + e.bulge * self.bulge_rgb[c]) * ds * t;
                }
            }
            av += dav;
            s += ds;
            // Done once the ray has left the galaxy (or the red channel is fully absorbed).
            let x = p.x - R0_PC;
            let z = p.z + Z_SUN_PC;
            let leaving_z = z.abs() > 4000.0 && z * dir.z > 0.0;
            let leaving_r =
                (x * x + p.y * p.y) > 20_000f64.powi(2) && x * dir.x + p.y * dir.y > 0.0;
            // e^-14 in red: what's left is far below anything visible.
            if leaving_z || leaving_r || av * self.k_rgb[0] > 15.0 {
                break;
            }
        }
        out
    }

    /// Galactic unit vector of texel (i, j) of the sky texture.
    pub fn texel_dir(i: usize, j: usize) -> DVec3 {
        let l = (i as f64 + 0.5) / TEX_W as f64 * std::f64::consts::TAU;
        let b =
            std::f64::consts::FRAC_PI_2 - (j as f64 + 0.5) / TEX_H as f64 * std::f64::consts::PI;
        DVec3::new(b.cos() * l.cos(), b.cos() * l.sin(), b.sin())
    }

    /// The sky texture seen from `from_world` (AU): Rgba16Float bytes holding the emissive
    /// brightness per min_angle^2 at EV 0 (the nebula scale, see `nebulae::surface_brightness_scale`).
    pub fn render(&self, from_world: DVec3, grid: Option<&DustGrid>, threads: usize) -> SkyTexture {
        let from = self.gal_pc(from_world);
        let scale = BRIGHTNESS_SCALE as f64 * std::f64::consts::PI;
        let mut bytes = vec![0u8; TEX_W * TEX_H * 8];
        let rows_per = TEX_H.div_ceil(threads.max(1));
        let peak = std::thread::scope(|scope| {
            let workers: Vec<_> = bytes
                .chunks_mut(rows_per * TEX_W * 8)
                .enumerate()
                .map(|(chunk_i, chunk)| {
                    scope.spawn(move || {
                        let mut peak = 0f32;
                        for (k, texel) in chunk.chunks_mut(8).enumerate() {
                            let j = chunk_i * rows_per + k / TEX_W;
                            let rgb = self.ray(from, Self::texel_dir(k % TEX_W, j), grid);
                            for c in 0..4 {
                                let v = if c < 3 { (rgb[c] * scale) as f32 } else { 0.0 };
                                peak = peak.max(v);
                                texel[2 * c..2 * c + 2].copy_from_slice(&f16_bits(v).to_le_bytes());
                            }
                        }
                        peak
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|w| w.join().unwrap_or(0.0))
                .fold(0.0, f32::max)
        });
        SkyTexture { bytes, peak }
    }
}

/// A computed sky: Rgba16Float texel bytes and the brightest channel value in them.
pub struct SkyTexture {
    pub bytes: Vec<u8>,
    pub peak: f32,
}

/// Evenly spread unit vectors.
fn fibonacci_sphere(n: usize) -> Vec<DVec3> {
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    (0..n)
        .map(|i| {
            let z = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
            let r = (1.0 - z * z).sqrt();
            let t = golden * i as f64;
            DVec3::new(r * t.cos(), r * t.sin(), z)
        })
        .collect()
}

/// IEEE half-precision bits of a non-negative finite f32 (rounded to nearest; overflow -> max).
pub fn f16_bits(x: f32) -> u16 {
    let x = x.max(0.0);
    if x >= 65504.0 {
        return 0x7bff;
    }
    if x < 6.103_515_6e-5 {
        // Subnormal: units of 2^-24.
        return (x * 16_777_216.0).round() as u16;
    }
    let bits = x.to_bits();
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mant = bits & 0x7f_ffff;
    // Round the 23-bit mantissa to 10 bits; a carry correctly bumps the exponent.
    let h = ((exp as u32) << 10) + (mant >> 13) + ((mant >> 12) & 1);
    h.min(0x7bff) as u16
}

pub use params::MilkyWayParams;

mod params {
    #![allow(dead_code)]
    use bevy::math::Vec4;
    use bevy::render::render_resource::ShaderType;

    /// Mirrors `MilkyWayParams` in milky_way.wgsl.
    #[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
    pub struct MilkyWayParams {
        /// World -> Galactic rotation, by rows (w unused).
        pub row_x: Vec4,
        pub row_y: Vec4,
        pub row_z: Vec4,
        /// 2^EV including auto-exposure.
        pub exposure: f32,
        /// Minimum point angular radius at the current FOV (the brightness scale's pixel).
        pub min_angle: f32,
        /// Half-size of the cube drawn around the camera (AU).
        pub radius: f32,
        /// Pixels dimmer than this are skipped.
        pub cutoff: f32,
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct MilkyWayMaterial {
    #[uniform(0)]
    pub params: MilkyWayParams,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Handle<Image>,
}

impl Material for MilkyWayMaterial {
    fn vertex_shader() -> ShaderRef {
        MILKY_WAY_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        MILKY_WAY_SHADER.into()
    }

    // Opaque, like Bevy's skybox: drawn behind everything (depth ~0 under reverse-Z) without
    // reading the target back, which a full-screen additive blend would (~2 ms on an HD 520).
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![
            layout
                .0
                .get_layout(&[Mesh::ATTRIBUTE_POSITION.at_shader_location(0)])?,
        ];
        // Nothing should be hidden behind the sky: don't write depth.
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = false;
        }
        // The camera is inside the cube: draw its back faces.
        descriptor.primitive.cull_mode = Some(Face::Front);
        Ok(())
    }
}

/// The empty sky texture (filled by the background thread): linear filtering, wrapping in
/// longitude.
fn sky_image() -> Image {
    use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
    let mut image = Image::new(
        Extent3d {
            width: TEX_W as u32,
            height: TEX_H as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0; TEX_W * TEX_H * 8],
        TextureFormat::Rgba16Float,
        // Kept in the main world too, so it can be rewritten.
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

fn cube_mesh() -> Mesh {
    let positions: Vec<[f32; 3]> = (0..8)
        .map(|i| {
            [
                if i & 1 == 0 { -1.0 } else { 1.0 },
                if i & 2 == 0 { -1.0 } else { 1.0 },
                if i & 4 == 0 { -1.0 } else { 1.0 },
            ]
        })
        .collect();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_indices(Indices::U32(CUBE_INDICES.to_vec()))
}

/// What the current texture shows: camera position (world AU) and whether dust was on.
type SkyKey = (DVec3, bool);

#[derive(Resource)]
pub struct MilkyWay {
    pub model: Arc<GalaxyModel>,
    image: Handle<Image>,
    material: Handle<MilkyWayMaterial>,
    shown: Option<SkyKey>,
    task: Option<(SkyKey, JoinHandle<SkyTexture>)>,
    /// Brightest texel of the current sky (per min_angle^2 at EV 0).
    peak: f32,
    /// Where the camera last stopped moving, and since when (seconds).
    settle: (DVec3, f32),
}

#[derive(Component)]
pub struct MilkyWayBody;

pub fn setup_milky_way(
    mut commands: Commands,
    catalog: Res<HygCatalog>,
    exposure: Res<ExposureSettings>,
    fov: Res<CameraFov>,
    images: Option<ResMut<Assets<Image>>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<MilkyWayMaterial>>>,
) {
    // Nothing to draw headless.
    let (Some(mut images), Some(mut meshes), Some(mut materials)) = (images, meshes, materials)
    else {
        return;
    };
    let model = GalaxyModel::new(&catalog);
    let rows = model.gal_from_world.transpose().to_cols_array_2d();
    let row = |a: usize| Vec4::new(rows[a][0] as f32, rows[a][1] as f32, rows[a][2] as f32, 0.0);
    let image = images.add(sky_image());
    let material = materials.add(MilkyWayMaterial {
        params: MilkyWayParams {
            row_x: row(0),
            row_y: row(1),
            row_z: row(2),
            exposure: exposure.factor(),
            min_angle: min_star_angle(fov.0),
            radius: SKY_CUBE_AU,
            cutoff: VISIBLE_CUTOFF,
        },
        texture: image.clone(),
    });
    commands.spawn((
        MilkyWayBody,
        Mesh3d(meshes.add(cube_mesh())),
        MeshMaterial3d(material.clone()),
        Transform::default(),
        Visibility::default(),
        // The shader places the cube around the camera, wherever the entity is.
        NoFrustumCulling,
    ));
    commands.insert_resource(MilkyWay {
        model: Arc::new(model),
        image,
        material,
        shown: None,
        task: None,
        settle: (DVec3::ZERO, 0.0),
        peak: 0.0,
    });
}

/// Starts a recompute in a background thread when the camera has moved [`RECOMPUTE_PC`] and
/// settled, or dust was toggled, and uploads the result when it's done (one texture write per
/// recompute).
#[allow(clippy::too_many_arguments)]
pub fn update_milky_way(
    mw: Option<ResMut<MilkyWay>>,
    time: Res<Time>,
    sky: Res<SkySettings>,
    view: (Res<ExposureSettings>, Res<CameraFov>),
    dust: Res<Dust>,
    dust_settings: Res<DustSettings>,
    cam: Query<&WorldPos, With<FlyCam>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<MilkyWayMaterial>>,
) {
    let Some(mut mw) = mw else { return };
    if mw.task.as_ref().is_some_and(|(_, t)| t.is_finished()) {
        let (key, task) = mw.task.take().unwrap();
        if let Ok(sky) = task.join()
            && let Some(image) = images.get_mut(&mw.image)
        {
            image.data = Some(sky.bytes);
            mw.peak = sky.peak;
            mw.shown = Some(key);
            // The material's bind group still holds the old GPU texture: re-prepare it.
            materials.get_mut(&mw.material);
        }
    }
    let Ok(cam) = cam.single() else { return };
    let now = time.elapsed_secs();
    if mw.settle.0.distance(cam.0) > SETTLE_PC * AU_PER_PC {
        mw.settle = (cam.0, now);
    }
    if !sky.milky_way || mw.task.is_some() {
        return;
    }
    let settled = now - mw.settle.1 >= SETTLE_SECS;
    let would_see = would_see(mw.peak, &view.0, &view.1);
    let stale = mw.shown.is_none_or(|(pos, dust_on)| {
        dust_on != dust_settings.enabled
            || (settled && would_see && pos.distance(cam.0) > RECOMPUTE_PC * AU_PER_PC)
    });
    if stale {
        let key = (cam.0, dust_settings.enabled);
        let model = mw.model.clone();
        let grid = dust_settings.enabled.then(|| dust.0.clone());
        // The first sky uses half the cores; later ones run on one thread, so they take little
        // CPU time from the frame loop (~1 s each on a 2-core laptop).
        let threads = if mw.shown.is_none() {
            std::thread::available_parallelism().map_or(2, |n| n.get() / 2)
        } else {
            1
        };
        let task = std::thread::spawn(move || {
            lower_thread_priority();
            model.render(key.0, grid.as_ref(), threads)
        });
        mw.task = Some((key, task));
    }
}

/// Whether a sky whose brightest texel is `peak` shows at all at this exposure and zoom.
fn would_see(peak: f32, exposure: &ExposureSettings, fov: &CameraFov) -> bool {
    peak * exposure.factor() * min_star_angle(fov.0).powi(2) >= VISIBLE_CUTOFF
}

/// Runs the calling thread (and threads it spawns, which inherit it) at the lowest priority, so
/// the sky recompute only uses CPU time the frame loop leaves idle. On a 2-core laptop it
/// otherwise costs dropped frames while it runs. Elsewhere there are enough cores to not matter.
fn lower_thread_priority() {
    #[cfg(target_os = "linux")]
    // SAFETY: plain syscall on the current thread's id; failure just leaves the priority as is.
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, libc::gettid() as libc::id_t, 19);
    }
}

/// Keeps exposure and zoom in step (writing the material only on change), and hides the glow
/// when it's off or too faint to see anywhere at this exposure, so it then costs no GPU time.
pub fn sync_milky_way_params(
    mw: Option<Res<MilkyWay>>,
    sky: Res<SkySettings>,
    exposure: Res<ExposureSettings>,
    fov: Res<CameraFov>,
    cam: Query<&WorldPos, With<FlyCam>>,
    mut materials: ResMut<Assets<MilkyWayMaterial>>,
    mut vis: Query<&mut Visibility, With<MilkyWayBody>>,
) {
    let Some(mw) = mw else { return };
    let current_sky = match (mw.shown, cam.single()) {
        (Some((pos, _)), Ok(cam)) => pos.distance(cam.0) <= STALE_PC * AU_PER_PC,
        _ => false,
    };
    let shown = sky.milky_way && current_sky && would_see(mw.peak, &exposure, &fov);
    for mut v in &mut vis {
        v.set_if_neq(if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let Some(current) = materials.get(&mw.material).map(|m| m.params) else {
        return;
    };
    let wanted = MilkyWayParams {
        exposure: exposure.factor(),
        min_angle: min_star_angle(fov.0),
        ..current
    };
    if wanted != current
        && let Some(m) = materials.get_mut(&mw.material)
    {
        m.params = wanted;
    }
}

pub struct MilkyWayPlugin;

impl Plugin for MilkyWayPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, MILKY_WAY_SHADER, "milky_way.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<MilkyWayMaterial> {
            prepass_enabled: false,
            shadows_enabled: false,
            ..default()
        })
        .add_systems(Startup, setup_milky_way)
        .add_systems(
            Update,
            (update_milky_way, sync_milky_way_params).in_set(NavSet::View),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gal(l: f64, b: f64) -> DVec3 {
        let (l, b) = (l.to_radians(), b.to_radians());
        DVec3::new(b.cos() * l.cos(), b.cos() * l.sin(), b.sin())
    }

    /// IP flux per steradian -> S10 (V): 10th-magnitude stars per square degree.
    fn s10(s: f64) -> f64 {
        // A V = 10 star's flux on IP's scale (L / d^2 with L = 1 at 10 pc for V = 4.83).
        let f10 = 0.01 * 10f64.powf(-0.4 * (10.0 - 4.83));
        let deg2 = (1f64).to_radians().powi(2);
        s * deg2 / f10
    }

    fn v_s10(rgb: [f64; 3]) -> f64 {
        // The colours have unit luminance, so luminance is the V-band flux.
        s10(0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2])
    }

    /// Catalog stars fainter than `fainter_than` (observed V) in a box around (l, b), as S10.
    fn catalog_s10(cat: &HygCatalog, l: f64, b: f64, half_deg: f64, fainter_than: f64) -> f64 {
        let frame = SkyFrame::galactic().0.transpose();
        let mut flux = 0.0;
        for (i, s) in cat.stars.iter().enumerate() {
            let g = (frame * s.pos).normalize();
            let sl = g.y.atan2(g.x).to_degrees();
            let sb = g.z.asin().to_degrees();
            let dl = ((sl - l + 540.0) % 360.0) - 180.0;
            if dl.abs() > half_deg || (sb - b).abs() > half_deg {
                continue;
            }
            let m = cat.apparent_magnitude_from_sun(i);
            if m > fainter_than {
                flux += 0.01 * 10f64.powf(-0.4 * (m - 4.83));
            }
        }
        let dl = (2.0 * half_deg).to_radians() * b.to_radians().cos();
        s10(flux / (dl * (2.0 * half_deg).to_radians()))
    }

    fn dereddened() -> (HygCatalog, DustGrid) {
        let mut cat = HygCatalog::embedded();
        let grid = DustGrid::embedded();
        crate::dust::deredden_catalog(&mut cat, &grid);
        (cat, grid)
    }

    #[test]
    fn test_local_disk_matches_flynn() {
        let m = GalaxyModel::bare();
        let at_plane = m.emission(DVec3::new(0.0, 0.0, -Z_SUN_PC)).disk;
        assert!((at_plane - RHO_V_LOCAL).abs() < 1e-9);
        let mut column = 0.0;
        let mut z = -5000.0;
        while z < 5000.0 {
            column += m.emission(DVec3::new(0.0, 0.0, z - Z_SUN_PC + 0.5)).disk;
            z += 1.0;
        }
        assert!((column - 24.4).abs() < 0.5, "column {column}");
    }

    #[test]
    fn test_bulge_luminosity() {
        // Integrate the bulge on a 50 pc grid.
        let m = GalaxyModel::bare();
        let mut l = 0.0;
        let h = 50.0;
        for i in -80..80 {
            for j in -80..80 {
                for k in -40..40 {
                    let g = DVec3::new(
                        R0_PC + (i as f64 + 0.5) * h,
                        (j as f64 + 0.5) * h,
                        (k as f64 + 0.5) * h - Z_SUN_PC,
                    );
                    l += m.emission(g).bulge * h * h * h;
                }
            }
        }
        assert!((l / BULGE_L_SUN - 1.0).abs() < 0.05, "{l:e}");
    }

    #[test]
    fn test_completeness_falls_with_distance() {
        let (cat, _) = dereddened();
        let m = GalaxyModel::new(&cat);
        let c = &m.completeness;
        assert!(c.windows(2).all(|w| w[1] <= w[0]));
        // The catalog holds most of the light near the Sun and little of it at 2 kpc.
        assert!(m.completeness_at(30.0) > 0.5, "{:?}", &c[..8]);
        assert!(m.completeness_at(2000.0) < 0.2, "{:?}", &c[70..90]);
    }

    #[test]
    fn test_f16_bits() {
        assert_eq!(f16_bits(0.0), 0);
        assert_eq!(f16_bits(1.0), 0x3c00);
        assert_eq!(f16_bits(2.0), 0x4000);
        assert_eq!(f16_bits(0.5), 0x3800);
        assert_eq!(f16_bits(65504.0), 0x7bff);
        assert_eq!(f16_bits(1e9), 0x7bff);
        assert_eq!(f16_bits(-1.0), 0);
        // Smallest subnormal, and rounding.
        assert_eq!(f16_bits(5.960_464_5e-8), 1);
        // One ulp at 1 is 2^-10 ~ 0.000977.
        assert_eq!(f16_bits(1.0004), 0x3c00);
        assert_eq!(f16_bits(1.0006), 0x3c01);
    }

    #[test]
    fn test_matches_pioneer_10() {
        // Pioneer 10 (stars brighter than V = 7.5 removed): ~280 S10 at b = -8, ~185 S10 at
        // b = -14, for l = 10..70. What IP draws there is the glow plus catalog stars fainter
        // than 7.5. Averaged over l = 20..60 to smooth the dust's clumps.
        let (cat, grid) = dereddened();
        let m = GalaxyModel::new(&cat);
        for (b, pioneer) in [(-8.0, 280.0), (-14.0, 185.0)] {
            let ls = [20.0, 30.0, 40.0, 50.0, 60.0];
            let glow = ls
                .iter()
                .map(|&l| v_s10(m.ray(DVec3::ZERO, gal(l, b), Some(&grid))))
                .sum::<f64>()
                / ls.len() as f64;
            let stars = catalog_s10(&cat, 40.0, b, 20.0, 7.5);
            let total = glow + stars;
            assert!(
                (pioneer / 1.6..pioneer * 1.6).contains(&total),
                "b = {b}: glow {glow:.0} + stars {stars:.0} = {total:.0} S10 vs Pioneer {pioneer}"
            );
        }
    }

    #[test]
    fn test_plane_brighter_than_pole() {
        let (cat, grid) = dereddened();
        let m = GalaxyModel::new(&cat);
        let pole = v_s10(m.ray(DVec3::ZERO, gal(0.0, 90.0), Some(&grid)));
        let plane = v_s10(m.ray(DVec3::ZERO, gal(40.0, -8.0), Some(&grid)));
        assert!(plane > 4.0 * pole, "plane {plane:.0}, pole {pole:.0}");
    }

    #[test]
    fn test_galactic_centre_hidden_baades_window_bright() {
        // Far dust (beyond the map) blacks out the centre in the plane; just below it, in
        // Baade's window, the bulge shows.
        let (cat, grid) = dereddened();
        let m = GalaxyModel::new(&cat);
        let centre = v_s10(m.ray(DVec3::ZERO, gal(0.0, 0.0), Some(&grid)));
        let window = v_s10(m.ray(DVec3::ZERO, gal(1.0, -3.9), Some(&grid)));
        assert!(
            window > 3.0 * centre,
            "window {window:.0}, centre {centre:.0}"
        );
        // Without dust the centre would be the brightest spot on the sky.
        let bare_centre = v_s10(m.ray(DVec3::ZERO, gal(0.0, 0.0), None));
        assert!(bare_centre > 10.0 * window, "{bare_centre:.0}");
    }

    #[test]
    fn test_great_rift_is_dark() {
        // Along the Aquila-Cygnus rift the dust removes most of the light; at high latitude it
        // barely matters.
        let (cat, grid) = dereddened();
        let m = GalaxyModel::new(&cat);
        let ratio = |l: f64, b: f64| {
            let with = v_s10(m.ray(DVec3::ZERO, gal(l, b), Some(&grid)));
            let without = v_s10(m.ray(DVec3::ZERO, gal(l, b), None));
            with / without
        };
        let darkest = (20..=60)
            .step_by(2)
            .map(|l| ratio(l as f64, 2.0))
            .fold(f64::INFINITY, f64::min);
        assert!(darkest < 0.2, "{darkest}");
        assert!(ratio(40.0, 60.0) > 0.8, "{}", ratio(40.0, 60.0));
    }

    #[test]
    fn test_texel_dirs_cover_the_sky() {
        let d = GalaxyModel::texel_dir(0, TEX_H / 2);
        assert!((d.length() - 1.0).abs() < 1e-12);
        // Texel 0 is just past l = 0, b ~ 0; the top row is near the north Galactic pole.
        assert!(d.x > 0.99);
        assert!(GalaxyModel::texel_dir(7, 0).z > 0.99);
    }

    #[test]
    #[ignore]
    fn perf_render_sky() {
        let (cat, grid) = dereddened();
        let m = GalaxyModel::new(&cat);
        let t = std::time::Instant::now();
        let sky = m.render(DVec3::ZERO, Some(&grid), 1);
        eprintln!(
            "perf_render_sky: {TEX_W}x{TEX_H}, one thread, in {:.0} ms",
            t.elapsed().as_secs_f64() * 1000.0
        );
        assert_eq!(sky.bytes.len(), TEX_W * TEX_H * 8);
    }
}
