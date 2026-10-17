//! Workarounds for older GPUs and drivers.
//!
//! - Vulkan debug labels (#10): wgpu turns on `InstanceFlags::DEBUG` in debug builds, which makes
//!   it call `vkCmdBeginDebugUtilsLabelEXT`. Some drivers (Mesa's Haswell Vulkan driver) don't
//!   provide it, and the render thread aborts. IP leaves `DEBUG` off by default; `WGPU_DEBUG=1`
//!   turns it back on. Validation stays as wgpu sets it (it only runs when the layer is installed).
//! - Bloom (#11): Bevy's bloom renders into `Rg11b10Ufloat`, which isn't renderable everywhere
//!   (e.g. the OpenGL backend on Mesa Haswell). Bloom is only added when the device can render to
//!   that format. `IP_BLOOM=0` turns it off by hand.
//! - GPU occlusion culling on OpenGL: Bevy 0.16 thinks GL supports it, then fails to translate
//!   the depth-downsample compute shader (`textureLoad` from a depth texture) and panics. When
//!   only GL is allowed (`WGPU_BACKEND=gl`), IP withholds `MULTI_DRAW_INDIRECT`, which makes Bevy
//!   fall back to preprocessing without GPU culling. IP doesn't use occlusion culling anyway.

use bevy::{
    core_pipeline::bloom::Bloom,
    prelude::*,
    render::{
        render_resource::{DownlevelFlags, TextureFormat, TextureUsages, WgpuFeatures as Features},
        renderer::{RenderAdapter, RenderDevice},
        settings::{Backends, InstanceFlags, WgpuSettings},
    },
};

/// Bevy's bloom texture format (bevy_core_pipeline::bloom::BLOOM_TEXTURE_FORMAT, private there).
const BLOOM_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rg11b10Ufloat;

/// wgpu instance flags: the build default without `DEBUG`, then the `WGPU_*` env overrides.
pub fn instance_flags() -> InstanceFlags {
    (InstanceFlags::from_build_config() - InstanceFlags::DEBUG).with_env()
}

/// Features to withhold from the device so Bevy doesn't build pipelines the backend can't run.
pub fn disabled_features(backends: Backends) -> Option<Features> {
    let gl_only = !backends.intersects(Backends::VULKAN | Backends::METAL | Backends::DX12);
    gl_only.then_some(Features::MULTI_DRAW_INDIRECT)
}

pub fn wgpu_settings() -> WgpuSettings {
    let defaults = WgpuSettings::default();
    WgpuSettings {
        instance_flags: instance_flags(),
        disabled_features: disabled_features(defaults.backends.unwrap_or(Backends::all())),
        ..defaults
    }
}

/// Whether wgpu will accept `RENDER_ATTACHMENT` on an `Rg11b10Ufloat` texture. Mirrors
/// wgpu-core's `describe_format_features`: with adapter-specific format features enabled, or on a
/// downlevel backend such as GL, the adapter's own answer counts; otherwise only the WebGPU
/// guarantee, which needs `RG11B10UFLOAT_RENDERABLE`.
pub fn rg11b10_renderable(
    device_features: Features,
    webgpu_format_support: bool,
    adapter_allows: bool,
) -> bool {
    if device_features.contains(Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        || !webgpu_format_support
    {
        adapter_allows
    } else {
        device_features.contains(Features::RG11B10UFLOAT_RENDERABLE)
    }
}

fn bloom_disabled_by_env(value: Option<&str>) -> bool {
    value == Some("0")
}

/// Adds `Bloom::NATURAL` to `camera` if the GPU can render it and `IP_BLOOM` isn't `0`.
/// Headless (no render device), nothing is added.
pub fn add_bloom_if_supported(
    commands: &mut Commands,
    camera: Entity,
    device: Option<&RenderDevice>,
    adapter: Option<&RenderAdapter>,
) {
    let (Some(device), Some(adapter)) = (device, adapter) else {
        return;
    };
    if bloom_disabled_by_env(std::env::var("IP_BLOOM").ok().as_deref()) {
        info!("bloom off (IP_BLOOM=0)");
        return;
    }
    let renderable = rg11b10_renderable(
        device.features(),
        adapter
            .get_downlevel_capabilities()
            .flags
            .contains(DownlevelFlags::WEBGPU_TEXTURE_FORMAT_SUPPORT),
        adapter
            .get_texture_format_features(BLOOM_TEXTURE_FORMAT)
            .allowed_usages
            .contains(TextureUsages::RENDER_ATTACHMENT),
    );
    if renderable {
        commands.entity(camera).insert(Bloom::NATURAL);
    } else {
        warn!("bloom off: this GPU can't render to {BLOOM_TEXTURE_FORMAT:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_labels_off_by_default() {
        // WGPU_DEBUG isn't set in CI, so DEBUG must be absent (it crashes Mesa Haswell, #10).
        if std::env::var_os("WGPU_DEBUG").is_none() {
            assert!(!instance_flags().contains(InstanceFlags::DEBUG));
        }
    }

    #[test]
    fn test_bloom_needs_a_renderable_format() {
        let none = Features::empty();
        let adapter_specific = Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        // GL on Mesa Haswell (#11): downlevel, and the adapter says no.
        assert!(!rg11b10_renderable(none, false, false));
        // Downlevel but the adapter can render it.
        assert!(rg11b10_renderable(none, false, true));
        // Full WebGPU support: only the feature flag counts.
        assert!(!rg11b10_renderable(none, true, true));
        assert!(rg11b10_renderable(
            Features::RG11B10UFLOAT_RENDERABLE,
            true,
            false
        ));
        // Adapter-specific formats enabled: the adapter's answer counts.
        assert!(!rg11b10_renderable(adapter_specific, true, false));
        assert!(rg11b10_renderable(adapter_specific, true, true));
    }

    #[test]
    fn test_gl_only_withholds_gpu_culling() {
        assert_eq!(
            disabled_features(Backends::GL),
            Some(Features::MULTI_DRAW_INDIRECT)
        );
        assert_eq!(disabled_features(Backends::all()), None);
        assert_eq!(disabled_features(Backends::VULKAN), None);
    }

    #[test]
    fn test_ip_bloom_env() {
        assert!(bloom_disabled_by_env(Some("0")));
        assert!(!bloom_disabled_by_env(Some("1")));
        assert!(!bloom_disabled_by_env(None));
    }
}
