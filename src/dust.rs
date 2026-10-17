//! Interstellar dust from the Edenhofer et al. (2024) 3D dust map, 1.25 kpc around the Sun.
//!
//! The grid (`data/dust_edenhofer.bin`, from `tools/build_dust.py`) holds A_V per parsec in 12 pc
//! voxels on a heliocentric Galactic Cartesian grid, |z| <= ~400 pc. Extinction along a line of
//! sight is the integral of that density.
//!
//! **The catalog brightnesses already include dust.** HYG luminosities (and the exoplanet hosts'
//! V magnitudes) come from observed magnitudes, i.e. after dimming by the dust between the star and
//! us. So at startup each star is de-reddened by the dust between it and the Sun
//! ([`deredden_catalog`]), and at runtime the dust between it and the *camera* is applied. From
//! home the two cancel and stars look as observed; elsewhere they dim and redden correctly.
//! That only works if every path computes the same integral: [`DustGrid::extinction_av`] is the
//! reference, and `dust.wgsl` reproduces it step for step (texel-centre sampling, trilinear
//! interpolation of the encoded byte *before* decoding, as GPU filtering does, the same step rule).

use crate::constants::AU_PER_PC;
use crate::hyg::HygCatalog;
use crate::sky::SkyFrame;
use bevy::math::{DMat3, DVec3};
use bevy::prelude::*;

static DUST_BIN: &[u8] = include_bytes!("../data/dust_edenhofer.bin");

const MAGIC: &[u8; 8] = b"IPDUST\0\0";
const HEADER_BYTES: usize = 52;

/// Integration step along a line of sight (one voxel; trilinear sampling smooths between them).
pub const STEP_PC: f64 = 12.0;
/// Upper bound on steps: longer than any path through the box (~3.6 kpc diagonal / 12 pc).
pub const MAX_STEPS: u32 = 320;

/// Total-to-selective extinction ratio of the diffuse ISM.
pub const R_V: f64 = 3.1;
/// Effective wavelengths (nm) used for IP's red, green and blue channels when reddening.
pub const CHANNEL_NM: [f64; 3] = [620.0, 550.0, 460.0];

/// Cardelli, Clayton & Mathis (1989) extinction curve, optical/near-IR (0.9..3.3 um^-1):
/// A(lambda) / A(V).
pub fn ccm89(lambda_nm: f64, r_v: f64) -> f64 {
    let x = 1000.0 / lambda_nm; // inverse microns
    if x < 1.1 {
        let a = 0.574 * x.powf(1.61);
        let b = -0.527 * x.powf(1.61);
        return a + b / r_v;
    }
    let y = x - 1.82;
    let a = 1.0 + 0.17699 * y - 0.50447 * y.powi(2) - 0.02427 * y.powi(3)
        + 0.72085 * y.powi(4)
        + 0.01979 * y.powi(5)
        - 0.77530 * y.powi(6)
        + 0.32999 * y.powi(7);
    let b = 1.41338 * y + 2.28305 * y.powi(2) + 1.07233 * y.powi(3)
        - 5.38434 * y.powi(4)
        - 0.62251 * y.powi(5)
        + 5.30260 * y.powi(6)
        - 2.09002 * y.powi(7);
    a + b / r_v
}

/// A_channel / A_V for IP's (red, green, blue).
pub fn channel_extinction_ratios() -> [f32; 3] {
    CHANNEL_NM.map(|nm| ccm89(nm, R_V) as f32)
}

/// Per-channel transmission for a V-band extinction of `av` magnitudes.
pub fn transmission(av: f64) -> [f32; 3] {
    channel_extinction_ratios().map(|k| 10f64.powf(-0.4 * av * k as f64) as f32)
}

#[derive(Debug)]
pub struct DustGrid {
    pub n: [usize; 3],
    pub voxel_pc: f64,
    /// Galactic coordinates (pc) of the grid's minimum corner.
    pub min_pc: DVec3,
    pub log_lo: f64,
    pub log_hi: f64,
    pub data: &'static [u8],
    /// World -> Galactic rotation.
    pub gal_from_world: DMat3,
}

