//! Elevation terrain uses the same pixel triangles as picking and clearance.
use super::*;
use opencity::heightmap::ElevationMap;

pub(super) fn append_heightmap(scene: &mut Scene, map: &ElevationMap, palette: EditorPalette) {
    scene.water_map = map.spec.water_level.map(|_| Arc::new(map.clone()));
    let base = match palette.theme {
        egui::Theme::Dark => [113.0, 137.0, 70.0],
        egui::Theme::Light => [194.0, 186.0, 139.0],
    };
    map.for_each_triangle(|triangle| {
        // Omit fully submerged bed faces so finite map edges do not expose
        // green strips below the water plane in oblique views.
        if map
            .spec
            .water_level
            .is_some_and(|level| triangle.iter().all(|p| p[2] < level))
        {
            return;
        }

        let [a, b, c] = triangle;
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let normal = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
            .sqrt()
            .max(f32::MIN_POSITIVE);
        let light = 0.65
            + 0.35
                * ((-0.4 * normal[0] - 0.3 * normal[1] + 0.8660254 * normal[2]) / length).max(0.0);
        let color = Color32::from_rgb(
            (base[0] * light) as u8,
            (base[1] * light) as u8,
            (base[2] * light) as u8,
        );
        for p in triangle {
            scene.vertices.push(WorldVertex {
                point: WorldPoint { x: p[0], y: p[1] },
                height: p[2],
                depth_bias: 0.001,
                color,
                surface: [0.0; 4],
            });
        }
    });
    let water = match palette.theme {
        egui::Theme::Dark => Color32::from_rgb(15, 99, 155),
        egui::Theme::Light => Color32::from_rgb(55, 143, 191),
    };
    map.for_each_water_triangle(|triangle| {
        for [x, y, height] in triangle {
            scene.vertices.push(WorldVertex {
                point: WorldPoint { x, y },
                height,
                depth_bias: 0.0,
                color: water,
                surface: [2.0, 0.0, 0.0, 0.0],
            });
        }
    });
}
