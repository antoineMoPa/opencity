// Shared native-resolution stellar detail for sky backgrounds and reflected rays.
// No derivatives or discard: safe from material-dependent fragment control flow.
fn sky_star_hash(cell: vec2<f32>) -> f32 {
    let q = bitcast<vec2<u32>>(vec2<i32>(cell));
    var h = q.x * 374761393u + q.y * 668265263u;
    h = (h ^ (h >> 13u)) * 1274126177u;
    return f32(h ^ (h >> 16u)) / 4294967295.0;
}
// params: density, angular size in pixels, radiance, angular pixel footprint (radians).
// color: linear RGB, solar visibility. The pattern stays fixed in world space.
fn sky_stars(ray: vec3<f32>, params: vec4<f32>, color: vec4<f32>) -> vec3<f32> {
    if color.w <= 0.0 || params.x <= 0.0 || params.z <= 0.0 || ray.z <= 0.0 {
        return vec3(0.0);
    }
    let direction = ray / max(length(ray), 0.00001);
    let coordinates = vec2(atan2(direction.y, direction.x) * 57.2957795 + 180.0,
        asin(clamp(direction.z, -1.0, 1.0)) * 57.2957795 + 90.0);
    let cell = floor(coordinates);
    if sky_star_hash(cell) <= 1.0 - params.x { return vec3(0.0); }
    let offset = vec2(sky_star_hash(cell + vec2(31.0, 73.0)),
        sky_star_hash(cell + vec2(97.0, 17.0)));
    let center = mix(vec2(0.4), vec2(0.6), offset);
    let delta = (fract(coordinates) - center) * 0.0174532925;
    let longitude_scale = sqrt(max(1.0 - direction.z * direction.z, 0.0001));
    let distance = length(delta * vec2(longitude_scale, 1.0));
    let pixel = max(params.w, 0.00001);
    let radius = pixel * 0.55 * params.y * mix(0.85, 1.15, offset.y);
    // Analytic coverage gives a crisp ~1–2 native-pixel core at default size.
    // No low-resolution texture filtering and no broad halo.
    let coverage = 1.0 - smoothstep(radius - pixel * 0.5, radius + pixel * 0.5, distance);
    let tint = mix(vec3(1.0, 0.78, 0.65), vec3(0.72, 0.85, 1.0), offset.x);
    return tint * color.rgb * coverage * params.z * mix(0.65, 1.25, offset.y)
        * color.w * smoothstep(0.0, 0.15, direction.z);
}
