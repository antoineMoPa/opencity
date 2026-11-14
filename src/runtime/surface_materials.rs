//! Scanned pavement shared by the editor and runtime. No per-frame uploads.
use std::sync::OnceLock;

pub const SIZE: u32 = 1024;
pub const TILE_METERS: [f32; 2] = [3., 2.];
pub struct Mip {
    pub size: u32,
    pub rgba: Vec<u8>,
}

fn decode(bytes: &[u8]) -> Vec<u8> {
    let mut reader = png::Decoder::new(bytes)
        .read_info()
        .expect("bundled surface PNG");
    let mut data = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut data)
        .expect("bundled surface pixels");
    assert_eq!(
        (info.width, info.height, info.color_type),
        (SIZE, SIZE, png::ColorType::Rgba)
    );
    data.truncate(info.buffer_size());
    data
}
fn linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
fn encoded(value: f32) -> f32 {
    if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1. / 2.4) - 0.055
    }
}
fn mip_chain(bytes: &[u8], color: bool) -> Vec<Mip> {
    let mut result = vec![Mip {
        size: SIZE,
        rgba: decode(bytes),
    }];
    while result.last().unwrap().size > 1 {
        let prev = result.last().unwrap();
        let size = prev.size / 2;
        let mut rgba = vec![0; (size * size * 4) as usize];
        for y in 0..size {
            for x in 0..size {
                let mut average = [0.; 4];
                for dy in 0..2 {
                    for dx in 0..2 {
                        let index = (((y * 2 + dy) * prev.size + x * 2 + dx) * 4) as usize;
                        for c in 0..4 {
                            let value = prev.rgba[index + c] as f32 / 255.;
                            average[c] += if color && c < 3 {
                                linear(value) * 0.25
                            } else {
                                value * 0.25
                            };
                        }
                    }
                }
                // Keep the averaged normal length: the shader uses it to broaden
                // roughness when subpixel bumps disappear. Never sharpen mip normals.
                for c in 0..4 {
                    let value = if color && c < 3 {
                        encoded(average[c])
                    } else {
                        average[c]
                    };
                    rgba[((y * size + x) * 4) as usize + c] = (value * 255.).round() as u8;
                }
            }
        }
        result.push(Mip { size, rgba });
    }
    result
}
pub fn tiles() -> &'static [[Vec<Mip>; 2]; 2] {
    static TILES: OnceLock<[[Vec<Mip>; 2]; 2]> = OnceLock::new();
    TILES.get_or_init(|| {
        [
            [
                mip_chain(
                    include_bytes!("../../assets/materials/asphalt-color.png"),
                    true,
                ),
                mip_chain(
                    include_bytes!("../../assets/materials/asphalt-normal-roughness.png"),
                    false,
                ),
            ],
            [
                mip_chain(
                    include_bytes!("../../assets/materials/concrete-color.png"),
                    true,
                ),
                mip_chain(
                    include_bytes!("../../assets/materials/concrete-normal-roughness.png"),
                    false,
                ),
            ],
        ]
    })
}

#[cfg(feature = "render-materials")]
mod gpu;
#[cfg(feature = "render-materials")]
pub use gpu::SurfaceTextures;
#[cfg(feature = "render-materials")]
mod environment;
#[cfg(feature = "render-materials")]
pub use environment::EnvironmentMap;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scans_have_complete_mips_and_nonconstant_roughness() {
        for tile in tiles() {
            for chain in tile {
                assert_eq!(chain.len(), 11);
                assert_eq!(chain.last().unwrap().rgba.len(), 4);
                for mip in chain {
                    assert_eq!(mip.rgba.len(), (mip.size * mip.size * 4) as usize);
                }
            }
            let roughness: Vec<_> = tile[1][0].rgba.chunks_exact(4).map(|p| p[3]).collect();
            assert!(roughness.iter().max().unwrap() - roughness.iter().min().unwrap() > 30);
        }
    }
    #[test]
    fn srgb_mip_average_preserves_linear_energy() {
        let average = encoded((linear(0.) + linear(1.)) * 0.5);
        assert!((average - 0.735357).abs() < 0.00001);
    }
}
