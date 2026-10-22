//! Shared world geometry for the top and isometric cameras. The detailed map
//! painters are tessellated in world space, then drawn with wgpu depth testing.
use super::*;
use eframe::{egui_wgpu, wgpu};
use std::sync::Arc;
use wgpu::util::DeviceExt;

const GEOMETRY_SCALE: f32 = 100.0;
const GEOMETRY_RECT: egui::Rect = egui::Rect::from_min_max(
    egui::pos2(-100_000.0, -100_000.0),
    egui::pos2(100_000.0, 100_000.0),
);

#[derive(Clone, Copy)]
pub struct WorldVertex {
    pub point: WorldPoint,
    pub height: f32,
    // Render-order offset independent of physical elevation. Grid lines sit
    // under terrain even when a land feature has a negative elevation.
    pub depth_bias: f32,
    pub color: Color32,
}

#[derive(Default)]
pub struct Scene {
    pub vertices: Vec<WorldVertex>,
}

fn capture_geometry(
    ctx: &egui::Context,
    paint: impl FnOnce(&egui::Painter),
    height: impl Fn(WorldPoint) -> f32,
    scene: &mut Scene,
) {
    ctx.begin_pass(egui::RawInput {
        screen_rect: Some(GEOMETRY_RECT),
        ..Default::default()
    });
    let painter = ctx
        .layer_painter(egui::LayerId::background())
        .with_clip_rect(GEOMETRY_RECT);
    paint(&painter);
    let mut output = ctx.end_pass();
    output.textures_delta.clear();
    // A painter shape is one visual layer. Give later shapes a small depth
    // priority without moving their geometry: raising each tessellated
    // triangle made adjacent road facets visible as separate rectangles.
    for (layer, shape) in output.shapes.into_iter().enumerate() {
        for clipped in ctx.tessellate(vec![shape], 1.0) {
            let egui::epaint::Primitive::Mesh(mesh) = clipped.primitive else {
                continue;
            };
            if mesh
                .vertices
                .iter()
                .any(|vertex| vertex.uv != egui::epaint::WHITE_UV)
            {
                continue;
            }
            for triangle in mesh.indices.chunks_exact(3) {
                for index in triangle {
                    let vertex = &mesh.vertices[*index as usize];
                    let world =
                        world_of(GEOMETRY_RECT, egui::Vec2::ZERO, GEOMETRY_SCALE, vertex.pos);
                    scene.vertices.push(WorldVertex {
                        point: world,
                        height: height(world),
                        depth_bias: -(layer as f32) * 0.0003,
                        color: vertex.color,
                    });
                }
            }
        }
    }
}

fn road_height(samples: &[WorldPoint; 65], heights: &[f32; 65], point: WorldPoint) -> f32 {
    let mut best = (f32::INFINITY, 0.0);
    for step in 0..64 {
        let a = samples[step];
        let b = samples[step + 1];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let along = (((point.x - a.x) * dx + (point.y - a.y) * dy)
            / (dx * dx + dy * dy).max(0.000001))
        .clamp(0.0, 1.0);
        let x = a.x + along * dx;
        let y = a.y + along * dy;
        let distance = (x - point.x).powi(2) + (y - point.y).powi(2);
        if distance < best.0 {
            best = (
                distance,
                heights[step] + (heights[step + 1] - heights[step]) * along,
            );
        }
    }
    best.1
}

fn add_elevated_edges(
    scene: &mut Scene,
    segment: &map::Segment,
    road: Option<&RoadDefinition>,
    profile: &PaintProfile,
    color: Color32,
) {
    for side in [0, 1] {
        let mut previous = None;
        for step in 0..=64 {
            let t = step as f32 / 64.0;
            let center = segment.point_at(t);
            let tangent = segment.tangent_at(t);
            let length = (tangent.x * tangent.x + tangent.y * tangent.y)
                .sqrt()
                .max(0.000001);
            let (left, right) = road
                .map(|road| section_edges(road, profile, t))
                .unwrap_or((-0.08, 0.08));
            let offset = if side == 0 { left } else { right };
            let point = WorldPoint {
                x: center.x - tangent.y / length * offset,
                y: center.y + tangent.x / length * offset,
            };
            let height = segment.height_at(t);
            if let Some((prior_point, prior_height)) = previous {
                if height.max(prior_height) > 0.05 {
                    let current = WorldVertex {
                        point,
                        height: height + 0.003,
                        depth_bias: 0.0,
                        color,
                    };
                    let prior = WorldVertex {
                        point: prior_point,
                        height: prior_height + 0.003,
                        depth_bias: 0.0,
                        color,
                    };
                    let current_bottom = WorldVertex {
                        height: height - 0.08,
                        ..current
                    };
                    let prior_bottom = WorldVertex {
                        height: prior_height - 0.08,
                        ..prior
                    };
                    scene.vertices.extend_from_slice(&[
                        prior,
                        current,
                        current_bottom,
                        prior,
                        current_bottom,
                        prior_bottom,
                    ]);
                }
            }
            previous = Some((point, height));
        }
    }
}

