use opencity::{map, network, parse, project_landscape_features};

const TEMPLATE: &str = r#"
intersection link({ length, width, direction, pavement }: {
    length: number, width: number, direction: identifier, pavement: identifier
}) {
    port exit.out at (length, 0, 2) heading direction width width;
    path length { move (0, 0); line to exit.out; }
    surface pavement along length width width material pavement;
}
"#;

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
}

#[test]
fn parameters_bind_value_positions_and_round_trip_without_mutating_templates() {
    let program = parse(&format!(
        "{TEMPLATE}\n        a = link(10, 20, 3, 90, {{ length: 4, width: \
            0.5, direction: east, pavement: asphalt }});\n        b = \
            link(-10, -20, {{ length: 8, width: 1, direction: west, pavement: \
            concrete }});\n    "
    ))
    .unwrap();
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    let projection = network::project(&program);
    assert!(projection.errors.is_empty(), "{:?}", projection.errors);
    assert_eq!(projection.instances.len(), 2);
    let a = &projection.instances[0];
    let port = &a.geometry.ports[0];
    assert_eq!(port.position.x, 4.);
    assert_eq!(port.width, Some(0.5));
    assert_eq!(a.geometry.paths[0].name, "length");
    assert_eq!(a.geometry.bands[0].name, "pavement");
    let point = a.world_point(port.position);
    close(point.x, 10.);
    close(point.y, 24.);
    close(point.z, 5.);
    let direction = a.world_direction(port.heading);
    close(direction[0], 0.);
    close(direction[1], 1.);
    assert_eq!(projection.instances[1].geometry.ports[0].position.x, 8.);
}

#[test]
fn nested_rotations_keep_roads_ports_landscape_and_heights_aligned() {
    let program = parse(&format!(
        "{TEMPLATE}\n        road street: carlane; end\n        block \
        leaf({{ size }}: {{ size: number }}):\n            junction = \
        link(1, 0, 2, -90, {{ length: size, width: 0.5, direction: east, \
        pavement: asphalt }});\n            approach = \
        street(junction.exit.out, 0, 1, 0, 3, junction.exit.out);\n            \
        line = street(0, 0, size, 0);\n            terrain = land(0, 0, \
        size, 0, size, 1);\n        end\n        block group({{ size }}: \
        {{ size: number }}):\n            leaf(2, 0, 1, 45, {{ size }});\
        \n        end\n        group(10, 20, 3, 45, {{ size: 4 }});\n    "
    ))
    .unwrap();

    let components = network::project(&program);
    assert!(components.errors.is_empty(), "{:?}", components.errors);
    let instance = &components.instances[0];
    close(instance.rotation, 0.);
    close(instance.origin.x, 10. + 2f32.sqrt());
    close(instance.origin.y, 21. + 2f32.sqrt());
    close(instance.origin.z, 6.);
    let roads = map::project(&program);
    assert_eq!(roads.unresolved, 0);
    assert_eq!(roads.segments.len(), 2);
    let port = instance.world_point(instance.geometry.ports[0].position);
    close(roads.segments[0].start.x, port.x);
    close(roads.segments[0].start.y, port.y);
    close(roads.segments[0].start_z, port.z);
    let landscape = project_landscape_features(&program);
    assert!(landscape.errors.is_empty());
    let line = &roads.segments[1];
    close(line.start.x, landscape.features[0].points[0][0]);
    close(line.start.y, landscape.features[0].points[0][1]);
    close(line.end.x, landscape.features[0].points[1][0]);
    close(line.end.y, landscape.features[0].points[1][1]);
    close(line.end.y - line.start.y, 4.);
    close(line.end.x - line.start.x, 0.);
}

#[test]
fn rotated_ports_expose_world_positions_and_headings() {
    let program = parse(
        r#"
        intersection link {
            port exit.out at (2, 0) heading east;
            path lane { move (0,0); line to exit.out; }
        }
        road street: carlane; end
        a = link(10, 20, 0, 90);
        b = link(10, 30, 0, -90);
        approach = street(a.exit.out, 10, 23, 10, 27, b.exit.out);
    "#,
    )
    .unwrap();
    let roads = map::project(&program);
    assert_eq!(roads.unresolved, 0);
    let road = &roads.segments[0];
    close(road.start.x, 10.);
    close(road.start.y, 22.);
    close(road.end.y, 28.);
    let direction = roads
        .points
        .iter()
        .find(|p| p.name == "a.exit.out")
        .unwrap()
        .direction
        .unwrap();
    close(direction.x, 0.);
    close(direction.y, 1.);
    assert!(road.controls[0].y > road.start.y);
    assert!(road.controls[1].y < road.end.y);
}

