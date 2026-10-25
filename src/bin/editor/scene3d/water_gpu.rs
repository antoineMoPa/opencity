//! Immutable elevation upload and a valid fallback for every scene pipeline.
use super::*;

pub(super) struct WaterResources {
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

impl WaterResources {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("water elevation layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = Self::binding(device, queue, &layout, None);
        Self { layout, bind_group }
    }

    pub fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, scene: &Scene) {
        self.bind_group = Self::binding(device, queue, &self.layout, scene.water_map.as_deref());
    }

    fn binding(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        map: Option<&opencity::heightmap::ElevationMap>,
    ) -> wgpu::BindGroup {
        let width = if map.is_some() { 1024 } else { 1 };
        let mut values = Vec::with_capacity((width * width) as usize);
        if let Some(map) = map {
            for y in 0..1024 {
                for x in 0..1024 {
                    values.push(map.point(x, y)[2]);
                }
            }
        } else {
            values.push(0.0);
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("water bed elevation"),
            size: wgpu::Extent3d {
                width,
                height: width,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes_of(&values),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(width),
            },
            wgpu::Extent3d {
                width,
                height: width,
                depth_or_array_layers: 1,
            },
        );
        let settings = map.map_or([0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0], |m| {
            [
                m.spec.origin[0],
                m.spec.origin[1],
                m.spec.size[0],
                m.spec.size[1],
                m.spec.water_level.unwrap_or(0.0),
                0.0,
                0.0,
                0.0,
            ]
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("water bounds and level"),
            contents: bytes_of(&settings),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let view = texture.create_view(&Default::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water elevation binding"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
            ],
        })
    }
}