pub fn build_scene(
    program: &Program,
    projection: &map::Projection,
    palette: EditorPalette,
    selected: &[SegmentSource],
    selected_landscape: Option<&str>,
) -> Scene {
    let mut scene = Scene::default();
    let ctx = egui::Context::default();
    let profiles = paint_profiles(program, projection);
    let grid_rect = egui::Rect::from_center_size(egui::Pos2::ZERO, egui::vec2(20_000.0, 20_000.0));
    capture_geometry(
        &ctx,
        |painter| {
            paint_grid(
                painter,
                grid_rect,
                egui::Vec2::ZERO,
                GEOMETRY_SCALE,
                palette,
            );
        },
        |_| 0.0,
        &mut scene,
    );
    for vertex in &mut scene.vertices {
        vertex.depth_bias = 100.0;
    }
    let mut features = project_landscape_features(program).features;
    features.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
    for feature in &features {
        capture_geometry(
            &ctx,
            |painter| {
                paint_landscape_feature(
                    painter,
                    GEOMETRY_RECT,
                    egui::Vec2::ZERO,
                    GEOMETRY_SCALE,
                    feature,
                    selected_landscape == feature.name.as_deref(),
                    palette,
                );
            },
            |_| feature.elevation - 0.02,
            &mut scene,
        );
    }
    for index in road_paint_order(program, projection, &profiles) {
        let segment = &projection.segments[index];
        let road = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == segment.road_name => Some(road),
                _ => None,
            });
        let is_selected = selected.contains(&segment.source);
        let samples = std::array::from_fn(|step| segment.point_at(step as f32 / 64.0));
        let heights = std::array::from_fn(|step| segment.height_at(step as f32 / 64.0));
        capture_geometry(
            &ctx,
            |painter| {
                if let Some(road) = road.filter(|_| {
                    profiles[index].start_alignment.is_some()
                        || profiles[index].merge_alignment.is_some()
                        || !profiles[index].start_taper.is_empty()
                        || !profiles[index].end_taper.is_empty()
                }) {
                    paint_profiled_road(
                        painter,
                        GEOMETRY_RECT,
                        egui::Vec2::ZERO,
                        GEOMETRY_SCALE,
                        segment,
                        road,
                        is_selected,
                        palette,
                        &profiles[index],
                    );
                } else {
                    paint_road_segment(
                        painter,
                        GEOMETRY_RECT,
                        egui::Vec2::ZERO,
                        GEOMETRY_SCALE,
                        segment,
                        road,
                        is_selected,
                        palette,
                    );
                }
            },
            |point| road_height(&samples, &heights, point) + 0.005,
            &mut scene,
        );
        add_elevated_edges(
            &mut scene,
            segment,
            road,
            &profiles[index],
            palette.map_outline.gamma_multiply(0.75),
        );
    }
    for intersection in map::intersections(program, projection) {
        capture_geometry(
            &ctx,
            |painter| {
                paint_intersection(
                    painter,
                    GEOMETRY_RECT,
                    egui::Vec2::ZERO,
                    GEOMETRY_SCALE,
                    program,
                    projection,
                    &intersection,
                    palette,
                );
            },
            |_| 0.02,
            &mut scene,
        );
    }
    scene
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GpuVertex {
    position: [f32; 3],
    color: [f32; 4],
}

