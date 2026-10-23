struct Camera { view: vec4<f32>, focus: vec4<f32>, viewport: vec4<f32>, elevation: vec4<f32> };
@group(0) @binding(2) var<uniform> camera: Camera;
struct VertexInput { @location(0) world: vec4<f32>, @location(1) color: vec4<f32> };
struct VertexOutput { @builtin(position) position: vec4<f32>, @location(0) color: vec4<f32> };
@vertex fn scene_vs(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let point = input.world.xy;
    let height = input.world.z;
    let delta = point - camera.focus.xy;
    var offset = vec2<f32>(point.x, -point.y);
    var distance = -height * 8.0;
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
        clamp(0.5 + (distance + input.world.w) / 2000.0, 0.0, 1.0), 1.0);
    output.color = input.color;
    return output;
}
@fragment fn scene_fs(input: VertexOutput) -> @location(0) vec4<f32> { return input.color; }
