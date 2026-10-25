use opencity::{
    heightmap::ElevationMapSpec, map, project_landscape_features, road_geometry::RoadGeometry,
    LandscapeFeature, LandscapeKind,
};
use std::path::Path;

fn bounds(feature: &LandscapeFeature) -> [f32; 4] {
    feature.points.iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |b, p| {
            [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ]
        },
    )
}

fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}

#[test]
fn every_enclosed_city_block_has_nine_lots_and_a_taller_center() {
    let program = opencity::parse(include_str!("../examples/default_map.road")).unwrap();
    let projection = map::project(&program);
    let landscape = project_landscape_features(&program);
    assert!(landscape.errors.is_empty());
    let buildings: Vec<_> = landscape
        .features
        .iter()
        .filter(|f| f.kind == LandscapeKind::Building)
        .collect();
    let mut assigned = 0;
    for city in ["west", "east"] {
        let prefix = format!("{city}_j");
        let nodes: Vec<_> = projection
            .points
            .iter()
            .filter(|p| {
                p.name
                    .strip_prefix(&prefix)
                    .is_some_and(|s| s.parse::<usize>().is_ok())
            })
            .map(|p| [p.position.x, p.position.y])
            .collect();
        let mut xs: Vec<_> = nodes.iter().map(|p| p[0]).collect();
        let mut ys: Vec<_> = nodes.iter().map(|p| p[1]).collect();
        xs.sort_by(f32::total_cmp);
        xs.dedup();
        ys.sort_by(f32::total_cmp);
        ys.dedup();
        let mut block_count = 0;
        for x in xs.windows(2) {
            for y in ys.windows(2) {
                if ![[x[0], y[0]], [x[1], y[0]], [x[1], y[1]], [x[0], y[1]]]
                    .iter()
                    .all(|p| nodes.contains(p))
                {
                    continue;
                }
                let lots: Vec<_> = buildings
                    .iter()
                    .filter(|f| {
                        let b = bounds(f);
                        b[0] > x[0] && b[2] < x[1] && b[1] > y[0] && b[3] < y[1]
                    })
                    .collect();
                assert_eq!(
                    lots.len(),
                    9,
                    "{city} block {x:?}/{y:?} must have 3×3 buildings"
                );
                for row in 0..3 {
                    for col in 0..3 {
                        let expected = [
                            x[0] + 0.15 + (col as f32 + 0.5) * (x[1] - x[0] - 0.30) / 3.0,
                            y[0] + 0.15 + (row as f32 + 0.5) * (y[1] - y[0] - 0.30) / 3.0,
                        ];
                        assert!(
                            lots.iter().any(|f| {
                                let b = bounds(f);
                                ((b[0] + b[2]) * 0.5 - expected[0]).abs() < 0.0001
                                    && ((b[1] + b[3]) * 0.5 - expected[1]).abs() < 0.0001
                            }),
                            "missing grid lot {row}/{col} in {city} block"
                        );
                    }
                }
                assigned += lots.len();
                block_count += 1;
            }
        }
        assert!(block_count > 0);
        let center = projection
            .points
            .iter()
            .find(|p| p.name == format!("{city}_city_center"))
            .unwrap()
            .position;
        let heights: Vec<_> = buildings
            .iter()
            .filter(|f| f.name.as_ref().unwrap().starts_with(city))
            .map(|f| {
                let b = bounds(f);
                (
                    ((b[0] + b[2]) * 0.5 - center.x).hypot((b[1] + b[3]) * 0.5 - center.y),
                    f.height.unwrap(),
                )
            })
            .collect();
        let radius = heights.iter().map(|p| p.0).fold(0.0_f32, f32::max);
        let mean = |inner: bool| {
            let values: Vec<_> = heights
                .iter()
                .filter(|(distance, _)| {
                    if inner {
                        *distance < radius * 0.20
                    } else {
                        *distance > radius * 0.80
                    }
                })
                .map(|p| p.1)
                .collect();
            assert!(!values.is_empty());
            values.iter().sum::<f32>() / values.len() as f32
        };
        assert!(
            mean(true) > mean(false) * 2.0,
            "{city} needs a distinct center-to-outskirts skyline"
        );
        let stories: Vec<_> = buildings
            .iter()
            .filter(|f| f.name.as_ref().unwrap().starts_with(city))
            .map(|f| f.height.unwrap() / f.building_style.unwrap().floor_height)
            .collect();
        assert!(
            stories
                .iter()
                .all(|n| (*n - n.round()).abs() < 0.001 && *n >= 0.999 && *n <= 10.001),
            "{city} buildings should use complete stories with modest center peaks"
        );
        assert!(
            stories.iter().filter(|n| **n <= 4.001).count() as f32 / stories.len() as f32 >= 0.80,
            "{city} should be dominated by 1–4 story buildings"
        );
        let distinct: std::collections::BTreeSet<_> =
            stories.iter().map(|n| n.round() as u32).collect();
        assert!(
            distinct.len() >= 6,
            "{city} needs varied low-rise and central building heights"
        );
        for building in buildings
            .iter()
            .filter(|f| f.name.as_ref().unwrap().starts_with(city))
        {
            let floors = building.height.unwrap() / building.building_style.unwrap().floor_height;
            if floors > 8.001 {
                let b = bounds(building);
                let distance =
                    ((b[0] + b[2]) * 0.5 - center.x).hypot((b[1] + b[3]) * 0.5 - center.y);
                assert!(
                    distance < radius * 0.20,
                    "tall peaks belong near {city}'s center"
                );
            }
        }
    }
    assert_eq!(
        assigned,
        buildings.len(),
        "every building belongs to an enclosed city block"
    );
}

