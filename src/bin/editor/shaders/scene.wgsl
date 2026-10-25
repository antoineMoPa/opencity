struct Camera { view: vec4<f32>, focus: vec4<f32>, viewport: vec4<f32>, elevation: vec4<f32>, clip: array<vec4<f32>, 4> };
@group(0) @binding(2) var<uniform> camera: Camera;
struct VertexInput { @location(0) world: vec4<f32>, @location(1) color: vec4<f32>, @location(2) surface: vec4<f32> };
struct VertexOutput { @builtin(position) position: vec4<f32>, @location(0) color: vec4<f32>, @location(1) world: vec3<f32>, @location(2) @interpolate(flat) surface: vec4<f32> };
@vertex fn scene_vs(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.world = input.world.xyz;
    output.surface = input.surface;
    if camera.elevation.z > 0.5 {
        let point = vec4<f32>(input.world.xyz, 1.0);
        output.position = vec4<f32>(dot(camera.clip[0], point), dot(camera.clip[1], point),
            dot(camera.clip[2], point), dot(camera.clip[3], point));
        let perspective_scale = length(camera.clip[3].xyz);
        let overview_scale = length(camera.clip[0].xyz) * camera.viewport.x / (camera.viewport.y * 1.428);
        let perspective = clamp(perspective_scale / max(overview_scale, 0.000001), 0.0, 1.0);
        let layer_depth = (1.0 - perspective) * output.position.w / 2000.0
            + 0.04 * perspective_scale * perspective_scale
                / max(abs(output.position.w), max(0.04 * perspective_scale, 0.000001));
        output.position.z -= input.world.w * layer_depth;

        output.color = input.color;
        return output;
    }
    let point = input.world.xy;

    let height = input.world.z;
    let delta = point - camera.focus.xy;
    var offset = vec2<f32>(point.x, -point.y);
    var distance = -height * 8.0;
    if camera.view.w > 1.5 {
        let forward = delta.x * camera.focus.w + delta.y * camera.focus.z;
        let right = -delta.x * camera.focus.z + delta.y * camera.focus.w;
        let focal = camera.viewport.y * 0.714;
        let height_above_eye = height - camera.elevation.y;
        output.position = vec4<f32>(
            right * focal * 2.0 / camera.viewport.x,
            height_above_eye * focal * 2.0 / camera.viewport.y,
            // Infinite reversed-Z preserves precision far from the camera.
            // Painter layers change depth only, as in the orthographic views.
            0.04 * (1.0 - input.world.w / max(abs(forward), 0.04)),
            forward);
        output.color = input.color;
        return output;
    }
    if camera.view.w > 0.5 {
        offset = vec2<f32>(
            delta.x * camera.focus.w + delta.y * camera.focus.z,
            (delta.x * camera.focus.z - delta.y * camera.focus.w) * camera.viewport.z
                - height * camera.viewport.w);
        distance = -delta.x * camera.focus.z + delta.y * camera.focus.w
            - height * camera.elevation.x;
    }
    let screen = camera.view.xy + offset * camera.view.z;
    output.position = vec4<f32>(
        2.0 * screen.x / camera.viewport.x,
        -2.0 * screen.y / camera.viewport.y,
        clamp(0.5 - (distance + input.world.w) / 2000.0, 0.0, 1.0), 1.0);
    output.color = input.color;
    return output;
}
struct Water { bounds: vec4<f32>, level: vec4<f32> };
@group(1) @binding(0) var bed: texture_2d<f32>;
@group(1) @binding(1) var<uniform> water: Water;
fn bed_height(point: vec2<f32>) -> f32 {
    let extent = vec2<i32>(textureDimensions(bed));
    let pixel = clamp((point - water.bounds.xy) / water.bounds.zw, vec2<f32>(0.0), vec2<f32>(1.0)) * vec2<f32>(extent - vec2<i32>(1));
    let a = vec2<i32>(floor(pixel));
    let b = min(a + vec2<i32>(1), extent - vec2<i32>(1));
    let f = fract(pixel);
    return mix(mix(textureLoad(bed,a,0).r,textureLoad(bed,vec2<i32>(b.x,a.y),0).r,f.x),
        mix(textureLoad(bed,vec2<i32>(a.x,b.y),0).r,textureLoad(bed,b,0).r,f.x),f.y);
}
@fragment fn scene_fs(input: VertexOutput) -> @location(0) vec4<f32> {
    var color = input.color.rgb;
    // Derivatives are evaluated uniformly, including along water/roof edges.
    let floor_height = max(input.surface.z, 0.0001);
    let floor_position = (input.world.z - input.surface.y) / floor_height;
    let footprint = max(fwidth(floor_position), 0.0001);
    let edge_distance = min(fract(floor_position), 1.0-fract(floor_position));
    let band = (1.0 - smoothstep(0.035, 0.035 + footprint, edge_distance))
        * (1.0 - smoothstep(0.15, 0.65, footprint));
    let across = input.world.x + input.world.y;
    let panel = 0.5 + 0.5 * cos(across * 180.0);
    let brick_row = floor(floor_position * 7.0);
    let brick = 0.5 + 0.5*cos(across*340.0 + (brick_row % 2.0)*3.141593);
    if input.surface.x > 0.5 && input.surface.x < 1.5 {
        if input.surface.w < 1.5 { color *= 0.88 + 0.12*panel; }
        else if input.surface.w > 2.5 { color *= 0.92 + 0.08*brick; }
        color = mix(color, vec3<f32>(0.80392157,0.82352941,0.83921569), band);
    }
    if input.surface.x > 1.5 && input.surface.x < 2.5 {
        let depth = max(water.level.x - bed_height(input.world.xy),0.0);
        let shallow = 1.0 - smoothstep(0.0,0.10,depth);
        // Increasing phase moves each crest toward smaller depths: the shore.
        let wave = 0.5 + 0.5*cos(depth*420.0 + camera.elevation.w*1.570796327);
        color = mix(color, vec3<f32>(0.72,0.84,0.87), shallow*(0.08+0.045*wave));
    }
    return vec4<f32>(color,input.color.a);
}
