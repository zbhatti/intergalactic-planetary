// The Milky Way's unresolved starlight: a cube around the camera (back faces only) textured with
// the sky computed on the CPU (milky_way.rs) in Galactic longitude/latitude. Opaque (it is the
// background) and depth tested so planets hide it; brightness on the same scale as stars and
// nebulae, which add on top.

#import bevy_pbr::mesh_view_bindings::view

struct MilkyWayParams {
    // World -> Galactic rotation, by rows (w unused).
    row_x: vec4<f32>,
    row_y: vec4<f32>,
    row_z: vec4<f32>,
    // 2^EV (including auto-exposure).
    exposure: f32,
    // Minimum point angular radius at the current FOV: the brightness scale's pixel.
    min_angle: f32,
    // Half-size of the cube (AU).
    radius: f32,
    // Pixels dimmer than this (invisible after tonemapping) are skipped.
    cutoff: f32,
};

@group(2) @binding(0) var<uniform> params: MilkyWayParams;
@group(2) @binding(1) var sky_texture: texture_2d<f32>;
@group(2) @binding(2) var sky_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    // Unnormalized view direction (world axes).
    @location(0) dir: vec3<f32>,
};

const PI: f32 = 3.14159265358979;

@vertex
fn vertex(@location(0) position: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    let world = view.world_position + position * params.radius;
    out.clip_position = view.clip_from_world * vec4<f32>(world, 1.0);
    out.dir = position;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let d = normalize(in.dir);
    let g = vec3<f32>(dot(params.row_x.xyz, d), dot(params.row_y.xyz, d), dot(params.row_z.xyz, d));
    let lon = atan2(g.y, g.x);
    let lat = asin(clamp(g.z, -1.0, 1.0));
    let uv = vec2<f32>(fract(lon / (2.0 * PI) + 1.0), 0.5 - lat / PI);
    let rgb = textureSampleLevel(sky_texture, sky_sampler, uv, 0.0).rgb
        * (params.exposure * params.min_angle * params.min_angle);
    if max(rgb.r, max(rgb.g, rgb.b)) < params.cutoff {
        discard;
    }
    return vec4<f32>(rgb, 1.0);
}
