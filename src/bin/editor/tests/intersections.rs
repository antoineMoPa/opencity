use super::*;

#[test]
fn starter_grid_intersection_is_square_with_four_cross_section_ports() {
    let program = starter_program();
    let projection = map::project(&program);
    let intersection = map::intersections(&program, &projection)
        .into_iter()
        .find(|intersection| intersection.point_ref == "grid_1_1")
        .unwrap();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1000.0));
    let paint = intersection_paint(
        &program,
        &projection,
        &intersection,
        rect,
        egui::Vec2::ZERO,
        100.0,
    )
    .unwrap();
    let edge = program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Road(road) if road.name == intersection.road_name => Some(road),
            _ => None,
        })
        .unwrap();
    let edge_width = edge
        .elements
        .iter()
        .map(|element| map::element_world_width(element.kind))
        .sum::<f32>()
        * 100.0;
    assert!((paint.square.width() - paint.square.height()).abs() < 0.001);
    assert!((paint.square.width() - edge_width).abs() < 0.001);
    assert_eq!(paint.closed, RoadElementKind::Sidewalk);
    assert_eq!(paint.ports.len(), 4);
    for side in PortSide::ALL {
        assert_eq!(
            paint.ports.iter().filter(|port| port.side == side).count(),
            1
        );
    }
    assert!(paint.ports.iter().all(|port| port
        .bands
        .iter()
        .any(|(kind, _)| *kind == RoadElementKind::Sidewalk)));
    for port in &paint.ports {
        assert_eq!(port.bands.len(), edge.elements.len());
        for (index, (kind, band)) in port.bands.iter().enumerate() {
            assert_eq!(*kind, edge.elements[index].kind);
            assert!(paint.square.contains_rect(*band));
        }
    }
    let north = paint
        .ports
        .iter()
        .find(|port| port.side == PortSide::North)
        .unwrap();
    for right in [false, true] {
        let corner_x = if right {
            paint.square.right()
        } else {
            paint.square.left()
        };
        let disks = corner_disks(north, corner_x, paint.square.center().x, right);
        assert_eq!(
            disks.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
            vec![RoadElementKind::Sidewalk, RoadElementKind::ParkingLane]
        );
        let expected = 100.0
            * (map::element_world_width(RoadElementKind::Sidewalk)
                + map::element_world_width(RoadElementKind::ParkingLane));
        assert!((disks.last().unwrap().1 - expected).abs() < 0.001);
    }
    let zoomed = intersection_paint(
        &program,
        &projection,
        &intersection,
        rect,
        egui::Vec2::ZERO,
        200.0,
    )
    .unwrap();
    assert!((zoomed.square.width() / paint.square.width() - 2.0).abs() < 0.001);
}

#[test]
fn three_way_intersection_has_one_closed_side() {
    let program = starter_program();
    let projection = map::project(&program);
    let intersection = map::intersections(&program, &projection)
        .into_iter()
        .find(|intersection| intersection.point_ref == "grid_0_1")
        .unwrap();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1000.0));
    let paint = intersection_paint(
        &program,
        &projection,
        &intersection,
        rect,
        egui::Vec2::ZERO,
        100.0,
    )
    .unwrap();
    assert_eq!(paint.ports.len(), 3);
    assert_eq!(
        PortSide::ALL
            .iter()
            .filter(|side| !paint.ports.iter().any(|port| port.side == **side))
            .count(),
        1
    );
}

