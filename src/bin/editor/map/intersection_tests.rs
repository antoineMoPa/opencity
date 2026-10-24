use super::*;
use opencity::parse;

#[test]
fn mixed_intersection_maps_each_arm_to_its_road_profile() {
    let program = parse("\
        road arterial: sidewalk-carlane-carlane-sidewalk; end; \
        road local: sidewalk-carlane-sidewalk; end; \
        center = map_point(0, 0); west = map_point(-1, 0); east = map_point(1, 0); north = map_point(0, 1); \
        arterial(west, -0.7, 0, -0.3, 0, center); \
        arterial(center, 0.3, 0, 0.7, 0, east); \
        local(center, 0, 0.3, 0, 0.7, north); \
        intersection3(center, arterial, north, local);").unwrap();
    let projection = project(&program);
    assert_eq!(projection.unresolved, 0);
    let junction = intersections(&program, &projection).pop().unwrap();
    assert_eq!(
        junction.road_name_for_direction(WorldPoint { x: 0.0, y: 1.0 }),
        "local"
    );
    assert_eq!(
        junction.road_name_for_direction(WorldPoint { x: 1.0, y: 0.0 }),
        "arterial"
    );
    for (index, is_end) in intersection_arms(&projection, "center") {
        let segment = &projection.segments[index];
        assert_eq!(
            junction.road_name_for_segment(segment, is_end),
            segment.road_name
        );
    }
}

