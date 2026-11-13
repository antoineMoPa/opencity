//! Small world-anchored contact masks, baked once and shared by editor and games.
use crate::road_geometry::{CrossSection, Wall};
use std::collections::HashSet;
use std::sync::OnceLock;
mod mesh;
pub use mesh::prepare_mesh;

/// Explicit mapping of authored road-band geometry to the shared material metadata.
pub fn pavement_band(
    road: &crate::RoadDefinition,
    rows: &[crate::road_geometry::CrossSection],
    band: usize,
    distance: f32,
    length: f32,
) -> Option<[[f32; 3]; 4]> {
    pavement_band_with_walls(road, rows, band, distance, length, [false; 2])
}

/// Marks real, complete guard-wall spans, including reversed/sloped road boundaries.
/// Build once per geometry revision; each row uses two constant-time lookups. Partial
/// spans at open road mouths are deliberately not extrapolated across the opening.
#[derive(Default)]
pub struct WallContacts(HashSet<[[i64; 3]; 2]>);
fn wall_key(a: crate::road_geometry::Vertex, b: crate::road_geometry::Vertex) -> [[i64; 3]; 2] {
    let mut key = [a, b]
        .map(|v| [v.point.x, v.point.y, v.height].map(|c| (f64::from(c) * 1e6).round() as i64));
    key.sort_unstable();
    key
}
impl WallContacts {
    pub fn new(walls: &[Wall]) -> Self {
        Self(walls.iter().map(|w| wall_key(w.start, w.end)).collect())
    }
    pub fn at_rows(&self, rows: &[CrossSection]) -> [bool; 2] {
        let [a, b] = [&rows[0], &rows[1]];
        [
            self.0.contains(&wall_key(a.boundaries[0], b.boundaries[0])),
            self.0.contains(&wall_key(
                *a.boundaries.last().unwrap(),
                *b.boundaries.last().unwrap(),
            )),
        ]
    }
}
pub fn pavement_band_with_walls(
    road: &crate::RoadDefinition,
    rows: &[CrossSection],
    band: usize,
    distance: f32,
    length: f32,
    walls: [bool; 2],
) -> Option<[[f32; 3]; 4]> {
    super::surface_weathering::band_coordinates(road, rows, band, distance, length, walls)
}

