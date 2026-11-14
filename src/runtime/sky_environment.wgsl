// Claydash's analytic atmosphere and world-fixed night stars, mapped to Z up.
struct Environment {
    sun: vec4<f32>, color_haze: vec4<f32>, night: vec4<f32>,
};
@group(0) @binding(0) var<uniform> environment: Environment;
struct Ray { @builtin(position) clip: vec4<f32>, @location(0) ray: vec3<f32> };
@vertex fn vs(@builtin(vertex_index) vertex: u32, @builtin(instance_index) face: u32) -> Ray {
    let p = vec2(f32((vertex << 1u) & 2u), f32(vertex & 2u)) * 2.0 - 1.0;
    // Texture Y grows down. Cube faces are +X,-X,+Y,-Y,+Z,-Z (WebGPU convention).
    let uv = vec2(p.x, -p.y);
    var ray: vec3<f32>;
    switch face {
        case 0u: { ray = vec3(1.0, -uv.y, -uv.x); }
        case 1u: { ray = vec3(-1.0, -uv.y, uv.x); }
        case 2u: { ray = vec3(uv.x, 1.0, uv.y); }
        case 3u: { ray = vec3(uv.x, -1.0, -uv.y); }
        case 4u: { ray = vec3(uv.x, -uv.y, 1.0); }
        default: { ray = vec3(-uv.x, -uv.y, -1.0); }
    }
    return Ray(vec4(p, 0.0, 1.0), ray);
}
fn night_sky(ray: vec3<f32>) -> vec3<f32> {
    let glow = exp(-abs(ray.z) * 7.0) * environment.night.x;
    return vec3(0.003, 0.005, 0.017) + vec3(0.018, 0.013, 0.031) * glow;
}
@fragment fn fs(v: Ray) -> @location(0) vec4<f32> {
    let ray = v.ray / length(v.ray);
    let sun = environment.sun.xyz / max(length(environment.sun.xyz), 0.0001);
    let elevation = sun.z;
    let daylight = smoothstep(-0.14, 0.20, elevation);
    let twilight = smoothstep(-0.25, -0.02, elevation)
        * (1.0 - smoothstep(0.08, 0.42, elevation));
    let haze = clamp((environment.color_haze.w - 1.0) / 9.0, 0.0, 1.0);
    // Keep clear weather blue after exposure/compression. Haze builds gently
    // from the default setting, reserving the neutral cast for hazy weather.
    let haze_tint = haze * haze;
    let horizon = pow(clamp(ray.z, 0.0, 1.0), mix(0.35, 0.65, haze));
    let zenith = mix(vec3(0.012, 0.18, 1.1), vec3(0.27, 0.40, 0.51), haze_tint);
    let edge = mix(vec3(0.16, 0.52, 1.6), vec3(0.78, 0.69, 0.56), haze_tint);
    let day = mix(edge, zenith, horizon);
    let sunset = vec3(1.0, 0.28, 0.07) * twilight
        * exp(-abs(ray.z) * mix(8.0, 3.0, haze));
    let sun_angle = max(dot(ray, sun), 0.0);
    let disc = smoothstep(0.99993, 0.999995, sun_angle);
    let glow = pow(sun_angle, mix(55.0, 12.0, haze));
    let visible = smoothstep(-0.025, 0.02, elevation);
    var night = vec3(0.0);
    if daylight < 1.0 { night = night_sky(ray); }
    let color = mix(night, day, daylight) + sunset
        + environment.color_haze.rgb * (disc * 8.0 + glow * mix(0.35, 1.2, haze))
            * environment.sun.w * visible;
    return vec4(color, 1.0);
}
