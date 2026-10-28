//! GPU pipelines, render targets and camera callbacks.
use super::*;

pub struct SceneCallback {
    pub time_seconds: f32,
    pub transition: Option<camera_transition::ClipMatrix>,
    pub scene: Arc<Scene>,
    pub rect: egui::Rect,
    pub pan: egui::Vec2,
    pub zoom: f32,
    pub target: WorldPoint,
    pub angle: f32,
    pub elevation: f32,
    pub eye_height: f32,
    pub camera: CameraView,
    pub background: Color32,
    pub target_format: wgpu::TextureFormat,
}

struct RenderResources {
    water: water_gpu::WaterResources,
    scene_pipeline: wgpu::RenderPipeline,
    close_zoom_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    scene_buffer: Option<(Arc<Scene>, wgpu::Buffer)>,
    target: Option<RenderTarget>,
    composite_pipeline: wgpu::RenderPipeline,
    composite_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    target_format: wgpu::TextureFormat,
}
struct FrameResources {
    vertex_buffer: wgpu::Buffer,
    vertices: u32,
    color: wgpu::TextureView,
    multisampled_color: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}
struct RenderTarget {
    width: u32,
    height: u32,
    samples: u32,
    color: wgpu::TextureView,
    multisampled_color: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

const SCENE_SHADER: &str = include_str!("../shaders/scene.wgsl");
const COMPOSITE_SHADER: &str = include_str!("../shaders/composite.wgsl");

impl SceneCallback {
    pub(super) fn road_is_visible(&self, road: &RoadDrawRange) -> bool {
        if self.transition.is_some() || self.camera == CameraView::Vehicular {
            return true;
        }
        let mut min = egui::pos2(f32::INFINITY, f32::INFINITY);
        let mut max = egui::pos2(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for x in [road.min.x, road.max.x] {
            for y in [road.min.y, road.max.y] {
                for height in [road.min_height, road.max_height] {
                    let point = WorldPoint { x, y };
                    let screen = if self.camera == CameraView::Top {
                        screen_of(self.rect, self.pan, self.zoom, point)
                    } else {
                        iso_screen(
                            self.rect,
                            self.pan,
                            self.zoom,
                            self.target,
                            self.angle,
                            self.elevation,
                            point,
                            height,
                        )
                    };
                    min.x = min.x.min(screen.x);
                    min.y = min.y.min(screen.y);
                    max.x = max.x.max(screen.x);
                    max.y = max.y.max(screen.y);
                }
            }
        }
        egui::Rect::from_min_max(min, max).intersects(self.rect.expand(2.0))
    }
}

impl egui_wgpu::CallbackTrait for SceneCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
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
        let samples = if self.camera == CameraView::Vehicular || self.zoom <= MULTISAMPLE_MAX_ZOOM {
            SCENE_SAMPLES
        } else {
            1
        };
        if resources
            .get::<RenderResources>()
            .is_none_or(|r| r.target_format != self.target_format)
        {
            resources.insert(RenderResources::new(device, queue, self.target_format));
        }
        let renderer = resources.get_mut::<RenderResources>().unwrap();
        if renderer
            .scene_buffer
            .as_ref()
            .is_none_or(|(scene, _)| !Arc::ptr_eq(scene, &self.scene))
        {
            let vertices = self.scene.gpu_vertices();
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("opencity scene vertices"),
                contents: bytes_of(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            renderer.water.update(device, queue, &self.scene);
            renderer.scene_buffer = Some((Arc::clone(&self.scene), buffer));
        }
        let camera = CameraUniform {
            view: [
                self.pan.x,
                self.pan.y,
                self.zoom,
                match self.camera {
                    CameraView::Top => 0.0,
                    CameraView::Isometric => 1.0,
                    CameraView::Vehicular => 2.0,
                },
            ],
            target: [
                self.target.x,
                self.target.y,
                self.angle.sin(),
                self.angle.cos(),
            ],
            viewport: [
                self.rect.width(),
                self.rect.height(),
                self.elevation.sin(),
                self.elevation.cos(),
            ],
            elevation: [
                self.elevation.tan(),
                self.eye_height,
                f32::from(self.transition.is_some()),
                self.time_seconds,
            ],
            clip: self.transition.unwrap_or([[0.0; 4]; 4]),
        };
        queue.write_buffer(
            &renderer.camera_buffer,
            0,
            bytes_of(std::slice::from_ref(&camera)),
        );
        if renderer.target.as_ref().is_none_or(|target| {
            target.width != width || target.height != height || target.samples != samples
        }) {
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
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let multisampled_color = (samples > 1).then(|| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("opencity multisampled scene color"),
                        size,
                        mip_level_count: 1,
                        sample_count: samples,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            });
            let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("opencity scene depth"),
                size,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
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
            renderer.target = Some(RenderTarget {
                width,
                height,
                samples,
                color,
                multisampled_color,
                depth,
                bind_group,
            });
        }
        let target = renderer.target.as_ref().unwrap();
        {
            let rgba = self.background.to_array();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("opencity depth-tested scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.multisampled_color.as_ref().unwrap_or(&target.color),
                    resolve_target: target.multisampled_color.as_ref().map(|_| &target.color),
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
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(if samples == 1 {
                &renderer.close_zoom_pipeline
            } else {
                &renderer.scene_pipeline
            });
            pass.set_bind_group(0, &renderer.camera_bind_group, &[]);
            pass.set_bind_group(1, &renderer.water.bind_group, &[]);
            pass.set_vertex_buffer(0, renderer.scene_buffer.as_ref().unwrap().1.slice(..));
            if let Some(span) = &self.scene.road_span {
                if span.start > 0 {
                    pass.draw(0..span.start, 0..1);
                }
                if samples == SCENE_SAMPLES {
                    pass.draw(span.clone(), 0..1);
                } else {
                    let mut visible: Option<Range<u32>> = None;
                    for road in &self.scene.road_ranges {
                        if self.road_is_visible(road) {
                            if let Some(range) = &mut visible {
                                if range.end == road.vertices.start {
                                    range.end = road.vertices.end;
                                    continue;
                                }
                                pass.draw(range.clone(), 0..1);
                            }
                            visible = Some(road.vertices.clone());
                        }
                    }
                    if let Some(range) = visible {
                        pass.draw(range, 0..1);
                    }
                }
                if span.end < self.scene.vertices.len() as u32 {
                    pass.draw(span.end..self.scene.vertices.len() as u32, 0..1);
                }
            } else {
                pass.draw(0..self.scene.vertices.len() as u32, 0..1);
            }
        }
        let vertex_buffer = renderer.scene_buffer.as_ref().unwrap().1.clone();
        let color = target.color.clone();
        let multisampled_color = target.multisampled_color.clone();
        let depth = target.depth.clone();
        let bind_group = target.bind_group.clone();
        resources.insert(FrameResources {
            vertex_buffer,
            vertices: self.scene.vertices.len() as u32,
            color,
            multisampled_color,
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
            &frame.multisampled_color,
            &frame.depth,
            frame.vertices,
        );
        pass.set_pipeline(&renderer.composite_pipeline);
        pass.set_bind_group(0, &frame.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}

impl RenderResources {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, target_format: wgpu::TextureFormat) -> Self {
        let water = water_gpu::WaterResources::new(device, queue);
        let scene_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("opencity scene shader"),
            source: wgpu::ShaderSource::Wgsl(SCENE_SHADER.into()),
        });
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("opencity composite shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("opencity camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("opencity camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("opencity camera binding"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 2,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("opencity scene layout"),
            bind_group_layouts: &[Some(&camera_layout), Some(&water.layout)],
            immediate_size: 0,
        });
        let make_scene_pipeline = |samples| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("opencity scene pipeline"),
                layout: Some(&scene_layout),
                vertex: wgpu::VertexState {
                    module: &scene_shader,
                    entry_point: Some("scene_vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<GpuVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 =>
                             Float32x4,
                             1 =>
                             Float32x4,
                             2 =>
                             Float32x4],
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: samples,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &scene_shader,
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
            })
        };
        let scene_pipeline = make_scene_pipeline(SCENE_SAMPLES);
        let close_zoom_pipeline = make_scene_pipeline(1);
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
                module: &composite_shader,
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
                module: &composite_shader,
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
            // The resolved scene already matches the viewport's pixel size.
            // Linear sampling would soften it a second time during compositing.
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self {
            water,
            scene_pipeline,
            close_zoom_pipeline,
            camera_buffer,
            camera_bind_group,
            scene_buffer: None,
            target: None,
            composite_pipeline,
            composite_layout,
            sampler,
            target_format,
        }
    }
}

#[cfg(test)]
#[path = "gpu_tests.rs"]
mod gpu_tests;
