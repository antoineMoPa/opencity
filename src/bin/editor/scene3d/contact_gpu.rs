//! Host bindings for OpenCity's shared, once-baked contact masks.
use super::*;
use opencity::runtime::contact_debris;

pub(super) fn prepare_vertices(
    mut vertices: Vec<GpuVertex>,
    protected_end: usize,
) -> Vec<GpuVertex> {
    use opencity::runtime::{Mesh, Vertex};
    let tagged = |v: &GpuVertex| (7.0..=9.0).contains(&v.surface[0]);
    let to_runtime = |v: &GpuVertex| {
        let mut surface = v.surface;
        surface[0] = match v.surface[0] {
            7. => 0.,
            8. => 2.,
            _ => 7.,
        };
        // Authoring pavement stays coplanar. Use the runtime lift only while baking contacts.
        let height = v.world[2] + if surface[0] == 2. { 0.0015 } else { 0. };
        Vertex {
            position: [v.world[0], v.world[1], height],
            normal: [v.world[3], v.color[3], 1.],
            color: [v.color[0], v.color[1], v.color[2]],
            surface,
        }
    };
    // Ordinary road ranges are already baked and used for culling. Their vertices
    // must retain both order and count. Junctions follow that protected prefix.
    let mut suffix = vertices.split_off(protected_end);
    let mut mesh = Mesh {
        vertices: vertices
            .iter()
            .filter(|v| tagged(v))
            .map(to_runtime)
            .collect(),
    };
    let prefix_contacts = mesh.vertices.len();
    suffix.retain(|v| {
        if tagged(v) {
            mesh.vertices.push(to_runtime(v));
            false
        } else {
            true
        }
    });
    contact_debris::prepare_mesh(&mut mesh);
    vertices.append(&mut suffix);
    vertices.extend(mesh.vertices.into_iter().skip(prefix_contacts).map(|v| {
        let mut surface = v.surface;
        surface[0] = match surface[0] {
            0. => 7.,
            2. => 8.,
            _ => 9.,
        };
        let height = v.position[2] - if surface[0] == 8. { 0.0015 } else { 0. };
        GpuVertex {
            world: [v.position[0], v.position[1], height, v.normal[0]],
            color: [v.color[0], v.color[1], v.color[2], v.normal[1]],
            surface,
        }
    }));
    vertices
}

pub(super) struct ContactTexture {
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub settings: wgpu::Buffer,
}
impl ContactTexture {
    pub fn layout(first: u32) -> [wgpu::BindGroupLayoutEntry; 3] {
        [
            wgpu::BindGroupLayoutEntry {
                binding: first,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
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
            wgpu::BindGroupLayoutEntry {
                binding: first + 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            },
        ]
    }
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mips = contact_debris::atlas();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("contact masks: dirt gravel leaves paper"),
            size: wgpu::Extent3d {
                width: contact_debris::WIDTH,
                height: contact_debris::HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: mips.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, mip) in mips.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    mip_level: level as u32,
                    ..texture.as_image_copy()
                },
                &mip.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(mip.width * 4),
                    rows_per_image: Some(mip.height),
                },
                wgpu::Extent3d {
                    width: mip.width,
                    height: mip.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("filtered contact tile"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let settings = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("approved contact settings"),
            contents: &contact_debris::Settings::default()
                .uniform()
                .into_iter()
                .flat_map(f32::to_ne_bytes)
                .collect::<Vec<_>>(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            view: texture.create_view(&Default::default()),
            sampler,
            settings,
        }
    }
    pub fn entries(&self, first: u32) -> [wgpu::BindGroupEntry<'_>; 3] {
        [
            wgpu::BindGroupEntry {
                binding: first,
                resource: wgpu::BindingResource::TextureView(&self.view),
            },
            wgpu::BindGroupEntry {
                binding: first + 1,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            },
            wgpu::BindGroupEntry {
                binding: first + 2,
                resource: self.settings.as_entire_binding(),
            },
        ]
    }
}
