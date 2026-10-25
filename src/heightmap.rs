//! A georeferenced 1024 × 1024 grayscale PNG terrain surface.
use crate::{Argument, Program, Statement};
use std::{io::Cursor, path::Path};

pub const ELEVATION_MAP_SIZE: usize = 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct ElevationMapSpec {
    pub path: String,
    pub origin: [f32; 2],
    pub size: [f32; 2],
    pub heights: [f32; 2],
    pub water_level: Option<f32>,
}

impl ElevationMapSpec {
    /// One top-level declaration: elevation_map("file.png", x, y, width, depth, low, high).
    pub fn from_program(program: &Program) -> Result<Option<Self>, String> {
        let mut result = None;
        let mut water_level = None;
        for statement in &program.statements {
            let Statement::Call(call) = statement else {
                continue;
            };
            if call.name == "water_level" {
                if water_level.is_some() {
                    return Err("Only one water_level is allowed per map".into());
                }
                let [Argument::Number(value)] = call.args.as_slice() else {
                    return Err("water_level expects one finite height".into());
                };
                water_level = Some(
                    value
                        .parse::<f32>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .ok_or("water_level expects one finite height")?,
                );
                continue;
            }
            if call.name != "elevation_map" {
                continue;
            }
            if result.is_some() {
                return Err("Only one elevation_map is allowed per map".into());
            }
            let [Argument::String(path), values @ ..] = call.args.as_slice() else {
                return Err("elevation_map requires a quoted PNG path".into());
            };
            let literal = path
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .ok_or("elevation_map requires a quoted PNG path")?;
            let mut path = String::new();
            let mut chars = literal.chars();
            while let Some(ch) = chars.next() {
                path.push(if ch == '\\' {
                    match chars.next() {
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        _ => return Err("Unsupported escape in elevation PNG path".into()),
                    }
                } else {
                    ch
                });
            }
            if values.len() != 6 || path.is_empty() {
                return Err("elevation_map expects path, x, y, width, depth, low, high".into());
            }
            let mut numbers = [0.0; 6];
            for (out, value) in numbers.iter_mut().zip(values) {
                let Argument::Number(value) = value else {
                    return Err("elevation_map bounds must be numbers".into());
                };
                *out = value
                    .parse::<f32>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .ok_or("elevation_map bounds must be finite")?;
            }
            let [x, y, width, depth, low, high] = numbers;
            if width <= 0.0
                || depth <= 0.0
                || high < low
                || ![x + width, y + depth, high - low]
                    .iter()
                    .all(|n| n.is_finite())
            {
                return Err("elevation_map needs positive dimensions and low <= high".into());
            }
            result = Some(Self {
                path: path.clone(),
                origin: [x, y],
                size: [width, depth],
                heights: [low, high],
                water_level: None,
            });
        }
        if let Some(map) = result.as_mut() {
            map.water_level = water_level;
        } else if water_level.is_some() {
            return Err("water_level requires an elevation_map".into());
        }
        Ok(result)
    }

    pub fn load(&self, base: Option<&Path>) -> Result<ElevationMap, String> {
        let path = base.unwrap_or_else(|| Path::new(".")).join(&self.path);
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        ElevationMap::from_png(self.clone(), &bytes)
    }
}

#[derive(Clone, Debug)]
pub struct ElevationMap {
    pub spec: ElevationMapSpec,
    samples: Vec<f32>,
}

impl ElevationMap {
    pub fn from_png(spec: ElevationMapSpec, bytes: &[u8]) -> Result<Self, String> {
        let mut reader = png::Decoder::new(Cursor::new(bytes))
            .read_info()
            .map_err(|e| e.to_string())?;
        let info = reader.info();
        if info.width != 1024 || info.height != 1024 {
            return Err("Elevation PNG must be exactly 1024x1024 pixels".into());
        }
        if info.color_type != png::ColorType::Grayscale
            || !matches!(
                info.bit_depth,
                png::BitDepth::Eight | png::BitDepth::Sixteen
            )
        {
            return Err("Elevation PNG must be 8-bit or 16-bit grayscale without alpha".into());
        }
        let mut buffer = vec![0; reader.output_buffer_size()];
        let output = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
        let range = spec.heights[1] - spec.heights[0];
        let samples = match output.bit_depth {
            png::BitDepth::Eight => buffer[..output.buffer_size()]
                .iter()
                .map(|v| spec.heights[0] + *v as f32 * (range / 255.0))
                .collect(),
            png::BitDepth::Sixteen => buffer[..output.buffer_size()]
                .chunks_exact(2)
                .map(|v| {
                    spec.heights[0] + u16::from_be_bytes([v[0], v[1]]) as f32 * (range / 65535.0)
                })
                .collect(),
            _ => unreachable!(),
        };
        Ok(Self { spec, samples })
    }

