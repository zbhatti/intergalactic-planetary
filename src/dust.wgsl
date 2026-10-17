// Line-of-sight dust extinction through the 3D dust grid. A step-for-step port of
// DustGrid::extinction_av in dust.rs (same clipping, step rule and sample points; the hardware
// linear filter interpolates the encoded byte before decode_dust, as the CPU does), so the
// de-reddening done on the CPU at startup cancels exactly when viewed from home.
#define_import_path ip::dust

struct DustParams {
    // Render-space position (AU) -> normalized texture coordinates: tex = M p + offset.
    // Rows of M in row_x..row_z (w unused).
    row_x: vec4<f32>,
    row_y: vec4<f32>,
    row_z: vec4<f32>,
    // xyz: offset; w: 1 if dust is on, 0 if off.
    offset: vec4<f32>,
    // x: log_lo, y: log_hi, z: step (pc), w: max steps.
    decode: vec4<f32>,
    // rgb: A_channel / A_V (CCM89, R_V = 3.1); w: AU per pc.
    k_rgb: vec4<f32>,
};

fn dust_tex(p: DustParams, x: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(p.row_x.xyz, x), dot(p.row_y.xyz, x), dot(p.row_z.xyz, x)) + p.offset.xyz;
}

fn decode_dust(p: DustParams, texel: f32) -> f32 {
    let x = texel * 255.0;
    let lo = pow(10.0, p.decode.x);
    if x < 1.0 {
        return lo * x;
    }
    return pow(10.0, p.decode.x + (x - 1.0) / 254.0 * (p.decode.y - p.decode.x));
}

// V-band extinction (mag) between two render-space points (AU).
fn dust_av(
    tex: texture_3d<f32>,
    samp: sampler,
    p: DustParams,
    from_render: vec3<f32>,
    to_render: vec3<f32>,
) -> f32 {
    if p.offset.w == 0.0 {
        return 0.0;
    }
    let a = dust_tex(p, from_render);
    let b = dust_tex(p, to_render);
    let d = b - a;
    var t_in = 0.0;
    var t_out = 1.0;
    for (var k = 0; k < 3; k++) {
        if abs(d[k]) < 1e-30 {
            if a[k] < 0.0 || a[k] > 1.0 {
                return 0.0;
            }
            continue;
        }
        var e = (0.0 - a[k]) / d[k];
        var x = (1.0 - a[k]) / d[k];
        if e > x {
            let tmp = e;
            e = x;
            x = tmp;
        }
        t_in = max(t_in, e);
        t_out = min(t_out, x);
        if t_in >= t_out {
            return 0.0;
        }
    }
    let len_pc = length(to_render - from_render) / p.k_rgb.w * (t_out - t_in);
    let steps = u32(clamp(ceil(len_pc / p.decode.z), 1.0, p.decode.w));
    var sum = 0.0;
    for (var k = 0u; k < steps; k++) {
        let t = t_in + (t_out - t_in) * (f32(k) + 0.5) / f32(steps);
        let v = textureSampleLevel(tex, samp, a + d * t, 0.0).r;
        sum += decode_dust(p, v);
    }
    return sum * len_pc / f32(steps);
}

// Per-channel transmission for a V-band extinction.
fn dust_transmission(p: DustParams, av: f32) -> vec3<f32> {
    return pow(vec3<f32>(10.0), -0.4 * av * p.k_rgb.rgb);
}
