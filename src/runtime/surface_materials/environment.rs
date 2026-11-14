//! Shared cached atmosphere and GGX/cosine convolution for game and editor.
use wgpu::*;

pub struct EnvironmentMap {
    pub layout: BindGroupLayout,
    pub group: BindGroup,
    faces: Vec<TextureView>,
    filtered_faces: Vec<TextureView>,
    uniform: Buffer,
    generator_group: BindGroup,
    filter_group: BindGroup,
    generator: RenderPipeline,
    filter: RenderPipeline,
}
impl EnvironmentMap {
    pub fn new(device: &Device) -> Self {
        let source_entries = [
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::Cube,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ];
        let mut entries = source_entries.to_vec();
        entries.push(BindGroupLayoutEntry {
            binding: 2,
            ..source_entries[0]
        });
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("shared atmosphere and lighting"),
            entries: &entries,
        });
        let texture = |size, mips| {
            device.create_texture(&TextureDescriptor {
                label: Some("cached atmosphere cube"),
                size: Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 6,
                },
                mip_level_count: mips,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
        };
        let sky = texture(512, 1);
        let filtered = texture(128, 8);
        let cube = |texture: &Texture| {
            texture.create_view(&TextureViewDescriptor {
                dimension: Some(TextureViewDimension::Cube),
                ..Default::default()
            })
        };
        let sky_cube = cube(&sky);
        let filtered_cube = cube(&filtered);
        let face_views = |texture: &Texture, mips| {
            (0..mips)
                .flat_map(|mip| (0..6).map(move |face| (mip, face)))
                .map(|(mip, face)| {
                    texture.create_view(&TextureViewDescriptor {
                        dimension: Some(TextureViewDimension::D2),
                        base_mip_level: mip,
                        mip_level_count: Some(1),
                        base_array_layer: face,
                        array_layer_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect()
        };
        let faces = face_views(&sky, 1);
        let filtered_faces = face_views(&filtered, 8);
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("filtered atmosphere"),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: MipmapFilterMode::Linear,
            ..Default::default()
        });
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("shared atmosphere and lighting"),
            layout: &layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&sky_cube),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&sampler),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(&filtered_cube),
                },
            ],
        });
        let source_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("convolution source"),
            entries: &source_entries,
        });
        let filter_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("convolution source"),
            layout: &source_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&sky_cube),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });
        let uniform_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("atmosphere settings"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: BufferSize::new(48),
                },
                count: None,
            }],
        });
        let uniform = device.create_buffer(&BufferDescriptor {
            label: Some("atmosphere settings"),
            size: 48,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let generator_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("atmosphere settings"),
            layout: &uniform_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let pipeline = |source: &'static str, layout: &BindGroupLayout| {
            let shader = device.create_shader_module(ShaderModuleDescriptor {
                label: Some("cached environment shader"),
                source: ShaderSource::Wgsl(source.into()),
            });
            let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("cached environment"),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some("cached environment"),
                layout: Some(&layout),
                vertex: VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(ColorTargetState {
                        format: TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let generator = pipeline(include_str!("../sky_environment.wgsl"), &uniform_layout);
        let filter = pipeline(include_str!("../sky_filter.wgsl"), &source_layout);
        Self {
            layout,
            group,
            faces,
            filtered_faces,
            uniform,
            generator_group,
            filter_group,
            generator,
            filter,
        }
    }
    /// Call only when the atmosphere changes. Camera motion never requires a bake.
    /// Rows: sun XYZ/intensity, sun RGB/turbidity, horizon glow/reserved.
    pub fn bake(&self, queue: &Queue, encoder: &mut CommandEncoder, settings: [[f32; 4]; 3]) {
        let bytes: Vec<u8> = settings
            .into_iter()
            .flatten()
            .flat_map(f32::to_ne_bytes)
            .collect();
        queue.write_buffer(&self.uniform, 0, &bytes);
        for (targets, pipeline, group) in [
            (&self.faces, &self.generator, &self.generator_group),
            (&self.filtered_faces, &self.filter, &self.filter_group),
        ] {
            for (index, target) in targets.iter().enumerate() {
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("cached environment face"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Clear(Color::BLACK),
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, group, &[]);
                pass.draw(0..3, index as u32..index as u32 + 1);
            }
        }
    }
}
