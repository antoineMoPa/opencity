//! Bake an unplaced `terrain_source` .road block into its declared elevation PNG.
//! Geography stays in .road; this utility only maps landscape contours to pixels.
use opencity::{heightmap::ElevationMapSpec, parse, project_landscape_features, LandscapeKind};
use std::{fs::File, path::PathBuf};

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Round polygon corners without changing long coastal reaches.
fn rounded(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mix =
        |a: [f32; 2], b: [f32; 2], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    let mut result = Vec::new();
    for (i, &p) in points.iter().enumerate() {
        let a = mix(p, points[(i + points.len() - 1) % points.len()], 0.25);
        let b = mix(p, points[(i + 1) % points.len()], 0.25);
        for step in 0..5 {
            let t = step as f32 / 4.0;
            result.push(mix(mix(a, p, t), mix(p, b, t), t));
        }
    }
    result
}

/// Positive inland, negative offshore; distances use the .road world units.
fn shore_distance(points: &[[f32; 2]], p: [f32; 2]) -> f32 {
    let mut inside = false;
    let mut distance_squared = f32::INFINITY;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let ab = [b[0] - a[0], b[1] - a[1]];
        let ap = [p[0] - a[0], p[1] - a[1]];
        let length_squared = ab[0] * ab[0] + ab[1] * ab[1];
        let t = ((ap[0] * ab[0] + ap[1] * ab[1]) / length_squared.max(1e-12)).clamp(0.0, 1.0);
        let dx = ap[0] - ab[0] * t;
        let dy = ap[1] - ab[1] * t;
        distance_squared = distance_squared.min(dx * dx + dy * dy);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < a[0] + (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1])
        {
            inside = !inside;
        }
    }
    distance_squared.sqrt() * if inside { 1.0 } else { -1.0 }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/default_map.road"));
    let mut program = parse(&std::fs::read_to_string(&source)?)?;
    let spec = ElevationMapSpec::from_program(&program)?.ok_or("Missing elevation_map")?;
    let water = spec.water_level.ok_or("Missing water_level")?;
    // Keep the reference's shoreline and hill softness when geography grows;
    // road widths and physical heights are deliberately independent of this.
    let contour_scale = spec.size[0] / 25.6;
    // Bake only the recipe, independent of buildings and ground-cover calls.
    program
        .statements
        .retain(|statement| !matches!(statement, opencity::Statement::Call(_)));
    program
        .statements
        .extend(parse("terrain_source(0, 0);")?.statements);
    let projection = project_landscape_features(&program);
    if !projection.errors.is_empty() {
        return Err(format!("Invalid terrain recipe: {:?}", projection.errors).into());
    }
    if !projection
        .features
        .iter()
        .any(|f| f.kind == LandscapeKind::Land)
    {
        return Err("terrain_source must contain land coastlines".into());
    }
    let contours: Vec<_> = projection
        .features
        .iter()
        .map(|f| (f, rounded(&f.points)))
        .collect();
    let mut data = Vec::with_capacity(1024 * 1024 * 2);
    for y in 0..1024 {
        for x in 0..1024 {
            // PNG row zero is the south edge, as required by ElevationMap.
            let p = [
                spec.origin[0] + spec.size[0] * x as f32 / 1023.0,
                spec.origin[1] + spec.size[1] * y as f32 / 1023.0,
            ];
            let coast = contours
                .iter()
                .filter(|(f, _)| f.kind == LandscapeKind::Land)
                .map(|(_, points)| shore_distance(points, p))
                .fold(f32::NEG_INFINITY, f32::max);
            let mut height = if coast < 0.0 {
                water - 0.18 * ease(-coast / (0.55 * contour_scale))
            } else {
                water + 0.10 * ease(coast / (0.30 * contour_scale))
            };
            if coast > 0.0 {
                for (f, points) in &contours {
                    match f.kind {
                        LandscapeKind::Terrain => {
                            let hill = ease(shore_distance(points, p) / (0.65 * contour_scale));
                            let summit = f.terrain_height.unwrap_or(0.10);
                            if hill > 0.0 {
                                let relief = (0.10 + (summit - 0.10) * hill)
                                    * ease(coast / (0.30 * contour_scale));
                                height = height.max(water + relief);
                            }
                        }
                        LandscapeKind::Lake => {
                            let basin = ease(
                                (shore_distance(points, p) + 0.12 * contour_scale)
                                    / (0.24 * contour_scale),
                            );
                            height += (water - 0.08 - height) * basin;
                        }
                        _ => {}
                    }
                }
                // Let the shore reach sea level smoothly even near a hill footprint.
                height = height
                    .min(water + (spec.heights[1] - water) * ease(coast / (0.20 * contour_scale)));
            }
            let sample = ((height - spec.heights[0]) * 65535.0
                / (spec.heights[1] - spec.heights[0]))
                .round()
                .clamp(0.0, 65535.0) as u16;
            data.extend_from_slice(&sample.to_be_bytes());
        }
    }
    let output = source.parent().unwrap().join(&spec.path);
    let mut encoder = png::Encoder::new(File::create(&output)?, 1024, 1024);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Sixteen);
    encoder.write_header()?.write_image_data(&data)?;
    println!("Wrote {} (1024×1024, 16-bit)", output.display());
    Ok(())
}
