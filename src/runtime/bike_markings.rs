//! Bicycle stencils mapped onto the same cross sections as the pavement.
use super::{geometry, City, SurfaceKind};
use crate::{road_geometry::CrossSection, RoadDefinition, RoadDirection, RoadElementKind};

/// Read the upright bicycle from the approaching rider's viewpoint.
/// Glyph X spans the lane; glyph Y points in its direction of travel.
fn stencil_coordinates(point: [f32; 2], direction: RoadDirection) -> [f32; 2] {
    let sign = match direction {
        RoadDirection::Reverse => -1.,
        RoadDirection::Forward | RoadDirection::Both => 1.,
    };
    [point[1] * 0.03 * sign, point[0] * 0.4 * sign]
}

pub(super) fn build(city: &mut City, road_id: usize, road: &RoadDefinition, rows: &[CrossSection]) {
    for (band, element) in road.elements.iter().enumerate() {
        if element.kind != RoadElementKind::BikeLane || rows.len() < 2 {
            continue;
        }
        let direction = road
            .lane_direction(band)
            .expect("bike lane travel direction");
        let center = |row: &CrossSection| {
            let a = row.boundaries[band].point;
            let b = row.boundaries[band + 1].point;
            [(a.x + b.x) * 0.5, (a.y + b.y) * 0.5]
        };
        let mut distances = vec![0.];
        for pair in rows.windows(2) {
            let a = center(&pair[0]);
            let b = center(&pair[1]);
            distances.push(distances.last().unwrap() + (b[0] - a[0]).hypot(b[1] - a[1]));
        }
        let total = *distances.last().unwrap();
        let point = |along: f32, across: f32| {
            let i = distances
                .partition_point(|&d| d < along)
                .clamp(1, rows.len() - 1);
            let t = (along - distances[i - 1]) / (distances[i] - distances[i - 1]).max(1e-8);
            let at = |row: &CrossSection| {
                let a = row.boundaries[band];
                let b = row.boundaries[band + 1];
                let u = 0.5 + across;
                [
                    a.point.x + (b.point.x - a.point.x) * u,
                    a.point.y + (b.point.y - a.point.y) * u,
                    a.height + (b.height - a.height) * u + 0.0003,
                ]
            };
            let a = at(&rows[i - 1]);
            let b = at(&rows[i]);
            std::array::from_fn(|j| a[j] + (b[j] - a[j]) * t)
        };
        let mut station = 0.2;
        while station + 0.04 < total {
            // Wheels sit across the lane; the top of the icon faces travel.
            let mut stroke = |a: [f32; 2], b: [f32; 2]| {
                let dx = b[0] - a[0];
                let dy = b[1] - a[1];
                let length = dx.hypot(dy);
                let offset = [-dy / length * 0.025, dx / length * 0.025];
                let p = [
                    [a[0] + offset[0], a[1] + offset[1]],
                    [b[0] + offset[0], b[1] + offset[1]],
                    [b[0] - offset[0], b[1] - offset[1]],
                    [a[0] - offset[0], a[1] - offset[1]],
                ]
                .map(|p| {
                    let [along, across] = stencil_coordinates(p, direction);
                    point(station + along, across)
                });
                for ids in [[0, 1, 2], [0, 2, 3]] {
                    let mut triangle = ids.map(|i| p[i]);
                    let [a, b, c] = triangle;
                    if (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) < 0. {
                        triangle.swap(1, 2);
                    }
                    geometry::push(
                        city,
                        triangle,
                        SurfaceKind::Marking,
                        Some(road_id),
                        [222, 223, 210],
                        [0.; 3],
                    );
                }
            };
            for x in [-0.55, 0.55] {
                for i in 0..12 {
                    let wheel = |i: usize| {
                        let angle = i as f32 * std::f32::consts::TAU / 12.;
                        [x + 0.3 * angle.cos(), -0.4 + 0.3 * angle.sin()]
                    };
                    stroke(wheel(i), wheel(i + 1));
                }
            }
            for (a, b) in [
                ([-0.55, -0.4], [-0.25, 0.3]),
                ([-0.25, 0.3], [0.1, -0.4]),
                ([0.1, -0.4], [-0.55, -0.4]),
                ([-0.25, 0.3], [0.4, 0.3]),
                ([0.4, 0.3], [0.1, -0.4]),
                ([0.55, -0.4], [0.35, 0.6]),
                ([0.35, 0.6], [0.6, 0.6]),
                ([-0.4, 0.35], [-0.1, 0.35]),
            ] {
                stroke(a, b);
            }
            station += 0.4;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stencils_are_upright_for_opposing_and_one_way_lanes() {
        for (direction, signs) in [
            ("both", [-1., 1.]),
            ("forward", [1., 1.]),
            ("reverse", [-1., -1.]),
        ] {
            let program = crate::parse(&format!(
                "road r: bikelane carlane middle carlane bikelane; direction {direction}; end r(0,0,1,0);"
            )).unwrap();
            let mut city = City::from_program(&program, None).unwrap();
            let road = city.roads[0].definition.clone();
            let projection = crate::map::project(&program);
            let geometry = crate::road_geometry::RoadGeometry::compile(&program, &projection);
            city.triangles.clear();
            build(&mut city, 0, &road, geometry.cross_sections(0));
            assert_eq!(city.triangles.len(), 4 * 64);
            for (i, glyph) in city.triangles.chunks_exact(64).enumerate() {
                let center = |range: std::ops::Range<usize>| {
                    let points: Vec<_> = glyph[range].iter().flat_map(|t| t.points).collect();
                    [0, 1].map(|axis| {
                        points.iter().map(|p| p[axis]).sum::<f32>() / points.len() as f32
                    })
                };
                let rear = center(0..24);
                let front = center(24..48);
                let handlebar = center(60..62);
                assert!(
                    (rear[0] - front[0]).abs() < 1e-5,
                    "wheels must lie across the lane"
                );
                assert!((rear[1] - front[1]).abs() > 0.01);
                assert!(
                    (handlebar[0] - rear[0]) * signs[i / 2] > 0.02,
                    "top of stencil must face lane travel for {direction}"
                );
            }
        }
    }

    #[test]
    fn bike_motor_separator_is_continuous_and_stencils_belong_to_road() {
        let program = crate::parse("road r: bikelane carlane; end r(0,0,1,0);").unwrap();
        let city = City::from_program(&program, None).unwrap();
        let projection = crate::map::project(&program);
        let geometry = crate::road_geometry::RoadGeometry::compile(&program, &projection);
        let rows = geometry.cross_sections(0);
        let markings: Vec<_> = city
            .triangles
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind == SurfaceKind::Marking)
            .collect();
        assert!(markings
            .iter()
            .all(|(i, _)| city.roads[0].triangles.contains(i)));
        for pair in rows.windows(2) {
            let a = pair[0].boundaries[1].point;
            let b = pair[1].boundaries[1].point;
            let point = [(a.x + b.x) * 0.5, (a.y + b.y) * 0.5];
            assert!(
                markings
                    .iter()
                    .any(|(_, t)| geometry::triangle_height(t.points, point).is_some()),
                "bike/motor separator must have no dashed gaps"
            );
        }
    }

    #[test]
    fn bike_stencils_follow_curved_sloped_pavement_and_stay_inside_lane() {
        let program = crate::parse(
            "road bike: bikelane; end path = bike(0,0,0.3,0,0.7,0.4,1,0.4); grade(path,0.1,0.2);",
        )
        .unwrap();
        let mut city = City::from_program(&program, None).unwrap();
        let markings: Vec<_> = city
            .triangles
            .iter()
            .filter(|t| t.kind == SurfaceKind::Marking)
            .cloned()
            .collect();
        assert!(!markings.is_empty());
        // Remove paint before probing the underlying road surface.
        city.triangles.retain(|t| t.kind != SurfaceKind::Marking);
        for triangle in markings {
            for point in triangle.points {
                let height = city
                    .triangles
                    .iter()
                    .filter(|t| t.kind == SurfaceKind::Asphalt)
                    .find_map(|t| geometry::triangle_height(t.points, [point[0], point[1]]))
                    .expect("stencil must remain on bike pavement")
                    .0;
                assert!((point[2] - height - 0.0003).abs() < 0.0001);
            }
        }
    }

    #[test]
    fn short_bike_lanes_and_motor_lanes_have_no_stencils() {
        for source in [
            "road r: bikelane; end r(0,0,0.1,0);",
            "road r: carlane; end r(0,0,1,0);",
        ] {
            let city = City::from_program(&crate::parse(source).unwrap(), None).unwrap();
            assert!(!city
                .triangles
                .iter()
                .any(|t| t.kind == SurfaceKind::Marking));
        }
    }
}
