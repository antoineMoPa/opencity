use opencity::{
    check_design_rules,
    heightmap::{ElevationMap, ElevationMapSpec},
    map,
    road_geometry::{RoadGeometry, Vertex},
    LandscapeKind, Program, Statement,
};
use std::{collections::HashMap, path::Path};

fn scene() -> (Program, map::Projection, ElevationMap) {
    let program = opencity::parse(include_str!("../examples/default_map.road")).unwrap();
    let projection = map::project(&program);
    let terrain = ElevationMapSpec::from_program(&program)
        .unwrap()
        .expect("default map has terrain")
        .load(Some(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .as_path(),
        ))
        .unwrap();
    (program, projection, terrain)
}

#[test]
fn default_map_round_trips_and_passes_every_design_rule() {
    let (program, projection, _) = scene();
    assert_eq!(opencity::parse(&program.to_road_source()).unwrap(), program);
    assert_eq!(projection.unresolved, 0);
    assert!(!projection.segments.is_empty(), "default map needs roads");
    let geometry = RoadGeometry::compile(&program, &projection);
    assert!(geometry.unresolved.is_empty());
    let issues = check_design_rules(&program);
    assert!(issues.is_empty(), "default map design issues: {issues:#?}");
    for junction in map::intersections(&program, &projection) {
        let widest_approach = projection
            .segments
            .iter()
            .filter(|segment| {
                [&segment.start_ref, &segment.end_ref]
                    .into_iter()
                    .any(|reference| {
                        reference.as_deref().is_some_and(|reference| {
                            reference == junction.point_ref
                                || junction
                                    .ports
                                    .iter()
                                    .any(|port| port.point_ref == reference)
                        })
                    })
            })
            .filter_map(|segment| opencity::road_profiles::road_for_segment(&program, segment))
            .map(|road| {
                road.elements
                    .iter()
                    .map(|e| map::element_world_width(e.kind))
                    .sum::<f32>()
            })
            .fold(0.0_f32, f32::max);
        assert!(
            junction.footprint_half <= widest_approach + 0.0001,
            "{} junction footprint grew with the terrain instead of matching its roads",
            junction.point_ref
        );
        if junction.kind == map::IntersectionKind::ThreeWay {
            assert!(
                junction.footprint_half <= widest_approach * 0.5 + 0.0001,
                "{} T-junction flares wider than its approach streets",
                junction.point_ref
            );
        }
    }
}

fn assert_surface_clear(terrain: &ElevationMap, vertices: &[Vertex], name: &str) {
    let min = vertices.iter().fold([f32::INFINITY; 2], |p, v| {
        [p[0].min(v.point.x), p[1].min(v.point.y)]
    });
    let max = vertices.iter().fold([f32::NEG_INFINITY; 2], |p, v| {
        [p[0].max(v.point.x), p[1].max(v.point.y)]
    });
    let road_height = vertices
        .iter()
        .map(|v| v.height)
        .fold(f32::INFINITY, f32::min);
    // Include every terrain pixel under each compiled band, catching peaks
    // between tessellation rows and between the road's lateral boundaries.
    let surface_height = terrain.surface_maximum_in(min, max).unwrap();
    assert!(
        road_height + 0.0001 >= surface_height,
        "{name} pavement {road_height} intersects surface {surface_height} in {min:?}..{max:?}"
    );
    for vertex in vertices {
        assert!(terrain
            .height_at([vertex.point.x, vertex.point.y])
            .is_some());
    }
    if !name.contains("_bridge") {
        // Sample the complete band interior, rather than its centerline only.
        for along in 0..=4 {
            let s = along as f32 / 4.0;
            for i in 0..=8 {
                let t = i as f32 / 8.0;
                let row_point = |a: Vertex, b: Vertex| {
                    [
                        a.point.x + (b.point.x - a.point.x) * t,
                        a.point.y + (b.point.y - a.point.y) * t,
                    ]
                };
                let start = row_point(vertices[0], vertices[1]);
                let end = row_point(vertices[2], vertices[3]);
                let point = [
                    start[0] + (end[0] - start[0]) * s,
                    start[1] + (end[1] - start[1]) * s,
                ];
                assert!(
                    terrain.height_at(point).unwrap() >= terrain.spec.water_level.unwrap(),
                    "{name} leaves dry land at {point:?}; water crossings must be explicit bridges"
                );
            }
        }
    }
}

