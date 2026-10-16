// Catalog starfield: one camera-facing billboard per star, sized and lit with the same
// model as the sphere stars in stars.rs (all constants arrive as uniforms from Rust).

#import bevy_pbr::mesh_functions::get_world_from_local
#import bevy_pbr::mesh_view_bindings::view

struct StarfieldParams {
    // 2^EV
    exposure: f32,
    brightness_scale: f32,
    max_brightness: f32,
    // Minimum angular radius, radians (MIN_STAR_ANGLE).
    min_angle: f32,
    au_per_pc: f32,
    // Distance clamp for the 1/d^2 law, parsecs.
    min_dist_pc: f32,
    // Points fade out between these camera distances (AU); a sphere takes over up close.
    fade_near_au: f32,
    fade_far_au: f32,
};

@group(2) @binding(0) var<uniform> params: StarfieldParams;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    // Star position relative to its chunk, AU.
    @location(0) position: vec3<f32>,
    // xy: billboard corner (-1..1); z: visual luminosity (solar); w: unused.
    @location(1) star: vec4<f32>,
    // Linear RGB spectral tint.
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    let world = (get_world_from_local(v.instance_index) * vec4<f32>(v.position, 1.0)).xyz;
    let view_pos = (view.view_from_world * vec4<f32>(world, 1.0)).xyz;
    let dist_au = length(view_pos);

    let dist_pc = max(dist_au / params.au_per_pc, params.min_dist_pc);
    var brightness = min(
        params.exposure * params.brightness_scale * v.star.z / (dist_pc * dist_pc),
        params.max_brightness,
    );
    brightness *= smoothstep(params.fade_near_au, params.fade_far_au, dist_au);

    // Stars are far smaller than MIN_STAR_ANGLE at these distances, so the min-angle
    // branch of star_visual_radius applies: radius = distance * angle.
    let radius = dist_au * params.min_angle;
    let offset = vec3<f32>(v.star.xy * radius, 0.0);

    var out: VertexOutput;
    out.clip_position = view.clip_from_view * vec4<f32>(view_pos + offset, 1.0);
    out.corner = v.star.xy;
    out.color = v.color.rgb * brightness;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Flat emissive disk, matching the emissive sphere stars.
    if dot(in.corner, in.corner) > 1.0 {
        discard;
    }
    return vec4<f32>(in.color, 1.0);
}