    pub fn point(&self, x: usize, y: usize) -> [f32; 3] {
        [
            self.spec.origin[0] + self.spec.size[0] * (x as f32 / 1023.0),
            self.spec.origin[1] + self.spec.size[1] * (y as f32 / 1023.0),
            self.samples[y * ELEVATION_MAP_SIZE + x],
        ]
    }

    /// Matches the two triangles used to render every pixel cell. PNG row zero
    /// lies at the minimum world Y. Outside the rectangle there is no surface.
    pub fn height_at(&self, point: [f32; 2]) -> Option<f32> {
        let x = (point[0] - self.spec.origin[0]) / self.spec.size[0] * 1023.0;
        let y = (point[1] - self.spec.origin[1]) / self.spec.size[1] * 1023.0;
        if !x.is_finite()
            || !y.is_finite()
            || !(0.0..=1023.0).contains(&x)
            || !(0.0..=1023.0).contains(&y)
        {
            return None;
        }
        let ix = (x as usize).min(1022);
        let iy = (y as usize).min(1022);
        let (u, v) = (x - ix as f32, y - iy as f32);
        let a = self.point(ix, iy)[2];
        let b = self.point(ix + 1, iy)[2];
        let c = self.point(ix, iy + 1)[2];
        let d = self.point(ix + 1, iy + 1)[2];
        Some(if u >= v {
            a + u * (b - a) + v * (d - b)
        } else {
            a + v * (c - a) + u * (d - c)
        })
    }

    /// The visible surface is water wherever the terrain is below sea level.
    pub fn surface_height_at(&self, point: [f32; 2]) -> Option<f32> {
        self.height_at(point)
            .map(|z| self.spec.water_level.map_or(z, |level| z.max(level)))
    }

    /// One horizontal water plane. Depth testing exposes precisely the flooded
    /// parts of the terrain without quantizing the coastline to pixel edges.
    pub fn for_each_water_triangle(&self, mut visit: impl FnMut([[f32; 3]; 3])) {
        let Some(level) = self.spec.water_level else {
            return;
        };
        let [a, b, c, d] = [
            self.point(0, 0),
            self.point(1023, 0),
            self.point(0, 1023),
            self.point(1023, 1023),
        ]
        .map(|[x, y, _]| [x, y, level]);
        visit([a, b, d]);
        visit([a, d, c]);
    }

    pub fn surface_maximum_in(&self, min: [f32; 2], max: [f32; 2]) -> Option<f32> {
        self.maximum_in(min, max)
            .map(|z| self.spec.water_level.map_or(z, |level| z.max(level)))
    }

    /// Visit the full-resolution surface without allocating a triangle buffer.
    /// Only coplanar pixel regions are merged; single-pixel peaks survive.
    pub fn for_each_triangle(&self, mut visit: impl FnMut([[f32; 3]; 3])) {
        fn patch(map: &ElevationMap, visit: &mut impl FnMut([[f32; 3]; 3]), bounds: [usize; 4]) {
            let [x0, y0, x1, y1] = bounds;
            let [a, b, c, d] = [
                map.point(x0, y0),
                map.point(x1, y0),
                map.point(x0, y1),
                map.point(x1, y1),
            ];
            let planar = (y0..=y1).all(|y| {
                (x0..=x1).all(|x| {
                    let u = (x - x0) as f32 / (x1 - x0) as f32;
                    let v = (y - y0) as f32 / (y1 - y0) as f32;
                    let z = a[2] + u * (b[2] - a[2]) + v * (c[2] - a[2]);
                    (map.point(x, y)[2] - z).abs() <= 1e-6
                })
            });
            if planar || (x1 - x0 == 1 && y1 - y0 == 1) {
                visit([a, b, d]);
                visit([a, d, c]);
            } else {
                let xm = (x0 + x1) / 2;
                let ym = (y0 + y1) / 2;
                let xs = if xm > x0 {
                    [(x0, xm), (xm, x1)]
                } else {
                    [(x0, x1), (x1, x1)]
                };
                let ys = if ym > y0 {
                    [(y0, ym), (ym, y1)]
                } else {
                    [(y0, y1), (y1, y1)]
                };
                for (y0, y1) in ys {
                    for (x0, x1) in xs {
                        if x0 < x1 && y0 < y1 {
                            patch(map, visit, [x0, y0, x1, y1]);
                        }
                    }
                }
            }
        }
        patch(self, &mut visit, [0, 0, 1023, 1023]);
    }