#[test]
fn standalone_intersection_has_edge_ports_and_roads_stop_at_them() {
    let mut program = parse("road street: sidewalk-carlane-sidewalk; end;").unwrap();
    let Statement::Road(road) = &program.statements[0] else {
        unreachable!()
    };
    let road = road.clone();
    let center = place_intersection(
        &mut program,
        IntersectionKind::FourWay,
        &road,
        WorldPoint { x: 2.0, y: 3.0 },
    );
    let junction = intersections(&program, &project(&program)).pop().unwrap();
    assert_eq!(junction.ports.len(), 4);
    assert_eq!(intersection_arms(&project(&program), &center).len(), 0);
    let east = &junction.ports[1];
    let half = road
        .elements
        .iter()
        .map(|element| element_world_width(element.kind))
        .sum::<f32>()
        / 2.0;
    assert!((east.position.x - (2.0 + half)).abs() < 0.0001);
    assert_eq!(east.position.y, 3.0);
    assert!(port_faces_road(
        &program,
        &east.point_ref,
        WorldPoint { x: 4.0, y: 3.0 }
    ));
    assert!(!port_faces_road(
        &program,
        &east.point_ref,
        WorldPoint { x: 2.0, y: 3.0 }
    ));
    let (index, _) = append_connected(
        &mut program,
        &road,
        east.position,
        Some(&east.point_ref),
        WorldPoint { x: 4.0, y: 3.0 },
        None,
    );
    let segment = project(&program)
        .segments
        .into_iter()
        .find(|segment| segment.source == SegmentSource::TopLevel(index))
        .unwrap();
    assert_eq!(segment.start_ref.as_deref(), Some(east.point_ref.as_str()));
    assert_eq!(segment.start, east.position);
    assert!(segment.controls[0].x > segment.start.x);
    assert!(!set_connected_endpoint(
        &mut program,
        SegmentSource::TopLevel(index),
        false,
        WorldPoint { x: 8.0, y: 3.0 }
    ));
    assert!(translate_intersection(
        &mut program,
        &center,
        WorldPoint { x: 1.0, y: -1.0 }
    ));
    let moved = project(&program)
        .segments
        .into_iter()
        .find(|segment| segment.source == SegmentSource::TopLevel(index))
        .unwrap();
    assert!((moved.start.x - (east.position.x + 1.0)).abs() < 0.0002);
    assert!((moved.controls[0].x - (segment.controls[0].x + 1.0)).abs() < 0.0002);
    let (approach_index, _) = append_connected(
        &mut program,
        &road,
        WorldPoint { x: 0.0, y: 2.0 },
        None,
        WorldPoint { x: 1.0, y: 2.0 },
        None,
    );
    let west_ref = intersections(&program, &project(&program))[0].ports[3]
        .point_ref
        .clone();
    assert!(attach_endpoint_to_port(
        &mut program,
        SegmentSource::TopLevel(approach_index),
        true,
        &west_ref
    ));
    let approach = project(&program)
        .segments
        .into_iter()
        .find(|segment| segment.source == SegmentSource::TopLevel(approach_index))
        .unwrap();
    assert_eq!(approach.end_ref.as_deref(), Some(west_ref.as_str()));
    assert!(approach.controls[1].x < approach.end.x);
    prune_unused_points(&mut program);
    assert_eq!(intersections(&program, &project(&program)).len(), 1);
    assert!(remove_placed_intersection(&mut program, &center));
    assert!(intersections(&program, &project(&program)).is_empty());
    assert_eq!(project(&program).segments.len(), 2);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn rotating_a_three_way_intersection_moves_its_ports() {
    let mut program = parse("road street: sidewalk-carlane-sidewalk; end;").unwrap();
    let Statement::Road(road) = &program.statements[0] else {
        unreachable!()
    };
    let road = road.clone();
    let center = place_intersection(
        &mut program,
        IntersectionKind::ThreeWay,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
    );
    let before = intersections(&program, &project(&program)).pop().unwrap();
    let north_ref = before.ports[0].point_ref.clone();
    assert_eq!(before.ports[0].direction, WorldPoint { x: 0.0, y: 1.0 });
    assert!(rotate_intersection(&mut program, &center));
    let after = intersections(&program, &project(&program)).pop().unwrap();
    let rotated = after
        .ports
        .iter()
        .find(|port| port.point_ref == north_ref)
        .unwrap();
    assert_eq!(rotated.direction, WorldPoint { x: 1.0, y: 0.0 });
    assert!(rotated.position.x > 0.0);
    assert_eq!(after.ports.len(), 3);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn three_and_four_way_intersections_require_matching_shared_road_ends() {
    let mut program = parse(
        "road street: sidewalk-carlane-sidewalk; end; \
        center = map_point(0, 0); west = map_point(-1, 0); east = map_point(1, 0); \
        north = map_point(0, 1); south = map_point(0, -1); \
        a = street(west, -0.7, 0, -0.3, 0, center); \
        b = street(center, 0.3, 0, 0.7, 0, east); \
        c = street(center, 0, 0.3, 0, 0.7, north);",
    )
    .unwrap();
    assert_eq!(intersection_arms(&project(&program), "center").len(), 3);
    assert!(set_intersection(
        &mut program,
        "center",
        Some(IntersectionKind::FourWay),
        Some("street")
    )
    .is_err());
    set_intersection(
        &mut program,
        "center",
        Some(IntersectionKind::ThreeWay),
        Some("street"),
    )
    .unwrap();
    assert_eq!(intersections(&program, &project(&program)).len(), 1);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    program.statements.push(Statement::Call(Call {
        assigns: vec!["d".into()],
        name: "street".into(),
        args: vec![
            Argument::Identifier("south".into()),
            Argument::Number("0".into()),
            Argument::Number("-0.7".into()),
            Argument::Number("0".into()),
            Argument::Number("-0.3".into()),
            Argument::Identifier("center".into()),
        ],
    }));
    assert_eq!(project(&program).unresolved, 1);
    set_intersection(
        &mut program,
        "center",
        Some(IntersectionKind::FourWay),
        Some("street"),
    )
    .unwrap();
    assert_eq!(project(&program).unresolved, 0);
    assert_eq!(
        intersections(&program, &project(&program))[0].kind,
        IntersectionKind::FourWay
    );
    set_intersection(&mut program, "center", None, None).unwrap();
    assert!(intersections(&program, &project(&program)).is_empty());
}

#[test]
fn right_turn_requires_two_perpendicular_arms() {
    let mut program = parse(
        "road lane: sidewalk-carlane-sidewalk; end; \
        center = map_point(0, 0); west = map_point(-1, 0); north = map_point(0, 1); \
        a = lane(west, -0.7, 0, -0.3, 0, center); \
        b = lane(center, 0, 0.3, 0, 0.7, north);",
    )
    .unwrap();
    set_intersection(
        &mut program,
        "center",
        Some(IntersectionKind::RightTurn),
        Some("lane"),
    )
    .unwrap();
    assert_eq!(
        intersections(&program, &project(&program))[0].kind,
        IntersectionKind::RightTurn
    );
    let Statement::Call(b) = program
        .statements
        .iter_mut()
        .find(|statement| {
            matches!(statement,
        Statement::Call(call) if call.assigns.first().is_some_and(|name| name == "b"))
        })
        .unwrap()
    else {
        unreachable!()
    };
    b.args[1] = Argument::Number("0.3".into());
    b.args[2] = Argument::Number("0".into());
    assert_eq!(project(&program).unresolved, 1);
}

#[test]
fn extending_and_joining_old_endpoints_creates_shared_junctions() {
    let mut program = parse("road street: verge-carlane-verge; end;").unwrap();
    let road = match &program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let first = append(
        &mut program,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
        WorldPoint { x: 2.0, y: 0.0 },
    );
    let second = append(
        &mut program,
        &road,
        WorldPoint { x: 4.0, y: 1.0 },
        WorldPoint { x: 6.0, y: 1.0 },
    );
    let (start, start_ref) =
        link_endpoint(&mut program, SegmentSource::TopLevel(first), true).unwrap();
    let (end, end_ref) =
        link_endpoint(&mut program, SegmentSource::TopLevel(second), false).unwrap();
    let (bridge, _) = append_connected(
        &mut program,
        &road,
        start,
        Some(&start_ref),
        end,
        Some(&end_ref),
    );
    let projection = project(&program);
    let first_segment = projection
        .segments
        .iter()
        .find(|s| s.source == SegmentSource::TopLevel(first))
        .unwrap();
    let bridge_segment = projection
        .segments
        .iter()
        .find(|s| s.source == SegmentSource::TopLevel(bridge))
        .unwrap();
    let second_segment = projection
        .segments
        .iter()
        .find(|s| s.source == SegmentSource::TopLevel(second))
        .unwrap();
    assert_eq!(first_segment.end_ref, bridge_segment.start_ref);
    assert_eq!(second_segment.start_ref, bridge_segment.end_ref);
    assert_eq!(bridge_segment.road_name, road.name);
    let first_tangent = WorldPoint {
        x: first_segment.end.x - first_segment.controls[1].x,
        y: first_segment.end.y - first_segment.controls[1].y,
    };
    let bridge_tangent = WorldPoint {
        x: bridge_segment.controls[0].x - bridge_segment.start.x,
        y: bridge_segment.controls[0].y - bridge_segment.start.y,
    };
    assert!(
        (first_tangent.x * bridge_tangent.y - first_tangent.y * bridge_tangent.x).abs() < 0.0001
    );
    assert!(set_connected_endpoint(
        &mut program,
        SegmentSource::TopLevel(first),
        true,
        WorldPoint { x: 2.0, y: 0.5 }
    ));
    let projection = project(&program);
    let bridge_segment = projection
        .segments
        .iter()
        .find(|s| s.source == SegmentSource::TopLevel(bridge))
        .unwrap();
    assert_eq!(bridge_segment.start, WorldPoint { x: 2.0, y: 0.5 });
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn rotating_an_endpoint_preserves_handle_length_and_connected_tangent() {
    let mut program = parse("road street: verge-carlane-verge; end;").unwrap();
    let road = match &program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let first = append(
        &mut program,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
        WorldPoint { x: 2.0, y: 0.0 },
    );
    let (start, reference) =
        link_endpoint(&mut program, SegmentSource::TopLevel(first), true).unwrap();
    let (second, _) = append_connected(
        &mut program,
        &road,
        start,
        Some(&reference),
        WorldPoint { x: 4.0, y: 0.0 },
        None,
    );
    assert!(rotate_endpoint(
        &mut program,
        SegmentSource::TopLevel(first),
        true,
        15.0
    ));
    let projection = project(&program);
    let first_segment = projection
        .segments
        .iter()
        .find(|s| s.source == SegmentSource::TopLevel(first))
        .unwrap();
    let second_segment = projection
        .segments
        .iter()
        .find(|s| s.source == SegmentSource::TopLevel(second))
        .unwrap();
    let incoming = WorldPoint {
        x: first_segment.end.x - first_segment.controls[1].x,
        y: first_segment.end.y - first_segment.controls[1].y,
    };
    let outgoing = WorldPoint {
        x: second_segment.controls[0].x - second_segment.start.x,
        y: second_segment.controls[0].y - second_segment.start.y,
    };
    assert!((incoming.x - outgoing.x).abs() < 0.0001);
    assert!((incoming.y - outgoing.y).abs() < 0.0001);
    assert!((incoming.x.powi(2) + incoming.y.powi(2) - (2.0 / 3.0_f32).powi(2)).abs() < 0.0001);
}