#[derive(Debug, PartialEq)]
pub enum DustError {
    BadHeader,
    Truncated,
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn f32_at(b: &[u8], at: usize) -> f64 {
    f32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as f64
}

impl DustGrid {
    pub fn parse(b: &'static [u8]) -> Result<Self, DustError> {
        if b.len() < HEADER_BYTES || &b[..8] != MAGIC || u32_at(b, 8) != 1 {
            return Err(DustError::BadHeader);
        }
        let n = [
            u32_at(b, 12) as usize,
            u32_at(b, 16) as usize,
            u32_at(b, 20) as usize,
        ];
        let data = &b[HEADER_BYTES..];
        if data.len() != n[0] * n[1] * n[2] {
            return Err(DustError::Truncated);
        }
        Ok(Self {
            n,
            voxel_pc: f32_at(b, 24),
            min_pc: DVec3::new(f32_at(b, 28), f32_at(b, 32), f32_at(b, 36)),
            log_lo: f32_at(b, 40),
            log_hi: f32_at(b, 44),
            data,
            gal_from_world: SkyFrame::galactic().0.transpose(),
        })
    }

    pub fn embedded() -> Self {
        Self::parse(DUST_BIN).expect("embedded dust grid is valid (checked by tests)")
    }

    /// Grid size in pc along Galactic X, Y, Z.
    pub fn size_pc(&self) -> DVec3 {
        DVec3::new(self.n[0] as f64, self.n[1] as f64, self.n[2] as f64) * self.voxel_pc
    }

    /// World position (AU) -> normalized texture coordinates (0..1 across the grid).
    pub fn tex_coord(&self, world_au: DVec3) -> DVec3 {
        let gal_pc = self.gal_from_world * world_au / AU_PER_PC;
        (gal_pc - self.min_pc) / self.size_pc()
    }

    /// The encoded byte as a 0..1 value (like an R8Unorm texel).
    fn texel(&self, i: [usize; 3]) -> f64 {
        self.data[i[0] + self.n[0] * (i[1] + self.n[1] * i[2])] as f64 / 255.0
    }

    /// Decodes an interpolated 0..1 texel value to A_V per pc. Below code 1 the value fades
    /// linearly to zero (0 = no dust); above it's logarithmic.
    pub fn decode(&self, unorm: f64) -> f64 {
        let x = unorm * 255.0;
        let lo = 10f64.powf(self.log_lo);
        if x < 1.0 {
            lo * x
        } else {
            10f64.powf(self.log_lo + (x - 1.0) / 254.0 * (self.log_hi - self.log_lo))
        }
    }

    /// A_V per pc at texture coordinate `t`: trilinear interpolation of the encoded values with
    /// texel centres at (i + 0.5) / n and clamp-to-edge, exactly as a linear-filtered GPU sample,
    /// then decoded.
    pub fn density(&self, t: DVec3) -> f64 {
        let mut lo = [0usize; 3];
        let mut hi = [0usize; 3];
        let mut f = [0f64; 3];
        for a in 0..3 {
            let n = self.n[a];
            let s = (t[a] * n as f64 - 0.5).clamp(0.0, (n - 1) as f64);
            let i = s.floor();
            lo[a] = i as usize;
            hi[a] = (lo[a] + 1).min(n - 1);
            f[a] = s - i;
        }
        let mut v = 0.0;
        for c in 0..8 {
            let pick = |a: usize| c >> a & 1 == 1;
            let idx = [0, 1, 2].map(|a| if pick(a) { hi[a] } else { lo[a] });
            let w: f64 = (0..3)
                .map(|a| if pick(a) { f[a] } else { 1.0 - f[a] })
                .product();
            v += w * self.texel(idx);
        }
        self.decode(v)
    }