#[test]
fn default_map_full_road_surfaces_clear_terrain_and_water() {
    let (program, projection, terrain) = scene();
    assert!(
        opencity::project_landscape_features(&program)
            .features
            .iter()
            .all(|feature| matches!(
                feature.kind,
                LandscapeKind::Building | LandscapeKind::Concrete | LandscapeKind::Vegetation
            )),
        "the bake recipe must stay unplaced"
    );
    let mut recipe = program.clone();
    recipe
        .statements
        .extend(opencity::parse("terrain_source(0, 0);").unwrap().statements);
    let landscape = opencity::project_landscape_features(&recipe);
    assert!(landscape.errors.is_empty(), "{:?}", landscape.errors);
    for (kind, name) in [
        (LandscapeKind::Land, "west_island"),
        (LandscapeKind::Land, "east_island"),
        (LandscapeKind::Land, "south_island"),
        (LandscapeKind::Lake, "west_lake"),
        (LandscapeKind::Lake, "east_lake"),
    ] {
        assert!(
            landscape
                .features
                .iter()
                .any(|feature| feature.kind == kind && feature.name.as_deref() == Some(name)),
            "missing terrain recipe feature {name}"
        );
    }
    for point in [[0.0, 6.0], [-12.0, -9.0], [-17.4, 3.9], [26.1, 15.3]] {
        assert!(
            terrain.height_at(point).unwrap() < terrain.spec.water_level.unwrap(),
            "straits and lakes must remain open at {point:?}"
        );
    }
    let geometry = RoadGeometry::compile(&program, &projection);
    for (index, segment) in projection.segments.iter().enumerate() {
        let name = segment.name.as_deref().expect("default roads are named");
        let rows = geometry.cross_sections(index);
        assert!(rows.len() >= 2, "{name} has no pavement");
        // These authoring budgets keep the default scene's bridges and ramps
        // low and gradual when viewed in isometric projection.
        assert!(
            segment.start_z.max(segment.end_z) <= 0.35 + 0.0001,
            "{name} exceeds the default map's 0.35 height budget: {}..{}",
            segment.start_z,
            segment.end_z
        );
        for pair in rows.windows(2) {
            let start = segment.point_at(pair[0].t);
            let end = segment.point_at(pair[1].t);
            let distance = (end.x - start.x).hypot(end.y - start.y);
            let rise = (segment.height_at(pair[1].t) - segment.height_at(pair[0].t)).abs();
            assert!(
                rise <= 0.20 * distance + 0.000001,
                "{name} exceeds the default map's 20% grade budget at t={}..{}: rise {rise}, distance {distance}",
                pair[0].t,
                pair[1].t
            );
            assert_eq!(pair[0].boundaries.len(), pair[1].boundaries.len());
            for band in 0..pair[0].boundaries.len() - 1 {
                assert_surface_clear(
                    &terrain,
                    &[
                        pair[0].boundaries[band],
                        pair[0].boundaries[band + 1],
                        pair[1].boundaries[band],
                        pair[1].boundaries[band + 1],
                    ],
                    name,
                );
            }
        }
    }
}

#[test]
fn default_map_roads_are_connected_without_attachment_height_steps() {
    let (program, projection, _) = scene();
    assert!(!projection.segments.is_empty());
    let mut at_point: HashMap<String, Vec<(usize, f32)>> = HashMap::new();
    let mut edges = vec![Vec::new(); projection.segments.len()];
    for (index, road) in projection.segments.iter().enumerate() {
        for (point, height) in [(&road.start_ref, road.start_z), (&road.end_ref, road.end_z)] {
            at_point
                .entry(point.clone().expect("default roads use explicit endpoints"))
                .or_default()
                .push((index, height));
        }
    }
    // A placed junction's ports are connected through its center even when
    // no road terminates at that center reference.
    for intersection in map::intersections(&program, &projection) {
        let mut arms = at_point
            .get(&intersection.point_ref)
            .cloned()
            .unwrap_or_default();
        for port in &intersection.ports {
            arms.extend(at_point.get(&port.point_ref).into_iter().flatten().copied());
        }
        at_point.insert(intersection.point_ref, arms);
    }
    for (point, ends) in &at_point {
        for &(a, height_a) in ends {
            for &(b, height_b) in ends {
                assert!(
                    (height_a - height_b).abs() < 0.0001,
                    "height step at {point}: {} ({height_a}) / {} ({height_b})",
                    projection.segments[a].name.as_deref().unwrap(),
                    projection.segments[b].name.as_deref().unwrap()
                );
                edges[a].push(b);
            }
        }
    }
    for statement in &program.statements {
        let Statement::Call(call) = statement else {
            continue;
        };
        if call.name != "lane_point" {
            continue;
        }
        let point = call.assigns.first().unwrap();
        let (parent, _, t) = map::lane_attachment(&program, point).unwrap();
        let parent_index = projection
            .segments
            .iter()
            .position(|road| road.name.as_deref() == Some(parent.as_str()))
            .unwrap();
        let parent_height = projection.segments[parent_index].height_at(t);
        for &(attached, height) in at_point.get(point).expect("lane point is used") {
            assert!(
                (height - parent_height).abs() < 0.0001,
                "height step at lane attachment {point}"
            );
            edges[parent_index].push(attached);
            edges[attached].push(parent_index);
        }
    }
    let mut reached = vec![false; edges.len()];
    let mut pending = vec![0];
    while let Some(index) = pending.pop() {
        if reached[index] {
            continue;
        }
        reached[index] = true;
        pending.extend(edges[index].iter().copied());
    }
    let disconnected: Vec<_> = projection
        .segments
        .iter()
        .zip(reached)
        .filter(|(_, reached)| !reached)
        .map(|(road, _)| road.name.as_deref().unwrap())
        .collect();
    assert!(
        disconnected.is_empty(),
        "disconnected roads: {disconnected:?}"
    );
}