pub const WIDTH: u32 = 1024;
pub const HEIGHT: u32 = 128;
/// Matches the infrastructure shaders' existing 100 meters per map unit convention.
pub const TILE_LENGTH: f32 = 0.08;
pub const TILE_SPREAD: f32 = 0.0065;

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub grime: f32,
    pub spread: f32,
    pub debris: f32,
    pub enabled: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            grime: 0.72,
            spread: 0.0063,
            debris: 0.86,
            enabled: true,
        }
    }
}
impl Settings {
    /// Strength, map-space spread, debris opacity, enabled flag. No comparison divider.
    pub fn uniform(self) -> [f32; 4] {
        [
            self.grime,
            self.spread,
            self.debris,
            if self.enabled { 1. } else { 0. },
        ]
    }
}
pub struct Mip {
    pub width: u32,
    pub height: u32,
    /// Linear masks: R dirt, G gravel, B leaves, A paper. Never upload as sRGB.
    pub rgba: Vec<u8>,
}
pub fn atlas() -> &'static [Mip] {
    static ATLAS: OnceLock<Vec<Mip>> = OnceLock::new();
    ATLAS.get_or_init(bake)
}
struct Random(u32);
impl Random {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as f32 / 16777216.
    }
}
fn bake() -> Vec<Mip> {
    let mut random = Random(38);
    let mut pixels = vec![0; (WIDTH * HEIGHT * 4) as usize];
    let phase = random.next() * std::f32::consts::TAU;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let t = x as f32 / WIDTH as f32 * std::f32::consts::TAU;
            let v = y as f32 / HEIGHT as f32;
            let value = 0.52
                + 0.20 * (t * 3. + phase).sin()
                + 0.13 * (t * 11. - v * 3.).sin()
                + 0.09 * (t * 27. + v * 8.).cos()
                + (random.next() - 0.5) * 0.24;
            pixels[((y * WIDTH + x) * 4) as usize] = (value.clamp(0., 1.) * 255.).round() as u8;
        }
    }
    for (channel, count) in [(1, 400), (2, 65), (3, 9)] {
        for _ in 0..count {
            let x = random.next() * WIDTH as f32;
            let y = random.next().powi(2) * HEIGHT as f32 * 0.9;
            let size = match channel {
                1 => 0.5 + random.next() * 1.7,
                2 => 2.5 + random.next() * 3.,
                _ => 4. + random.next() * 3.,
            };
            let angle = random.next() * std::f32::consts::TAU;
            let (s, c) = angle.sin_cos();
            let opacity = 0.55 + random.next() * 0.45;
            let reach = (size * 1.6).ceil() as i32;
            for py in y as i32 - reach..=y as i32 + reach {
                if !(0..HEIGHT as i32).contains(&py) {
                    continue;
                }
                for px in x as i32 - reach..=x as i32 + reach {
                    let mut coverage = 0.;
                    // Four subpixel samples retain small stones in the mip chain.
                    for (dx, dy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                        let a = px as f32 + dx - x;
                        let b = py as f32 + dy - y;
                        let u = (a * c + b * s) / size;
                        let v = (-a * s + b * c) / size;
                        let inside = match channel {
                            1 => u.abs() + v.abs() * 1.2 < 1.15 && v > -0.7,
                            2 => u.abs() < 1. && v.abs() < (1. - u * u) * 0.45,
                            _ => u.abs() < 0.8 && (v + u * 0.12).abs() < 0.52,
                        };
                        if inside {
                            coverage += 0.25;
                        }
                    }
                    let i = ((py as u32 * WIDTH + px.rem_euclid(WIDTH as i32) as u32) * 4) as usize
                        + channel;
                    pixels[i] = pixels[i].max((coverage * opacity * 255.).round() as u8);
                }
            }
        }
    }
    let mut mips = vec![Mip {
        width: WIDTH,
        height: HEIGHT,
        rgba: pixels,
    }];
    while mips.last().unwrap().width > 1 || mips.last().unwrap().height > 1 {
        let prior = mips.last().unwrap();
        let width = (prior.width / 2).max(1);
        let height = (prior.height / 2).max(1);
        let mut rgba = vec![0; (width * height * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                for channel in 0..4 {
                    let mut sum = 0u32;
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let sx = (x * 2 + dx).min(prior.width - 1);
                            let sy = (y * 2 + dy).min(prior.height - 1);
                            sum += u32::from(
                                prior.rgba[((sy * prior.width + sx) * 4 + channel) as usize],
                            );
                        }
                    }
                    rgba[((y * width + x) * 4 + channel) as usize] = ((sum + 2) / 4) as u8;
                }
            }
        }
        mips.push(Mip {
            width,
            height,
            rgba,
        });
    }
    mips
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_masks_have_a_complete_small_mip_chain_and_sparse_debris() {
        let mips = atlas();
        assert_eq!(mips.len(), 11);
        assert_eq!(mips.iter().map(|m| m.rgba.len()).sum::<usize>(), 699068);
        assert_eq!((mips[10].width, mips[10].height), (1, 1));
        for channel in 1..4 {
            let occupied = mips[0]
                .rgba
                .chunks_exact(4)
                .filter(|p| p[channel] > 0)
                .count();
            assert!(occupied > 100 && occupied < (WIDTH * HEIGHT / 8) as usize);
            assert!(mips[0]
                .rgba
                .chunks_exact(4)
                .skip((WIDTH * HEIGHT * 95 / 100) as usize)
                .all(|p| p[channel] == 0));
        }
        assert!(std::ptr::eq(atlas(), mips));
    }
}