    /// Conservative cell maximum over a world rectangle, including its rim.
    /// Road placement uses this to detect narrow peaks between cross sections.
    pub fn maximum_in(&self, min: [f32; 2], max: [f32; 2]) -> Option<f32> {
        if (0..2).any(|i| {
            max[i] < self.spec.origin[i] || min[i] > self.spec.origin[i] + self.spec.size[i]
        }) {
            return None;
        }
        let pixel = |v: f32, i: usize| {
            ((v - self.spec.origin[i]) / self.spec.size[i] * 1023.0).clamp(0.0, 1023.0)
        };
        let x0 = pixel(min[0], 0).floor() as usize;
        let x1 = pixel(max[0], 0).ceil() as usize;
        let y0 = pixel(min[1], 1).floor() as usize;
        let y1 = pixel(max[1], 1).ceil() as usize;
        Some(
            (y0..=y1)
                .flat_map(|y| (x0..=x1).map(move |x| self.samples[y * 1024 + x]))
                .fold(f32::NEG_INFINITY, f32::max),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn png_bytes(
        depth: png::BitDepth,
        width: u32,
        color: png::ColorType,
        data: &[u8],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, 1024);
            encoder.set_color(color);
            encoder.set_depth(depth);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(data)
                .unwrap();
        }
        bytes
    }
    fn spec() -> ElevationMapSpec {
        ElevationMapSpec::from_program(
            &crate::parse("elevation_map(\"hill.png\", -2, -3, 10, 20, -1, 3);").unwrap(),
        )
        .unwrap()
        .unwrap()
    }
    #[test]
    fn maps_8_and_16_bit_heights_and_world_bounds() {
        for (depth, data) in [
            (png::BitDepth::Eight, vec![255; 1024 * 1024]),
            (png::BitDepth::Sixteen, vec![255; 1024 * 1024 * 2]),
        ] {
            let map = ElevationMap::from_png(
                spec(),
                &png_bytes(depth, 1024, png::ColorType::Grayscale, &data),
            )
            .unwrap();
            assert_eq!(map.height_at([-2.0, -3.0]), Some(3.0));
            assert_eq!(map.height_at([8.0, 17.0]), Some(3.0));
            assert_eq!(map.height_at([9.0, 17.0]), None);
            assert_eq!(map.height_at([f32::NAN, 0.0]), None);
        }
    }
    #[test]
    fn retains_16_bit_precision_and_pixel_orientation() {
        let mut data = vec![0; 1024 * 1024 * 2];
        data[0..2].copy_from_slice(&32768_u16.to_be_bytes());
        let map = ElevationMap::from_png(
            spec(),
            &png_bytes(
                png::BitDepth::Sixteen,
                1024,
                png::ColorType::Grayscale,
                &data,
            ),
        )
        .unwrap();
        assert!((map.height_at([-2.0, -3.0]).unwrap() - 1.0000305).abs() < 1e-6);
        assert_eq!(map.height_at([-2.0, 17.0]), Some(-1.0));
        let mid = [-2.0 + 5.0 / 1023.0, -3.0];
        assert!((map.height_at(mid).unwrap() - 0.0000153).abs() < 0.0001);
    }
    #[test]
    fn rejects_wrong_dimensions_color_and_bad_declarations() {
        assert!(ElevationMap::from_png(
            spec(),
            &png_bytes(
                png::BitDepth::Eight,
                2,
                png::ColorType::Grayscale,
                &vec![0; 2048]
            )
        )
        .unwrap_err()
        .contains("1024x1024"));
        assert!(ElevationMap::from_png(
            spec(),
            &png_bytes(
                png::BitDepth::Eight,
                1024,
                png::ColorType::Rgb,
                &vec![0; 1024 * 1024 * 3]
            )
        )
        .unwrap_err()
        .contains("grayscale"));
        for source in [
            "elevation_map(\"a\",0,0,0,1,0,1);",
            "elevation_map(\"a\",0,0,1,1,2,1);",
            "elevation_map(\"a\",0,0,1,1,0);",
            "elevation_map(\"a\",0,0,1,1,0,1); elevation_map(\"b\",0,0,1,1,0,1);",
        ] {
            assert!(ElevationMapSpec::from_program(&crate::parse(source).unwrap()).is_err());
        }
    }
    #[test]
    fn cell_maximum_catches_single_pixel_peaks() {
        let mut data = vec![0; 1024 * 1024];
        data[512 * 1024 + 512] = 255;
        let map = ElevationMap::from_png(
            spec(),
            &png_bytes(png::BitDepth::Eight, 1024, png::ColorType::Grayscale, &data),
        )
        .unwrap();
        assert_eq!(map.maximum_in([2.9, 6.9], [3.1, 7.1]), Some(3.0));
        assert_eq!(map.maximum_in([20.0, 20.0], [21.0, 21.0]), None);
    }
}

