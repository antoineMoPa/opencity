//! Clip the water plane against the same two triangles as ElevationMap::height_at.
use super::ElevationMap;
use crate::heightmap::ELEVATION_MAP_SIZE;
use std::fmt::Write;

fn clipped_triangle(path: &mut String, points: [[f32; 3]; 3], level: f32) {
    let mut command = 'M';
    let mut vertex = |x: f32, y: f32| {
        write!(path, "{command}{x:.5} {y:.5}").unwrap();
        command = 'L';
    };
    let mut previous = points[2];
    for point in points {
        if (point[2] <= level) != (previous[2] <= level) {
            let t = (level - previous[2]) / (point[2] - previous[2]);
            vertex(
                previous[0] + t * (point[0] - previous[0]),
                previous[1] + t * (point[1] - previous[1]),
            );
        }
        if point[2] <= level {
            vertex(point[0], point[1]);
        }
        previous = point;
    }
    if command == 'L' {
        path.push('Z');
    }
}

pub(super) fn write_water(svg: &mut String, map: &ElevationMap) {
    let level = map.spec.water_level.unwrap_or(map.spec.heights[0]);
    svg.push_str(r##"<path id="navigation-water" fill="#173e55" d=""##);
    let last = ELEVATION_MAP_SIZE - 1;
    for y in 0..last {
        // A fully submerged run needs just one rectangle. Only shoreline cells
        // need individual polygons, all in one SVG fill to avoid internal seams.
        let mut run = None;
        let mut a = map.point(0, y);
        let mut c = map.point(0, y + 1);
        for x in 0..last {
            let b = map.point(x + 1, y);
            let d = map.point(x + 1, y + 1);
            let flooded = [a, b, c, d].iter().filter(|p| p[2] <= level).count();
            if flooded == 4 {
                run.get_or_insert(a[0]);
            }
            if flooded != 4 || x + 1 == last {
                if let Some(left) = run.take() {
                    let right = if flooded == 4 { b[0] } else { a[0] };
                    write!(
                        svg,
                        "M{left:.5} {:.5}H{right:.5}V{:.5}H{left:.5}Z",
                        a[1], c[1]
                    )
                    .unwrap();
                }
            }
            if (1..4).contains(&flooded) {
                clipped_triangle(svg, [a, b, d], level);
                clipped_triangle(svg, [a, d, c], level);
            }
            a = b;
            c = d;
        }
    }
    svg.push_str("\"/>");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shoreline_crosses_both_terrain_triangles_at_interpolated_heights() {
        let a = [0., 0., -1.];
        let b = [1., 0., 3.];
        let c = [0., 1., -1.];
        let d = [1., 1., 1.];
        let mut path = String::new();
        clipped_triangle(&mut path, [a, b, d], 0.);
        clipped_triangle(&mut path, [a, d, c], 0.);
        assert_eq!(
            path,
            concat!(
                "M0.50000 0.50000L0.00000 0.00000L0.25000 0.00000Z",
                "M0.00000 0.00000L0.50000 0.50000L0.50000 1.00000L0.00000 1.00000Z",
            )
        );
    }
}
