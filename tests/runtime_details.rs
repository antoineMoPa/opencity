#![cfg(feature = "game-runtime")]

use opencity::{
    parse,
    runtime::{City, Region, StreetFeatureKind, SurfaceKind},
};

#[test]
fn sidewalk_furniture_follows_both_sides_of_straight_reversed_and_curved_roads() {
    for coordinates in [
        "-1,0,1,0",
        "1,0,-1,0",
        "0,-1,0,1",
        "0,1,0,-1",
        "-1,0,-0.4,0.8,0.4,0.8,1,0",
        "1,0,0.4,0.8,-0.4,0.8,-1,0",
    ] {
        for profile in [
            "sidewalk carlane sidewalk",
            "sidewalk carlane carlane carlane verge sidewalk",
        ] {
            let source = format!("road street: {profile}; end street({coordinates});");
            let city = City::from_program(&parse(&source).unwrap(), None).unwrap();
            let road = &city.roads[0];
            let mut benches = [0; 2];
            let mut lamps = [0; 2];
            for feature in city.street_features(Region::WholeMap) {
                let p = feature.position;
                // Independently project onto the curve to recover the local street
                // direction, including reversed curves and both sidewalk offsets.
                let t = (0..=4096)
                    .min_by(|a, b| {
                        let distance = |i: usize| {
                            let point = road.segment.point_at(i as f32 / 4096.);
                            (point.x - p[0]).hypot(point.y - p[1])
                        };
                        distance(*a).total_cmp(&distance(*b))
                    })
                    .unwrap() as f32
                    / 4096.;
                let center = road.segment.point_at(t);
                let tangent = road.segment.tangent_at(t);
                let angle = tangent.y.atan2(tangent.x);
                let along = [angle.cos(), angle.sin()];
                let inward = [feature.heading.cos(), feature.heading.sin()];
                let dot = |a: [f32; 2], b: [f32; 2]| a[0] * b[0] + a[1] * b[1];
                let toward_center = [center.x - p[0], center.y - p[1]];
                assert!(dot(inward, along).abs() < 0.002, "{source}: {feature:?}");
                assert!(dot(inward, toward_center) > 0., "{source}: {feature:?}");
                let side =
                    usize::from(tangent.x * (p[1] - center.y) - tangent.y * (p[0] - center.x) > 0.);
                let mesh = city.detail_mesh(Region::radius([p[0], p[1]], 0.00001).unwrap());
                assert!(!mesh.vertices.is_empty());
                let offsets = mesh
                    .vertices
                    .iter()
                    .map(|v| [v.position[0] - p[0], v.position[1] - p[1]]);
                match feature.kind {
                    StreetFeatureKind::Bench => {
                        benches[side] += 1;
                        let extent = |axis| {
                            let projected: Vec<_> = offsets.clone().map(|p| dot(p, axis)).collect();
                            projected.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                                - projected.iter().copied().fold(f32::INFINITY, f32::min)
                        };
                        assert!(extent(along) > 2. * extent(inward));
                        // Only the backrest reaches above the seat: every point
                        // must sit on the side facing away from the carriageway.
                        let back: Vec<_> = mesh
                            .vertices
                            .iter()
                            .filter(|v| v.position[2] > p[2] + 0.0061)
                            .collect();
                        assert!(!back.is_empty());
                        assert!(back.iter().all(|v| {
                            dot([v.position[0] - p[0], v.position[1] - p[1]], inward) < -0.0019
                        }));
                    }
                    StreetFeatureKind::Lamp => {
                        lamps[side] += 1;
                        let light: Vec<_> = mesh
                            .vertices
                            .iter()
                            .filter(|v| v.surface[0] == SurfaceKind::Light as u32 as f32)
                            .collect();
                        assert!(!light.is_empty());
                        assert!(light.iter().all(|v| {
                            dot([v.position[0] - p[0], v.position[1] - p[1]], inward) > 0.0049
                        }));
                    }
                    StreetFeatureKind::Tree => unreachable!(),
                }
            }
            assert!(benches.into_iter().all(|count| count > 0), "{source}");
            assert!(lamps.into_iter().all(|count| count > 0), "{source}");
        }
    }
}
