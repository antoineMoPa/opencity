//! Navigation export owned by OpenCity, independently of a game's renderer.
use super::*;
use base64::Engine;
use std::fmt::Write;
impl City {
    /// North-up SVG of authored terrain, footprints and vector roads.
    /// The viewBox uses map bounds; world Y is reflected into SVG Y once.
    pub fn minimap_svg(&self) -> Result<String, String> {
        let [x, y, right, top] = self.bounds;
        let width = (right - x).max(0.001);
        let height = (top - y).max(0.001);
        let mut svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="{}" viewBox="{x} {y} {width} {height}"><title>OpenCity navigation map</title><rect x="{x}" y="{y}" width="{width}" height="{height}" fill="#173e55"/><g transform="translate(0,{}) scale(1,-1)">"##,
            1024. * height / width,
            y + top
        );
        if let Some(e) = &self.elevation {
            let size = 512;
            let mut pixels = Vec::with_capacity(size * size * 3);
            for iy in 0..size {
                for ix in 0..size {
                    let px = ix * 1023 / (size - 1);
                    let py = iy * 1023 / (size - 1);
                    let h = e.point(px, py)[2];
                    let water = e.spec.water_level.unwrap_or(e.spec.heights[0]);
                    let rgb = if h <= water {
                        [23, 62, 85]
                    } else {
                        let dx = e.point((px + 1).min(1023), py)[2]
                            - e.point(px.saturating_sub(1), py)[2];
                        let dy = e.point(px, (py + 1).min(1023))[2]
                            - e.point(px, py.saturating_sub(1))[2];
                        let shade = (0.92 - dx * 8. + dy * 5.).clamp(0.55, 1.25);
                        [77., 105., 79.].map(|c| (c * shade) as u8)
                    };
                    pixels.extend_from_slice(&rgb);
                }
            }
            let mut png = Vec::new();
            {
                let mut encoder = png::Encoder::new(&mut png, size as u32, size as u32);
                encoder.set_color(png::ColorType::Rgb);
                encoder.set_depth(png::BitDepth::Eight);
                encoder
                    .write_header()
                    .map_err(|e| e.to_string())?
                    .write_image_data(&pixels)
                    .map_err(|e| e.to_string())?;
            }
            let data = base64::engine::general_purpose::STANDARD.encode(png);
            write!(svg,r#"<image x="{x}" y="{y}" width="{width}" height="{height}" preserveAspectRatio="none" href="data:image/png;base64,{data}"/>"#).unwrap();
        }
        for f in &self.landscape {
            let color = match f.kind {
                LandscapeKind::Building => "#869184",
                LandscapeKind::Concrete => "#969d94",
                LandscapeKind::Land | LandscapeKind::Terrain | LandscapeKind::Vegetation => {
                    "#4d694f"
                }
                LandscapeKind::Beach => "#b9b28b",
                LandscapeKind::Lake | LandscapeKind::River => "#173e55",
            };
            if f.points.len() < 3 {
                continue;
            }
            write!(svg, r#"<polygon fill="{color}" points=""#).unwrap();
            for p in &f.points {
                write!(svg, "{},{} ", p[0], p[1]).unwrap();
            }
            svg.push_str("\"/>");
        }
        // Separate edge and carriageway passes keep connected streets readable.
        for edge in [true, false] {
            for road in &self.roads {
                let s = &road.segment;
                let full: f32 = road
                    .definition
                    .elements
                    .iter()
                    .map(|e| map::element_world_width(e.kind))
                    .sum();
                let travel: f32 = road
                    .definition
                    .elements
                    .iter()
                    .filter(|e| {
                        e.kind.is_travel_lane()
                            || e.kind == crate::RoadElementKind::Middle
                            || e.kind == crate::RoadElementKind::ParkingLane
                    })
                    .map(|e| map::element_world_width(e.kind))
                    .sum();
                let stroke = if edge { "#a7afa1" } else { "#e2e5d2" };
                let w = if edge { full } else { travel.max(full * 0.5) };
                write!(svg,r#"<path d="M {} {} C {} {},{} {},{} {}" fill="none" stroke="{stroke}" stroke-width="{w}" stroke-linecap="butt"/>"#,s.start.x,s.start.y,s.controls[0].x,s.controls[0].y,s.controls[1].x,s.controls[1].y,s.end.x,s.end.y).unwrap();
            }
        }
        for component in &self.components {
            for band in &component.geometry.bands {
                if band.kind == network::BandKind::Marking {
                    continue;
                }
                let color = match band.material {
                    network::Material::Sidewalk
                    | network::Material::Concrete
                    | network::Material::RoadElement(crate::RoadElementKind::Sidewalk) => "#a7afa1",
                    network::Material::Verge
                    | network::Material::RoadElement(crate::RoadElementKind::Verge) => "#4d694f",
                    _ => "#e2e5d2",
                };
                write!(svg, r#"<path fill="{color}" d=""#).unwrap();
                for q in component.geometry.band_quads(band) {
                    for (i, p) in q.into_iter().enumerate() {
                        let p = component.world_point(p);
                        write!(svg, "{} {} {} ", if i == 0 { "M" } else { "L" }, p.x, p.y).unwrap();
                    }
                    svg.push_str("Z ");
                }
                svg.push_str("\"/>");
            }
        }
        svg.push_str("</g></svg>");
        Ok(svg)
    }
}