    /// V-band extinction (magnitudes) between two world positions (AU). Only the part of the
    /// segment inside the grid counts; outside it IP has no dust data.
    pub fn extinction_av(&self, from_au: DVec3, to_au: DVec3) -> f64 {
        let a = self.tex_coord(from_au);
        let b = self.tex_coord(to_au);
        let Some((t0, t1)) = clip_unit_box(a, b) else {
            return 0.0;
        };
        let len_pc = (to_au - from_au).length() / AU_PER_PC * (t1 - t0);
        let steps = ((len_pc / STEP_PC).ceil() as u32).clamp(1, MAX_STEPS);
        let dl = len_pc / steps as f64;
        let mut sum = 0.0;
        for k in 0..steps {
            let t = t0 + (t1 - t0) * (k as f64 + 0.5) / steps as f64;
            sum += self.density(a + (b - a) * t);
        }
        sum * dl
    }
}

/// Parameter range [t0, t1] of segment a->b (t in 0..1) inside the unit box, if any.
pub fn clip_unit_box(a: DVec3, b: DVec3) -> Option<(f64, f64)> {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for k in 0..3 {
        if d[k].abs() < 1e-30 {
            if a[k] < 0.0 || a[k] > 1.0 {
                return None;
            }
            continue;
        }
        let (mut e, mut x) = ((0.0 - a[k]) / d[k], (1.0 - a[k]) / d[k]);
        if e > x {
            std::mem::swap(&mut e, &mut x);
        }
        t0 = t0.max(e);
        t1 = t1.min(x);
        if t0 >= t1 {
            return None;
        }
    }
    Some((t0, t1))
}

/// The embedded grid, shared by the CPU paths.
#[derive(Resource)]
pub struct Dust(pub DustGrid);

/// Whether dust dims and reddens what's behind it (Esc menu). Off shows the stars as they'd be
/// without dust (de-reddened), which is brighter than observed from Earth.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct DustSettings {
    pub enabled: bool,
}