#[cfg(test)]
mod mesh_tests {
    use super::*;
    #[test]
    fn merged_surface_preserves_peaks_and_matches_point_sampling() {
        let mut data = vec![0; 1024 * 1024];
        data[512 * 1024 + 512] = 255;
        let spec = ElevationMapSpec::from_program(
            &crate::parse("elevation_map(\"peak.png\",0,0,1023,1023,0,1);").unwrap(),
        )
        .unwrap()
        .unwrap();
        let bytes =
            super::tests::png_bytes(png::BitDepth::Eight, 1024, png::ColorType::Grayscale, &data);
        let map = ElevationMap::from_png(spec, &bytes).unwrap();
        let mut highest = 0.0_f32;
        map.for_each_triangle(|triangle| {
            for p in triangle {
                highest = highest.max(p[2]);
            }
            let center = [0, 1, 2].map(|i| triangle.iter().map(|p| p[i]).sum::<f32>() / 3.0);
            assert!((map.height_at([center[0], center[1]]).unwrap() - center[2]).abs() < 0.0001);
        });
        assert_eq!(highest, 1.0);
    }
}

#[cfg(test)]
mod water_tests {
    use super::*;
    #[test]
    fn water_threshold_preserves_bed_height_and_floods_only_low_terrain() {
        let program =
            crate::parse("water_level(0.35); elevation_map(\"island.png\",-2,-2,4,4,0,1);")
                .unwrap();
        let spec = ElevationMapSpec::from_program(&program).unwrap().unwrap();
        let mut data = vec![0; 1024 * 1024];
        for row in data.chunks_exact_mut(1024) {
            row[512..].fill(255);
        }
        let bytes =
            super::tests::png_bytes(png::BitDepth::Eight, 1024, png::ColorType::Grayscale, &data);
        let map = ElevationMap::from_png(spec, &bytes).unwrap();
        assert_eq!(map.height_at([-1.0, 0.0]), Some(0.0));
        assert_eq!(map.surface_height_at([-1.0, 0.0]), Some(0.35));
        assert_eq!(map.surface_height_at([1.0, 0.0]), Some(1.0));
        assert_eq!(map.surface_height_at([3.0, 0.0]), None);
        assert_eq!(
            map.surface_maximum_in([-1.5, -1.0], [-0.5, 1.0]),
            Some(0.35)
        );
        let mut triangles = 0;
        map.for_each_water_triangle(|triangle| {
            triangles += 1;
            assert!(triangle.iter().all(|p| p[2] == 0.35));
        });
        assert_eq!(triangles, 2);
        assert_eq!(crate::parse(&program.to_road_source()).unwrap(), program);
    }
    #[test]
    fn water_declarations_reject_missing_map_bad_values_and_duplicates() {
        for source in [
            "water_level(0.3);",
            "elevation_map(\"a\",0,0,1,1,0,1); water_level(high);",
            "elevation_map(\"a\",0,0,1,1,0,1); water_level(0,1);",
            "elevation_map(\"a\",0,0,1,1,0,1); water_level(0.1); water_level(0.2);",
        ] {
            assert!(ElevationMapSpec::from_program(&crate::parse(source).unwrap()).is_err());
        }
    }
}
