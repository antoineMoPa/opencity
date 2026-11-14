use super::*;
pub struct SurfaceTextures {
    color: wgpu::TextureView,
    detail: wgpu::TextureView,
    sampler: wgpu::Sampler,
}
impl SurfaceTextures {
    pub fn layout(first: u32) -> [wgpu::BindGroupLayoutEntry; 3] {
        std::array::from_fn(|i| wgpu::BindGroupLayoutEntry {
            binding: first + i as u32,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: if i == 2 {
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)
            } else {
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                }
            },
            count: None,
        })
    }
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let upload = |channel: usize, format| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("scanned pavement mip array"),
                size: wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: 2,
                },
                mip_level_count: 11,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            for (layer, tile) in tiles().iter().enumerate() {
                for (level, mip) in tile[channel].iter().enumerate() {
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            mip_level: level as u32,
                            origin: wgpu::Origin3d {
                                x: 0,
                                y: 0,
                                z: layer as u32,
                            },
                            ..texture.as_image_copy()
                        },
                        &mip.rgba,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(mip.size * 4),
                            rows_per_image: Some(mip.size),
                        },
                        wgpu::Extent3d {
                            width: mip.size,
                            height: mip.size,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        };
        Self {
            color: upload(0, wgpu::TextureFormat::Rgba8UnormSrgb),
            detail: upload(1, wgpu::TextureFormat::Rgba8Unorm),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("pavement repeat anisotropic sampler"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                anisotropy_clamp: 4,
                ..Default::default()
            }),
        }
    }
    pub fn entries(&self, first: u32) -> [wgpu::BindGroupEntry<'_>; 3] {
        [
            wgpu::BindGroupEntry {
                binding: first,
                resource: wgpu::BindingResource::TextureView(&self.color),
            },
            wgpu::BindGroupEntry {
                binding: first + 1,
                resource: wgpu::BindingResource::TextureView(&self.detail),
            },
            wgpu::BindGroupEntry {
                binding: first + 2,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            },
        ]
    }
}