#[test]
fn buildings_clear_streets_and_ground_and_landscape_areas_are_visible() {
    let program = opencity::parse(include_str!("../examples/default_map.road")).unwrap();
    let projection = map::project(&program);
    let terrain = ElevationMapSpec::from_program(&program)
        .unwrap()
        .unwrap()
        .load(Some(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("examples"),
        ))
        .unwrap();
    let features = project_landscape_features(&program).features;
    let materials: std::collections::BTreeSet<_> = features
        .iter()
        .filter_map(|f| f.building_style.map(|style| style.material.keyword()))
        .collect();
    assert_eq!(
        materials,
        ["brick", "concrete", "glass"].into_iter().collect()
    );
    let geometry = RoadGeometry::compile(&program, &projection);
    let mut street_boxes = Vec::new();
    for (index, _) in projection.segments.iter().enumerate() {
        for pair in geometry.cross_sections(index).windows(2) {
            street_boxes.push(pair.iter().flat_map(|row| &row.boundaries).fold(
                [
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ],
                |b, v| {
                    [
                        b[0].min(v.point.x),
                        b[1].min(v.point.y),
                        b[2].max(v.point.x),
                        b[3].max(v.point.y),
                    ]
                },
            ));
        }
    }
    for j in map::intersections(&program, &projection) {
        street_boxes.push([
            j.position.x - j.footprint_half,
            j.position.y - j.footprint_half,
            j.position.x + j.footprint_half,
            j.position.y + j.footprint_half,
        ]);
    }
    for building in features
        .iter()
        .filter(|f| f.kind == LandscapeKind::Building)
    {
        let b = bounds(building);
        assert!(
            terrain
                .surface_maximum_in([b[0], b[1]], [b[2], b[3]])
                .unwrap()
                <= building.elevation + 0.00001,
            "{:?} has a buried foundation",
            building.name
        );
        assert!(
            !street_boxes.iter().any(|&street| overlaps(b, street)),
            "{:?} overlaps a street or junction",
            building.name
        );
        assert!(building.height.unwrap() > 0.0 && building.height.unwrap().is_finite());
        assert!(building.building_style.unwrap().floor_height > 0.0);
    }
    let forests: Vec<_> = features
        .iter()
        .filter(|f| f.kind == LandscapeKind::Vegetation)
        .collect();
    assert!(!forests.is_empty());
    assert!(features.iter().any(|f| f.kind == LandscapeKind::Concrete));
    for forest in forests {
        assert_eq!(forest.vegetation_type.as_deref(), Some("forest"));
        for point in &forest.points {
            let ground = terrain.height_at(*point).unwrap();
            assert!(
                ground >= 0.0 && (forest.elevation - 0.02 - ground).abs() < 0.005,
                "{:?} forest must sit on dry ground",
                forest.name
            );
        }
    }
}
