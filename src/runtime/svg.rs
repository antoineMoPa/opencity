//! Navigation export owned by OpenCity, independently of a game's renderer.
mod shore;
use super::*;
use crate::heightmap::ELEVATION_MAP_SIZE;
use base64::Engine;
use std::fmt::Write;

/// sRGB colors for land, road surfaces and road edges in navigation SVGs.
pub struct NavigationPalette {
    pub grass: [u8; 3],
    pub street: [u8; 3],
    pub street_edge: [u8; 3],
}

impl Default for NavigationPalette {
    fn default() -> Self {
        Self {
            grass: [77, 105, 79],
            street: [226, 229, 210],
            street_edge: [167, 175, 161],
        }
    }
}

impl City {
    /// North-up SVG of authored terrain, footprints and vector roads.
    /// The viewBox uses map bounds; world Y is reflected into SVG Y once.
    pub fn minimap_svg(&self) -> Result<String, String> {
        self.minimap_svg_with_palette(&NavigationPalette::default())
    }
    /// Render the north-up navigation map with custom land and street colors.
    pub fn minimap_svg_with_palette(&self, palette: &NavigationPalette) -> Result<String, String> {
        self.navigation_svg(true, true, palette)
    }
    /// Elevation shading beneath the game's resolution-independent map paths.
    pub fn minimap_background_svg(&self) -> Result<String, String> {
        self.navigation_svg(false, true, &NavigationPalette::default())
    }
    /// Relief underlay for renderers that draw the vector water and roads themselves.
    pub fn minimap_relief_svg(&self) -> Result<String, String> {
        self.minimap_relief_svg_with_palette(&NavigationPalette::default())
    }
    /// Render the relief underlay using the palette's land color.
    pub fn minimap_relief_svg_with_palette(
        &self,
        palette: &NavigationPalette,
    ) -> Result<String, String> {
        self.navigation_svg(false, false, palette)
    }
    fn navigation_svg(
        &self,
        geometry: bool,
        water: bool,
        palette: &NavigationPalette,
    ) -> Result<String, String> {
        let [x, y, right, top] = self.bounds;
        let width = (right - x).max(0.001);
        let height = (top - y).max(0.001);
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1024\" \
                height=\"{}\" viewBox=\"{x} {y} {width} \
                {height}\"><title>OpenCity navigation map</title><rect id=\"navigation-ocean\" x=\"{x}\" \
                y=\"{y}\" width=\"{width}\" height=\"{height}\" \
                fill=\"#173e55\"/><g transform=\"translate(0,{}) scale(1,-1)\">",
            1024. * height / width,
            y + top
        );
        if let Some(e) = &self.elevation {
            let size = ELEVATION_MAP_SIZE;
            let mut pixels = Vec::with_capacity(size * size * 3);
            for py in 0..size {
                for px in 0..size {
                    let dx = e.point((px + 1).min(size - 1), py)[2]
                        - e.point(px.saturating_sub(1), py)[2];
                    let dy = e.point(px, (py + 1).min(size - 1))[2]
                        - e.point(px, py.saturating_sub(1))[2];
                    let shade = (0.92 - dx * 8. + dy * 5.).clamp(0.55, 1.25);
                    pixels.extend(palette.grass.map(|c| (f32::from(c) * shade) as u8));
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
            let [x, y] = e.spec.origin;
            let [width, height] = e.spec.size;
            write!(
                svg,
                "<image x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" \
                    preserveAspectRatio=\"none\" href=\"data:image/png;base64,\
                    {data}\"/>"
            )
            .unwrap();
            if water {
                shore::write_water(&mut svg, e);
            }
        }
        if !geometry {
            svg.push_str("</g></svg>");
            return Ok(svg);
        }
        let [grass, street, street_edge] = [palette.grass, palette.street, palette.street_edge]
            .map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}"));
        for f in &self.landscape {
            let color = match f.kind {
                LandscapeKind::Building => "#869184",
                LandscapeKind::Concrete => "#969d94",
                LandscapeKind::Land | LandscapeKind::Terrain | LandscapeKind::Vegetation => &grass,
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
                let stroke = if edge { &street_edge } else { &street };
                let w = if edge { full } else { travel.max(full * 0.5) };
                write!(
                    svg,
                    "<path d=\"M {} {} C {} {},{} {},{} {}\" fill=\"none\" \
                        stroke=\"{stroke}\" stroke-width=\"{w}\" \
                        stroke-linecap=\"butt\"/>",
                    s.start.x,
                    s.start.y,
                    s.controls[0].x,
                    s.controls[0].y,
                    s.controls[1].x,
                    s.controls[1].y,
                    s.end.x,
                    s.end.y
                )
                .unwrap();
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
                    | network::Material::RoadElement(crate::RoadElementKind::Sidewalk) => {
                        &street_edge
                    }
                    network::Material::Verge
                    | network::Material::RoadElement(crate::RoadElementKind::Verge) => &grass,
                    _ => &street,
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