#[test]
fn invalid_instances_report_errors_and_leave_valid_placements_visible() {
    for args in [
        "0, 0",
        "0, 0, {}",
        "0, 0, { length: 4, width: 1, direction: east, pavement: asphalt, extra: 1 }",
        "0, 0, { length: bad, width: 1, direction: east, pavement: asphalt }",
        "0, 0, 0, bad, { length: 4, width: 1, direction: east, pavement: asphalt }",
        "0, 0, { length: 4, width: -1, direction: east, pavement: asphalt }",
        "0, 0, { length: 4, width: 1, direction: invalid, pavement: asphalt }",
        "0, 0, { length: 4, width: 1, direction: east, pavement: invalid }",
    ] {
        let source = format!(
            "{TEMPLATE} link({args}); link(0, 0, {{ length: 2, width: 1, \
                direction: east, pavement: asphalt }});"
        );
        let projection = network::project(&parse(&source).unwrap());
        assert_eq!(
            projection.errors.len(),
            1,
            "{args}: {:?}",
            projection.errors
        );
        assert_eq!(projection.instances.len(), 1, "{args}");
    }
}

#[test]
fn example_and_all_component_kinds_support_rotation_and_parameters() {
    let program = parse(include_str!("../examples/parameterized_intersections.road")).unwrap();
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    let projection = network::project(&program);
    assert!(projection.errors.is_empty(), "{:?}", projection.errors);
    assert_eq!(projection.instances.len(), 4);
    assert_eq!(project_landscape_features(&program).features.len(), 1);
    for kind in ["intersection", "interchange", "road_segment"] {
        let program = parse(&format!(
            r#"
            {kind} piece({{ length }}: {{ length: number }}) {{
                path p {{ move (0,0); cubic (1,0), (1,1) to (length,1); }}
                anchor a at p distance 1;
                surface s along p width 0.2;
            }}
            piece(0,0,0,30,{{ length: 2 }});
        "#
        ))
        .unwrap();
        assert!(network::project(&program).errors.is_empty(), "{kind}");
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    }
}

#[test]
fn block_rotation_moves_both_bezier_handles_and_building_footprints() {
    let program = parse(
        r#"
        road street: carlane; end
        block piece:
            curve = street(0,0, 1,0, 2,1, 2,2);
            home = building(0.5, 0,0, 2,0, 2,1, 0,1);
        end
        piece(10,20,3,-90);
    "#,
    )
    .unwrap();
    let roads = map::project(&program);
    assert_eq!(roads.unresolved, 0);
    let curve = &roads.segments[0];
    close(curve.start.x, 10.);
    close(curve.start.y, 20.);
    close(curve.controls[0].x, 10.);
    close(curve.controls[0].y, 19.);
    close(curve.controls[1].x, 11.);
    close(curve.controls[1].y, 18.);
    close(curve.end.x, 12.);
    close(curve.end.y, 18.);
    let features = project_landscape_features(&program);
    assert!(features.errors.is_empty());
    let home = &features.features[0];
    close(home.points[2][0], 11.);
    close(home.points[2][1], 18.);
    assert_eq!(home.elevation, 3.);
    assert_eq!(home.height, Some(0.5));
}

#[cfg(feature = "game-runtime")]
#[test]
fn runtime_support_queries_use_rotated_parameterized_surface_geometry() {
    use opencity::runtime::{City, SurfaceKind};
    let program = parse(
        r#"
        intersection node({ length }: { length: number }) {
            path p { move (0,0); line to (length,0); }
            surface s along p width 0.2 material asphalt;
        }
        node(10,20,3,90,{ length: 4 });
    "#,
    )
    .unwrap();
    let city = City::from_program(&program, None).unwrap();
    let support = city.surface_at([10., 22.], 4.).unwrap();
    assert_eq!(support.kind, SurfaceKind::Asphalt);
    close(support.height, 3.);
    assert!(city.surface_at([12., 20.], 4.).is_none());
}
