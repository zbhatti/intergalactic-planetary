// Nebulae: an emissive sphere ray-traced inside a cube mesh (back faces only, so each pixel is
// shaded once whether the camera is outside or inside). Brightness follows the path length
// through the sphere, so a nebula looks right from afar and when you fly into it.
// Additive (AlphaMode::Add); constants arrive as uniforms from Rust.

#import bevy_pbr::mesh_functions::get_world_from_local
#import bevy_pbr::mesh_view_bindings::view
#import ip::dust::{DustParams, dust_av, dust_transmission}

struct NebulaParams {
    // 2^EV (including auto-exposure).
    exposure: f32,
    // Minimum point angular radius (radians) at the current FOV: the pixel scale that the star
    // brightness model is calibrated to, so extended and point sources share one scale.
    min_angle: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(2) @binding(0) var<uniform> params: NebulaParams;
@group(2) @binding(1) var dust_texture: texture_3d<f32>;
@group(2) @binding(2) var dust_sampler: sampler;
@group(2) @binding(3) var<uniform> dust: DustParams;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // rgb: linear colour x central surface brightness (per min_angle^2, EV 0); a: radius (AU).
    @location(1) nebula: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) center: vec3<f32>,
    @location(2) nebula: vec4<f32>,
};

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    let model = get_world_from_local(v.instance_index);
    let world = (model * vec4<f32>(v.position, 1.0)).xyz;
    var out: VertexOutput;
    out.clip_position = view.clip_from_world * vec4<f32>(world, 1.0);
    out.world = world;
    out.center = (model * vec4<f32>(0.0, 0.0, 0.0, 1.0)).xyz;
    // Dust between the camera and the nebula's centre dims and reddens it (the line ratios are
    // intrinsic, so this is the only reddening applied).
    let av = dust_av(dust_texture, dust_sampler, dust, view.world_position, out.center);
    out.nebula = vec4<f32>(v.nebula.rgb * dust_transmission(dust, av), v.nebula.a);
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let radius = in.nebula.a;
    let ro = view.world_position;
    let rd = normalize(in.world - ro);
    let oc = ro - in.center;
    let b = dot(oc, rd);
    // Perpendicular offset of the sphere centre from the ray, computed without squaring the
    // (possibly huge) camera distance, which would lose all precision in f32.
    let perp = oc - b * rd;
    let d2 = dot(perp, perp);
    let r2 = radius * radius;
    if d2 >= r2 {
        discard;
    }
    let h = sqrt(r2 - d2);
    // Chord length through the sphere in front of the camera: 2h from outside, less from inside.
    var len: f32;
    if -b - h >= 0.0 {
        len = 2.0 * h;
    } else {
        len = max(-b + h, 0.0);
    }
    // Centrally concentrated glow: (chord / diameter)^2 of the central surface brightness.
    let f = len / (2.0 * radius);
    let brightness = params.exposure * params.min_angle * params.min_angle * f * f;
    return vec4<f32>(in.nebula.rgb * brightness, 0.0);
}
