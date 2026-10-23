@group(0) @binding(0) var scene_texture: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
struct CompositeOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn composite_vs(@builtin(vertex_index) vertex: u32) -> CompositeOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0,-1.0), vec2<f32>(1.0,-1.0), vec2<f32>(1.0,1.0),
        vec2<f32>(-1.0,-1.0), vec2<f32>(1.0,1.0), vec2<f32>(-1.0,1.0));
    var output: CompositeOutput;
    output.position = vec4<f32>(positions[vertex], 0.0, 1.0);
    output.uv = vec2<f32>(positions[vertex].x * 0.5 + 0.5,
        0.5 - positions[vertex].y * 0.5);
    return output;
}
@fragment fn composite_fs(input: CompositeOutput) -> @location(0) vec4<f32> {
    return textureSample(scene_texture, scene_sampler, input.uv);
}
@fragment fn composite_fs_linear(input: CompositeOutput) -> @location(0) vec4<f32> {
    let gamma = textureSample(scene_texture, scene_sampler, input.uv);
    let low = gamma.rgb / 12.92;
    let high = pow((gamma.rgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return vec4<f32>(select(low, high, gamma.rgb > vec3<f32>(0.04045)), gamma.a);
}