impl Default for DustSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Removes the dust between each star and the Sun from its (observed) luminosity, and records
/// that extinction. Stars without a measured distance are left alone (their 1 kpc placeholder
/// path would be made up).
pub fn deredden_catalog(catalog: &mut HygCatalog, dust: &DustGrid) {
    // ~110k line integrals: 0.2 s in release, several seconds in the unoptimized dev build, so
    // split them across threads.
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = catalog.stars.len().div_ceil(threads).max(1);
    let mut av = vec![0f32; catalog.stars.len()];
    std::thread::scope(|scope| {
        for (stars, out) in catalog.stars.chunks_mut(chunk).zip(av.chunks_mut(chunk)) {
            scope.spawn(move || {
                for (s, a) in stars.iter_mut().zip(out.iter_mut()) {
                    if !s.dist_known {
                        continue;
                    }
                    *a = dust.extinction_av(DVec3::ZERO, s.pos) as f32;
                    s.lum *= 10f32.powf(0.4 * *a);
                }
            });
        }
    });
    catalog.av_from_sun = av;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gal_dir(l: f64, b: f64) -> DVec3 {
        SkyFrame::galactic().dir(l, b)
    }

    fn av_to(dust: &DustGrid, l: f64, b: f64, d_pc: f64) -> f64 {
        dust.extinction_av(DVec3::ZERO, gal_dir(l, b) * d_pc * AU_PER_PC)
    }

    #[test]
    fn test_embedded_grid_parses() {
        let d = DustGrid::embedded();
        assert_eq!(
            d.n,
            [208, 208, 66],
            "matches data/dust_edenhofer.bin.manifest.txt"
        );
        assert_eq!(d.voxel_pc, 12.0);
        // The Sun is near the middle in X/Y and inside the slab in Z.
        let t = d.tex_coord(DVec3::ZERO);
        assert!((t.x - 0.5).abs() < 0.01 && (t.y - 0.5).abs() < 0.01 && (0.3..0.7).contains(&t.z));
    }

    #[test]
    fn test_ccm89() {
        // Johnson B (440 nm) / V: ~1.32 for R_V = 3.1; V itself 1.0; red light is dimmed less.
        assert!(
            (ccm89(440.0, 3.1) - 1.32).abs() < 0.03,
            "{}",
            ccm89(440.0, 3.1)
        );
        assert!((ccm89(549.0, 3.1) - 1.0).abs() < 0.02);
        let k = channel_extinction_ratios();
        assert!(k[0] < k[1] && k[1] < k[2], "{k:?}");
        let t = transmission(1.0);
        assert!(t[0] > t[1] && t[1] > t[2], "reddening: {t:?}");
    }

    #[test]
    fn test_taurus_cloud_is_where_it_should_be() {
        // The Taurus molecular clouds: l ~ 172, b ~ -15, ~130-160 pc away. Extinction should jump
        // across them, while the mirror direction b = +15 (which a flipped Z axis would swap in)
        // stays nearly clear. This guards the grid's axis orientation.
        let d = DustGrid::embedded();
        let near = av_to(&d, 172.0, -15.0, 100.0);
        let far = av_to(&d, 172.0, -15.0, 220.0);
        assert!(far - near > 0.5, "Taurus: A_V {near:.2} -> {far:.2}");
        let mirror = av_to(&d, 172.0, 15.0, 220.0);
        assert!(mirror < 0.3 * far, "mirror A_V {mirror:.2} vs {far:.2}");
    }

    #[test]
    fn test_plane_versus_pole() {
        let d = DustGrid::embedded();
        // ~1 mag of A_V per kpc in the plane toward the Galactic centre (the paper's maps).
        let gc = av_to(&d, 0.0, 0.0, 1000.0);
        assert!((0.5..3.0).contains(&gc), "toward GC: {gc}");
        // Out of the plane there's almost nothing.
        let pole = av_to(&d, 0.0, 90.0, 400.0);
        assert!(pole < 0.1, "toward NGP: {pole}");
        // Nothing at all within the dust-free Local Bubble's inner 50 pc.
        assert!(av_to(&d, 0.0, 0.0, 40.0) < 0.05);
    }

    #[test]
    fn test_integral_is_symmetric_and_additive_in_direction() {
        let d = DustGrid::embedded();
        let a = gal_dir(30.0, 2.0) * 300.0 * AU_PER_PC;
        let b = gal_dir(80.0, -3.0) * 900.0 * AU_PER_PC;
        let ab = d.extinction_av(a, b);
        let ba = d.extinction_av(b, a);
        assert!((ab - ba).abs() < 1e-9 * ab.max(1.0));
        // Beyond the box nothing is added.
        let out = gal_dir(30.0, 2.0) * 5000.0 * AU_PER_PC;
        let edge = gal_dir(30.0, 2.0) * 2000.0 * AU_PER_PC;
        assert!(
            (d.extinction_av(DVec3::ZERO, out) - d.extinction_av(DVec3::ZERO, edge)).abs() < 1e-9
        );
    }

    #[test]
    fn test_clip_unit_box() {
        let inside = DVec3::splat(0.5);
        assert_eq!(
            clip_unit_box(inside, inside + DVec3::X * 0.2),
            Some((0.0, 1.0))
        );
        let (t0, t1) =
            clip_unit_box(DVec3::new(-1.0, 0.5, 0.5), DVec3::new(2.0, 0.5, 0.5)).unwrap();
        assert!((t0 - 1.0 / 3.0).abs() < 1e-12 && (t1 - 2.0 / 3.0).abs() < 1e-12);
        assert_eq!(
            clip_unit_box(DVec3::new(-1.0, 2.0, 0.5), DVec3::new(2.0, 2.0, 0.5)),
            None
        );
    }

    #[test]
    fn test_dereddening_keeps_stars_as_observed_from_home() {
        // From the Sun, the runtime extinction undoes the de-reddening exactly: observed
        // magnitudes are unchanged, including for a star behind real dust.
        let dust = DustGrid::embedded();
        let mut cat = HygCatalog::embedded();
        let before: Vec<f64> = ["Sirius", "Vega", "Polaris", "Deneb"]
            .iter()
            .map(|n| cat.apparent_magnitude(cat.index_of_name(n).unwrap(), DVec3::ZERO))
            .collect();
        deredden_catalog(&mut cat, &dust);
        for (k, n) in ["Sirius", "Vega", "Polaris", "Deneb"].iter().enumerate() {
            let i = cat.index_of_name(n).unwrap();
            let intrinsic = cat.apparent_magnitude(i, DVec3::ZERO);
            let observed = intrinsic + dust.extinction_av(DVec3::ZERO, cat.stars[i].pos);
            assert!(
                (observed - before[k]).abs() < 1e-4,
                "{n}: {observed} vs {}",
                before[k]
            );
            assert!((cat.apparent_magnitude_from_sun(i) - before[k]).abs() < 1e-4);
        }
        // Deneb (~800 pc in Cygnus, in the plane) sits behind real dust.
        let deneb = cat.index_of_name("Deneb").unwrap();
        assert!(
            cat.av_from_sun[deneb] > 0.1,
            "Deneb A_V {}",
            cat.av_from_sun[deneb]
        );
    }
}

