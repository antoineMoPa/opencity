//! Shared baked bark and foliage atlas layers for game and editor.
pub struct TreeTextures {
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
}
fn art() -> [&'static [u8]; 6] {
    [
        crate::game_asset!(
            "../../assets/props/trees/leaves.png",
            "opencity/assets/props/trees/leaves.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/grass.png",
            "opencity/assets/props/trees/grass.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/leaves.png",
            "opencity/assets/props/trees/leaves.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/bark.png",
            "opencity/assets/props/trees/bark.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/bark.png",
            "opencity/assets/props/trees/bark.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/bark-normal.png",
            "opencity/assets/props/trees/bark-normal.png"
        ),
    ]
}
impl TreeTextures {
    pub fn layout(first: u32) -> [wgpu::BindGroupLayoutEntry; 2] {
        [
            wgpu::BindGroupLayoutEntry {
                binding: first,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: first + 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ]
    }
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("street tree bark and leaf atlases"),
            size: wgpu::Extent3d {
                width: 512,
                height: 512,
                // wgpu's GL backend infers cube targets for square arrays with
                // a multiple of six layers. Pad these 2D arrays on WebGL.
                depth_or_array_layers: if device.limits().max_storage_buffers_per_shader_stage == 0
                {
                    7
                } else {
                    6
                },
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (layer, png) in art().iter().enumerate() {
            let mut decoder = png::Decoder::new(std::io::Cursor::new(png))
                .read_info()
                .expect("bundled tree PNG");
            let mut pixels = vec![0; decoder.output_buffer_size()];
            let info = decoder.next_frame(&mut pixels).expect("bundled tree RGBA");
            assert_eq!(info.color_type, png::ColorType::Rgba);
            let mut scaled = vec![0; 512 * 512 * 4];
            for y in 0..512usize {
                for x in 0..512usize {
                    let from = ((y * info.height as usize / 512) * info.width as usize
                        + x * info.width as usize / 512)
                        * 4;
                    scaled[(y * 512 + x) * 4..(y * 512 + x + 1) * 4]
                        .copy_from_slice(&pixels[from..from + 4]);
                }
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &scaled,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(512 * 4),
                    rows_per_image: Some(512),
                },
                wgpu::Extent3d {
                    width: 512,
                    height: 512,
                    depth_or_array_layers: 1,
                },
            );
        }
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            }),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                ..Default::default()
            }),
        }
    }
    pub fn entries(&self, first: u32) -> [wgpu::BindGroupEntry<'_>; 2] {
        [
            wgpu::BindGroupEntry {
                binding: first,
                resource: wgpu::BindingResource::TextureView(&self.view),
            },
            wgpu::BindGroupEntry {
                binding: first + 1,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            },
        ]
    }
}

pub struct TreeImpostors {
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
}
fn images() -> [&'static [u8]; 12] {
    [
        crate::game_asset!(
            "../../assets/props/trees/broad-summer.png",
            "opencity/assets/props/trees/broad-summer.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/broad-summer.png",
            "opencity/assets/props/trees/broad-summer.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/upright-summer.png",
            "opencity/assets/props/trees/upright-summer.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/upright-summer.png",
            "opencity/assets/props/trees/upright-summer.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/young-summer.png",
            "opencity/assets/props/trees/young-summer.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/young-summer.png",
            "opencity/assets/props/trees/young-summer.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/broad-winter.png",
            "opencity/assets/props/trees/broad-winter.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/broad-winter.png",
            "opencity/assets/props/trees/broad-winter.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/upright-winter.png",
            "opencity/assets/props/trees/upright-winter.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/upright-winter.png",
            "opencity/assets/props/trees/upright-winter.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/young-winter.png",
            "opencity/assets/props/trees/young-winter.png"
        ),
        crate::game_asset!(
            "../../assets/props/trees/young-winter.png",
            "opencity/assets/props/trees/young-winter.png"
        ),
    ]
}
impl TreeImpostors {
    pub fn layout(first: u32) -> [wgpu::BindGroupLayoutEntry; 2] {
        TreeTextures::layout(first)
    }
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("street tree single-plane impostors"),
            size: wgpu::Extent3d {
                width: 1024,
                height: 1024,
                depth_or_array_layers: if device.limits().max_storage_buffers_per_shader_stage == 0
                {
                    13
                } else {
                    12
                },
            },
            mip_level_count: 6,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (layer, png) in images().iter().enumerate() {
            let mut decoder = png::Decoder::new(std::io::Cursor::new(png))
                .read_info()
                .expect("bundled tree impostor PNG");
            let mut pixels = vec![0; decoder.output_buffer_size()];
            let info = decoder
                .next_frame(&mut pixels)
                .expect("bundled tree impostor RGBA");
            assert_eq!(
                (info.width, info.height, info.color_type),
                (1024, 1024, png::ColorType::Rgba)
            );
            let mut level_pixels = pixels[..info.buffer_size()].to_vec();
            for level in 0..6u32 {
                let width = 1024 >> level;
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: layer as u32,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &level_pixels,
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
                if level == 5 {
                    break;
                }
                let next_width = width as usize / 2;
                let mut next = vec![0u8; next_width * next_width * 4];
                for y in 0..next_width {
                    for x in 0..next_width {
                        let mut alpha = 0u32;
                        let mut weighted = [0u32; 3];
                        for dy in 0..2 {
                            for dx in 0..2 {
                                let source = ((y * 2 + dy) * width as usize + x * 2 + dx) * 4;
                                let coverage = level_pixels[source + 3] as u32;
                                alpha += coverage;
                                for color in 0..3 {
                                    weighted[color] +=
                                        level_pixels[source + color] as u32 * coverage;
                                }
                            }
                        }
                        let target = (y * next_width + x) * 4;
                        for color in 0..3 {
                            next[target + color] = if alpha > 0 {
                                (weighted[color] / alpha) as u8
                            } else {
                                0
                            };
                        }
                        next[target + 3] = (alpha / 4) as u8;
                    }
                }
                level_pixels = next;
            }
        }
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            }),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            }),
        }
    }
    pub fn entries(&self, first: u32) -> [wgpu::BindGroupEntry<'_>; 2] {
        [
            wgpu::BindGroupEntry {
                binding: first,
                resource: wgpu::BindingResource::TextureView(&self.view),
            },
            wgpu::BindGroupEntry {
                binding: first + 1,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            },
        ]
    }
}
