//! Static facade bands preserve building styles in viewers without our shader.
use super::*;

impl Geometry {
    pub(super) fn building_facades(&mut self, feature: &opencity::LandscapeFeature) {
        let Some(height) = feature.height else { return };
        let style = feature.building_style.unwrap_or_default();
        // Extremely fine floors are visually unresolved; avoid unbounded meshes.
        let floors = (height / style.floor_height).floor().min(512.0) as usize;
        let half_band = style.floor_height * 0.035;
        for index in 0..feature.points.len() {
            let a = feature.points[index];
            let b = feature.points[(index + 1) % feature.points.len()];
            let rgb = style.material.wall_rgb_for_edge(a, b);
            let color = [rgb[0], rgb[1], rgb[2], 255];
            let mut bottom = 0.0;
            for floor in 1..=floors {
                let level = floor as f32 * style.floor_height;
                if level + half_band >= height {
                    break;
                }
                self.facade_panel(
                    a,
                    b,
                    feature.elevation + bottom,
                    feature.elevation + level - half_band,
                    color,
                );
                self.facade_panel(
                    a,
                    b,
                    feature.elevation + level - half_band,
                    feature.elevation + level + half_band,
                    [205, 210, 214, 255],
                );
                bottom = level + half_band;
            }
            self.facade_panel(
                a,
                b,
                feature.elevation + bottom,
                feature.elevation + height,
                color,
            );
        }
    }

    fn facade_panel(&mut self, a: [f32; 2], b: [f32; 2], low: f32, high: f32, color: [u8; 4]) {
        self.quad(
            [
                (WorldPoint { x: a[0], y: a[1] }, low),
                (WorldPoint { x: b[0], y: b[1] }, low),
                (WorldPoint { x: b[0], y: b[1] }, high),
                (WorldPoint { x: a[0], y: a[1] }, high),
            ],
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exported_facades_keep_materials_and_floor_bands_at_the_declared_height() {
        let program = opencity::parse(
            "b=building(0.2,{material:brick,floor_height:0.04},0,0,1,0,1,1,0,\
                1); elevation(b,0.1);",
        )
        .unwrap();
        let feature = project_landscape_features(&program).features.remove(0);
        let mut mesh = Geometry::new();
        mesh.building_facades(&feature);
        assert!(mesh.colors.contains(&[205, 210, 214, 255]));
        assert!(mesh.colors.contains(&[164, 109, 91, 255]));
        assert!((mesh.min[1] - 0.1).abs() < 0.00001 && (mesh.max[1] - 0.3).abs() < 0.00001);
        assert!(mesh
            .positions
            .iter()
            .any(|p| (p[1] - 0.1386).abs() < 0.00001));
    }
}