pub use params::DustParams;

mod params {
    // ShaderType generates per-field layout checks that rustc reports as unused.
    #![allow(dead_code)]
    use bevy::math::Vec4;
    use bevy::render::render_resource::ShaderType;

    /// Mirrors `DustParams` in dust.wgsl.
    #[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
    pub struct DustParams {
        pub row_x: Vec4,
        pub row_y: Vec4,
        pub row_z: Vec4,
        pub offset: Vec4,
        pub decode: Vec4,
        pub k_rgb: Vec4,
    }
}

impl DustParams {
    /// Render-space -> texture mapping for a floating origin at `origin` (world AU).
    pub fn new(grid: &DustGrid, origin: DVec3, enabled: bool) -> Self {
        let size = grid.size_pc();
        let row = |a: usize| {
            let r = grid.gal_from_world.row(a) / (AU_PER_PC * size[a]);
            r.as_vec3().extend(0.0)
        };
        let gal_origin = grid.gal_from_world * origin / AU_PER_PC;
        let offset = (gal_origin - grid.min_pc) / size;
        let k = channel_extinction_ratios();
        Self {
            row_x: row(0),
            row_y: row(1),
            row_z: row(2),
            offset: offset.as_vec3().extend(if enabled { 1.0 } else { 0.0 }),
            decode: Vec4::new(
                grid.log_lo as f32,
                grid.log_hi as f32,
                STEP_PC as f32,
                MAX_STEPS as f32,
            ),
            k_rgb: Vec4::new(k[0], k[1], k[2], AU_PER_PC as f32),
        }
    }
}

/// The dust grid as a 3D texture (R8Unorm, linear filtering, clamp to edge).
pub fn dust_image(grid: &DustGrid) -> Image {
    use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
    use bevy::render::render_asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let mut image = Image::new(
        Extent3d {
            width: grid.n[0] as u32,
            height: grid.n[1] as u32,
            depth_or_array_layers: grid.n[2] as u32,
        },
        TextureDimension::D3,
        grid.data.to_vec(),
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// V-band extinction (mag) between two world positions, or 0 with dust off.
pub fn av_between(dust: &Dust, settings: &DustSettings, from: DVec3, to: DVec3) -> f64 {
    if settings.enabled {
        dust.0.extinction_av(from, to)
    } else {
        0.0
    }
}

#[derive(Resource)]
pub struct DustTexture(pub Handle<Image>);

pub fn setup_dust_texture(
    mut commands: Commands,
    dust: Res<Dust>,
    images: Option<ResMut<Assets<Image>>>,
) {
    if let Some(mut images) = images {
        commands.insert_resource(DustTexture(images.add(dust_image(&dust.0))));
    }
}

pub const DUST_SHADER: Handle<Shader> =
    bevy::asset::weak_handle!("8c2b9d4e-1f7a-4e3b-a6d5-0b9c8e7f6a21");

pub struct DustPlugin;

impl Plugin for DustPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::load_internal_asset!(app, DUST_SHADER, "dust.wgsl", Shader::from_wgsl);
        app.init_resource::<DustSettings>()
            .add_systems(PreStartup, setup_dust_texture);
    }
}