#[test]
fn mixed_intersection_paints_narrow_local_arm_without_design_mismatch() {
    let program = parse("\
        road arterial: sidewalk-carlane-carlane-sidewalk; end; \
        road local: sidewalk-carlane-sidewalk; end; \
        center = map_point(0, 0); west = map_point(-1, 0); east = map_point(1, 0); north = map_point(0, 1); \
        arterial(west, -0.7, 0, -0.3, 0, center); \
        arterial(center, 0.3, 0, 0.7, 0, east); \
        local(center, 0, 0.3, 0, 0.7, north); \
        intersection3(center, arterial, north, local);").unwrap();
    let projection = map::project(&program);
    let junction = map::intersections(&program, &projection).pop().unwrap();
    let profiles = paint_profiles(&program, &projection);
    assert!(design_rule_issues(&program, &projection, &profiles).is_empty());
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1000.0));
    let paint = intersection_paint(
        &program,
        &projection,
        &junction,
        rect,
        egui::Vec2::ZERO,
        100.0,
    )
    .unwrap();
    assert_eq!(paint.ports.len(), 3);
    assert!(paint.mixed);
    let north = paint
        .ports
        .iter()
        .find(|port| port.side == PortSide::North)
        .unwrap();
    let east = paint
        .ports
        .iter()
        .find(|port| port.side == PortSide::East)
        .unwrap();
    let north_width: f32 = north.bands.iter().map(|(_, band)| band.width()).sum();
    let east_width: f32 = east.bands.iter().map(|(_, band)| band.height()).sum();
    assert!(north_width < east_width);
}

#[test]
fn right_turn_uses_adjacent_square_ports() {
    let program = parse(
        "road lane: sidewalk-carlane-sidewalk; end; \
        center = map_point(0, 0); west = map_point(-1, 0); north = map_point(0, 1); \
        a = lane(west, -0.7, 0, -0.3, 0, center); \
        b = lane(center, 0, 0.3, 0, 0.7, north); \
        intersection2(center, lane);",
    )
    .unwrap();
    let projection = map::project(&program);
    let intersection = &map::intersections(&program, &projection)[0];
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1000.0));
    let paint = intersection_paint(
        &program,
        &projection,
        intersection,
        rect,
        egui::Vec2::ZERO,
        100.0,
    )
    .unwrap();
    assert_eq!(paint.ports.len(), 2);
    assert!(paint.ports.iter().any(|port| port.side == PortSide::West));
    assert!(paint.ports.iter().any(|port| port.side == PortSide::North));
    assert_eq!(paint.closed, RoadElementKind::Sidewalk);
    let expected = 100.0
        * (2.0 * map::element_world_width(RoadElementKind::Sidewalk)
            + map::element_world_width(RoadElementKind::CarLane));
    assert!((paint.square.width() - expected).abs() < 0.001);
}

#[test]
fn placed_intersection_ports_are_pickable_without_any_roads() {
    let mut program = default_program();
    let road = program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Road(road) => Some(road.clone()),
            _ => None,
        })
        .unwrap();
    let center_ref = map::place_intersection(
        &mut program,
        map::IntersectionKind::FourWay,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
    );
    let projection = map::project(&program);
    let intersection = map::intersections(&program, &projection)
        .into_iter()
        .find(|intersection| intersection.point_ref == center_ref)
        .unwrap();
    assert_eq!(intersection.ports.len(), 4);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1000.0));
    for port in &intersection.ports {
        let screen = screen_of(rect, egui::Vec2::ZERO, 420.0, port.position);
        let picked =
            intersection_port_near(&program, &projection, rect, egui::Vec2::ZERO, 420.0, screen)
                .unwrap();
        assert_eq!(picked.1, port.point_ref);
        assert_eq!(picked.2, road.name);
    }
}

#[test]
fn port_bands_meet_on_diagonals_without_overlapping_at_corners() {
    let center = egui::pos2(50.0, 50.0);
    let band = egui::Rect::from_min_max(egui::pos2(50.0, 50.0), egui::pos2(100.0, 100.0));
    for side in [PortSide::East, PortSide::South] {
        let polygon = clipped_port_band(band, side, center);
        assert!(polygon.len() >= 3);
        for point in polygon {
            for (x, y) in side.wedge_planes() {
                assert!(x * (point.x - center.x) + y * (point.y - center.y) >= -0.001);
            }
        }
    }
}
