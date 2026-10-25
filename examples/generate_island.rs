//! Rebuild the example's deterministic 16-bit multi-scale noise heightmap.
//! Map bounds, water, and roads remain in elevation_map.road.
use std::{fs::File, path::PathBuf};

fn lattice(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x85ebca6b) ^ (y as u32).wrapping_mul(0xc2b2ae35) ^ seed;
    h = (h ^ (h >> 16)).wrapping_mul(0x7feb352d);
    h = (h ^ (h >> 15)).wrapping_mul(0x846ca68b);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}
fn noise(x: f32, y: f32, seed: u32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    let u = ease(x - ix as f32);
    let v = ease(y - iy as f32);
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
    mix(
        mix(lattice(ix, iy, seed), lattice(ix + 1, iy, seed), u),
        mix(lattice(ix, iy + 1, seed), lattice(ix + 1, iy + 1, seed), u),
        v,
    )
}
fn island_height(x: usize, y: usize) -> f32 {
    let u = x as f32 / 1023.0;
    let v = y as f32 / 1023.0;
    let dx = (u - 0.49) * 2.0;
    let dy = (v - 0.51) * 2.0;
    let radius = (dx * dx + dy * dy * 1.12).sqrt();
    let mut detail = 0.0;
    // Broad landforms, ridges, hills, gullies, and fine surface variation.
    for (i, (frequency, amplitude)) in [
        (2.5, 0.24),
        (5.0, 0.14),
        (10.0, 0.075),
        (20.0, 0.035),
        (40.0, 0.016),
        (80.0, 0.006),
    ]
    .into_iter()
    .enumerate()
    {
        detail += (noise(
            u * frequency + 17.0,
            v * frequency + 31.0,
            0x51a7 + (i as u32) * 997,
        ) - 0.5)
            * amplitude;
    }
    let border = (u.min(1.0 - u).min(v.min(1.0 - v)) * 12.0).clamp(0.0, 1.0);
    ((0.80 - radius) * 0.97 + detail).clamp(0.0, 1.0) * border
}
fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/elevation.png"));
    let mut data = Vec::with_capacity(1024 * 1024 * 2);
    for y in 0..1024 {
        for x in 0..1024 {
            let value = (island_height(x, y) * 65535.0).round() as u16;
            data.extend_from_slice(&value.to_be_bytes());
        }
    }
    let mut encoder =
        png::Encoder::new(File::create(&output).expect("create heightmap"), 1024, 1024);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Sixteen);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&data)
        .unwrap();
    println!("Wrote {}", output.display());
}