fn bytes_of<T>(values: &[T]) -> &[u8] {
    // GpuVertex contains only initialized f32 fields; the buffers are consumed
    // immediately by wgpu and never interpreted as Rust objects again.
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

pub struct SceneCallback {
    pub scene: Arc<Scene>,
    pub rect: egui::Rect,
    pub pan: egui::Vec2,
    pub zoom: f32,
    pub target: WorldPoint,
    pub angle: f32,
    pub camera: CameraView,
    pub background: Color32,
    pub target_format: wgpu::TextureFormat,
}

struct RenderResources {
    scene_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    composite_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    target_format: wgpu::TextureFormat,
}
struct FrameResources {
    vertex_buffer: wgpu::Buffer,
    vertices: u32,
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

const SHADER: &str = r#"
struct VertexInput { @location(0) position: vec3<f32>, @location(1) color: vec4<f32> };
struct VertexOutput { @builtin(position) position: vec4<f32>, @location(0) color: vec4<f32> };
@vertex fn scene_vs(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 1.0);
    output.color = input.color;
    return output;
}
@fragment fn scene_fs(input: VertexOutput) -> @location(0) vec4<f32> { return input.color; }
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
"#;

impl SceneCallback {
    fn projected(&self, point: WorldPoint, height: f32, depth_bias: f32) -> [f32; 3] {
        let screen = match self.camera {
            CameraView::Top => screen_of(self.rect, self.pan, self.zoom, point),
            CameraView::Isometric => iso_screen(
                self.rect,
                self.pan,
                self.zoom,
                self.target,
                self.angle,
                point,
                height,
            ),
        };
        let x = 2.0 * (screen.x - self.rect.min.x) / self.rect.width() - 1.0;
        let y = 1.0 - 2.0 * (screen.y - self.rect.min.y) / self.rect.height();
        let distance = match self.camera {
            CameraView::Top => -height * 8.0,
            CameraView::Isometric => {
                let dx = point.x - self.target.x;
                let dy = point.y - self.target.y;
                -dx * self.angle.sin() + dy * self.angle.cos() - height * ISO_ELEVATION.tan()
            }
        };
        [
            x,
            y,
            (0.5 + (distance + depth_bias) / 2000.0).clamp(0.0, 1.0),
        ]
    }
}

impl egui_wgpu::CallbackTrait for SceneCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let width = (self.rect.width() * screen.pixels_per_point)
            .round()
            .max(1.0) as u32;
        let height = (self.rect.height() * screen.pixels_per_point)
            .round()
            .max(1.0) as u32;
        if resources
            .get::<RenderResources>()
            .is_none_or(|r| r.target_format != self.target_format)
        {
            resources.insert(RenderResources::new(device, self.target_format));
        }
        let renderer = resources.get::<RenderResources>().unwrap();
        let vertices: Vec<_> = self
            .scene
            .vertices
            .iter()
            .map(|vertex| {
                let rgba = vertex.color.to_array();
                GpuVertex {
                    position: self.projected(vertex.point, vertex.height, vertex.depth_bias),
                    color: rgba.map(|channel| channel as f32 / 255.0),
                }
            })
            .collect();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("opencity scene vertices"),
            contents: bytes_of(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("opencity scene color"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("opencity scene depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color = color_texture.create_view(&Default::default());
        let depth = depth_texture.create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("opencity scene image"),
            layout: &renderer.composite_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&renderer.sampler),
                },
            ],
        });
        {
            let rgba = self.background.to_array();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("opencity depth-tested scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: rgba[0] as f64 / 255.0,
                            g: rgba[1] as f64 / 255.0,
                            b: rgba[2] as f64 / 255.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&renderer.scene_pipeline);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        resources.insert(FrameResources {
            vertex_buffer,
            vertices: vertices.len() as u32,
            color,
            depth,
            bind_group,
        });
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::epaint::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let renderer = resources.get::<RenderResources>().unwrap();
        let frame = resources.get::<FrameResources>().unwrap();
        let _keep_alive = (
            &frame.vertex_buffer,
            &frame.color,
            &frame.depth,
            frame.vertices,
        );
        pass.set_pipeline(&renderer.composite_pipeline);
        pass.set_bind_group(0, &frame.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}

impl RenderResources {
    fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("opencity scene shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("opencity scene layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let scene_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("opencity scene pipeline"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("scene_vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("scene_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("opencity composite image layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let composite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("opencity composite layout"),
                bind_group_layouts: &[Some(&composite_layout)],
                immediate_size: 0,
            });
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("opencity composite pipeline"),
            layout: Some(&composite_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("composite_vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(if target_format.is_srgb() {
                    "composite_fs_linear"
                } else {
                    "composite_fs"
                }),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("opencity scene sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            scene_pipeline,
            composite_pipeline,
            composite_layout,
            sampler,
            target_format,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_scene_contains_detailed_starter_roads_landscape_and_junctions() {
        let program = starter_program();
        let projection = map::project(&program);
        let palette = editor_palette(egui::Theme::Dark);
        let scene = build_scene(&program, &projection, palette, &[], None);
        assert!(
            scene.vertices.len() > 100_000,
            "{} scene vertices",
            scene.vertices.len()
        );
        assert!(scene
            .vertices
            .iter()
            .any(|vertex| vertex.color == kind_color(RoadElementKind::Sidewalk, palette.theme)));
        assert!(scene.vertices.iter().any(|vertex| vertex.height > 0.1));
        assert!(scene.vertices.iter().any(|vertex| vertex.height < -0.1));
        for intersection in map::intersections(&program, &projection) {
            let half = 0.5;
            assert!(
                scene.vertices.iter().any(|vertex| {
                    (vertex.point.x - intersection.position.x).abs() < half
                        && (vertex.point.y - intersection.position.y).abs() < half
                        && vertex.height > 0.015
                        && vertex.height < 0.04
                }),
                "missing intersection {}",
                intersection.point_ref
            );
        }
    }
}

#[cfg(test)]
mod gpu_tests {
    use super::*;
    use egui_wgpu::CallbackTrait;
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    #[test]
    fn wgpu_scene_pipelines_validate_when_an_adapter_is_available() {
        let instance = wgpu::Instance::default();
        let Ok(adapter) =
            block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            eprintln!("Skipping GPU readback: no wgpu adapter is available");
            return;
        };
        let Ok((device, queue)) =
            block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        else {
            eprintln!("Skipping GPU readback: no wgpu device is available");
            return;
        };
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _resources = RenderResources::new(&device, wgpu::TextureFormat::Bgra8Unorm);
        let _srgb_resources = RenderResources::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut resources = egui_wgpu::CallbackResources::default();
        let callback = SceneCallback {
            scene: Arc::new(Scene {
                vertices: vec![
                    WorldVertex {
                        point: WorldPoint { x: -1.0, y: -1.0 },
                        height: 0.0,
                        depth_bias: 0.0,
                        color: Color32::RED,
                    },
                    WorldVertex {
                        point: WorldPoint { x: 1.0, y: -1.0 },
                        height: 0.0,
                        depth_bias: 0.0,
                        color: Color32::RED,
                    },
                    WorldVertex {
                        point: WorldPoint { x: 0.0, y: 1.0 },
                        height: 0.0,
                        depth_bias: 0.0,
                        color: Color32::RED,
                    },
                ],
            }),
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(128.0, 128.0)),
            pan: egui::Vec2::ZERO,
            zoom: 40.0,
            target: WorldPoint { x: 0.0, y: 0.0 },
            angle: 0.0,
            camera: CameraView::Top,
            background: Color32::BLACK,
            target_format: wgpu::TextureFormat::Bgra8Unorm,
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        callback.prepare(
            &device,
            &queue,
            &egui_wgpu::ScreenDescriptor {
                size_in_pixels: [128, 128],
                pixels_per_point: 1.0,
            },
            &mut encoder,
            &mut resources,
        );
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene test output"),
            size: wgpu::Extent3d {
                width: 128,
                height: 128,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = output.create_view(&Default::default());
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene test composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut pass = pass.forget_lifetime();
            pass.set_viewport(0.0, 0.0, 128.0, 128.0, 0.0, 1.0);
            callback.paint(
                egui::epaint::PaintCallbackInfo {
                    viewport: callback.rect,
                    clip_rect: callback.rect,
                    pixels_per_point: 1.0,
                    screen_size_px: [128, 128],
                },
                &mut pass,
                &resources,
            );
        }
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene test readback"),
            size: 128 * 128 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &output,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(128 * 4),
                    rows_per_image: Some(128),
                },
            },
            wgpu::Extent3d {
                width: 128,
                height: 128,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            sender.send(result).unwrap();
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receiver.recv().unwrap().unwrap();
        let pixels = readback.get_mapped_range(..).unwrap();
        let center = &pixels[(64 * 128 + 64) * 4..(64 * 128 + 64) * 4 + 4];
        let corner = &pixels[0..4];
        assert!(
            center[2] > 180 && center[1] < 80 && center[0] < 80,
            "center BGRA should be red: {center:?}"
        );
        assert!(
            corner[0] < 10 && corner[1] < 10 && corner[2] < 10,
            "corner BGRA should be black: {corner:?}"
        );
        drop(pixels);
        readback.unmap();
        assert!(block_on(scope.pop()).is_none());
    }

    #[test]
    #[ignore = "manual offscreen visual QA"]
    fn render_starter_scene_pngs() {
        const WIDTH: u32 = 1024;
        const HEIGHT: u32 = 768;
        let instance = wgpu::Instance::default();
        let adapter =
            block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
        let (device, queue) =
            block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let program = starter_program();
        let projection = map::project(&program);
        let palette = editor_palette(egui::Theme::Dark);
        let scene = Arc::new(build_scene(&program, &projection, palette, &[], None));
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        );
        for segment in &projection.segments {
            for point in [
                segment.start,
                segment.controls[0],
                segment.controls[1],
                segment.end,
            ] {
                min_x = min_x.min(point.x);
                min_y = min_y.min(point.y);
                max_x = max_x.max(point.x);
                max_y = max_y.max(point.y);
            }
        }
        let center = WorldPoint {
            x: (min_x + max_x) * 0.5,
            y: (min_y + max_y) * 0.5,
        };
        let fit = (WIDTH as f32 / (max_x - min_x + 5.0)).min(HEIGHT as f32 / (max_y - min_y + 5.0));
        eprintln!("starter bounds: ({min_x},{min_y}) to ({max_x},{max_y}), fit zoom {fit}");
        let junction = map::intersections(&program, &projection)
            .into_iter()
            .find(|item| item.point_ref == "grid_1_1")
            .map(|item| item.position)
            .unwrap_or(center);
        let interchange = WorldPoint { x: 4.7, y: -3.0 };
        for (name, camera, target, zoom) in [
            ("top-full", CameraView::Top, center, fit),
            ("iso-full", CameraView::Isometric, center, fit * 0.72),
            ("top-intersection", CameraView::Top, junction, 125.0),
            ("iso-intersection", CameraView::Isometric, junction, 125.0),
            ("top-interchange", CameraView::Top, interchange, 115.0),
            ("iso-interchange", CameraView::Isometric, interchange, 115.0),
        ] {
            let rect = egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(WIDTH as f32, HEIGHT as f32),
            );
            let pan = if camera == CameraView::Top {
                egui::vec2(-target.x * zoom, target.y * zoom)
            } else {
                egui::Vec2::ZERO
            };
            let callback = SceneCallback {
                scene: Arc::clone(&scene),
                rect,
                pan,
                zoom,
                target,
                angle: 0.74,
                camera,
                background: palette.map_background,
                target_format: wgpu::TextureFormat::Bgra8Unorm,
            };
            let mut resources = egui_wgpu::CallbackResources::default();
            let mut encoder = device.create_command_encoder(&Default::default());
            callback.prepare(
                &device,
                &queue,
                &egui_wgpu::ScreenDescriptor {
                    size_in_pixels: [WIDTH, HEIGHT],
                    pixels_per_point: 1.0,
                },
                &mut encoder,
                &mut resources,
            );
            let output = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("starter QA output"),
                size: wgpu::Extent3d {
                    width: WIDTH,
                    height: HEIGHT,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = output.create_view(&Default::default());
            {
                let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("starter QA composite"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                let mut pass = pass.forget_lifetime();
                pass.set_viewport(0.0, 0.0, WIDTH as f32, HEIGHT as f32, 0.0, 1.0);
                callback.paint(
                    egui::epaint::PaintCallbackInfo {
                        viewport: rect,
                        clip_rect: rect,
                        pixels_per_point: 1.0,
                        screen_size_px: [WIDTH, HEIGHT],
                    },
                    &mut pass,
                    &resources,
                );
            }
            let size = (WIDTH * HEIGHT * 4) as u64;
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("starter QA readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &output,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(WIDTH * 4),
                        rows_per_image: Some(HEIGHT),
                    },
                },
                wgpu::Extent3d {
                    width: WIDTH,
                    height: HEIGHT,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit([encoder.finish()]);
            let (sender, receiver) = std::sync::mpsc::channel();
            readback.map_async(wgpu::MapMode::Read, .., move |result| {
                sender.send(result).unwrap();
            });
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            receiver.recv().unwrap().unwrap();
            let mapped = readback.get_mapped_range(..).unwrap();
            let mut rgba = mapped.to_vec();
            for pixel in rgba.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            drop(mapped);
            readback.unmap();
            let path = format!("/private/tmp/opencity-{name}-qa.png");
            let file = std::fs::File::create(&path).unwrap();
            let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&rgba)
                .unwrap();
            eprintln!("wrote {path}");
        }
    }
}
