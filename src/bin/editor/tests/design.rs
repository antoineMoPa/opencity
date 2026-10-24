use super::*;

#[test]
fn design_check_finds_a_straight_misaligned_join() {
    let program = parse("road narrow: carlane; end; road wide: verge-carlane-verge; end; p0 = map_point(0, 0); seam = map_point(1, 0); p1 = map_point(2, 0); a = narrow(p0, 0.3, 0, 0.7, 0, seam); b = wide(seam, 1.3, 0, 1.7, 0, p1);").unwrap();
    let projection = map::project(&program);
    let mut profiles = paint_profiles(&program, &projection);
    assert!(design_rule_issues(&program, &projection, &profiles).is_empty());
    profiles[1].start_taper.clear();
    let issues = design_rule_issues(&program, &projection, &profiles);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].code, "road_edge_mismatch");
}

#[test]
fn design_check_reports_a_ramp_that_nearly_meets_a_road() {
    let program = parse("road ramp: verge-carlane-verge;
role ramp; end; road street: carlane; end; a = map_point(0, 0); b = map_point(1, 0); near = map_point(1.02, 0); c = map_point(2, 0); exit = ramp(a, 0.3, 0, 0.7, 0, b); street(near, 1.3, 0, 1.7, 0, c);").unwrap();
    let projection = map::project(&program);
    let issues = design_rule_issues(
        &program,
        &projection,
        &paint_profiles(&program, &projection),
    );
    assert!(issues
        .iter()
        .any(|issue| issue.code == "unconnected_ramp_end"
            && issue.at == WorldPoint { x: 1.0, y: 0.0 }));
}

#[test]
fn design_check_requires_a_junction_or_height_for_a_ramp_crossing() {
    let source = "road ramp: verge-carlane-verge;
role ramp; end; road street: carlane; end; a = map_point(-2, 0); b = map_point(2, 0); c = map_point(0, -2); d = map_point(0, 2); exit = ramp(a, -0.7, 0, 0.7, 0, b); street(c, 0, -0.7, 0, 0.7, d);";
    let at_grade = parse(source).unwrap();
    let projection = map::project(&at_grade);
    let issues = design_rule_issues(
        &at_grade,
        &projection,
        &paint_profiles(&at_grade, &projection),
    );
    assert!(issues
        .iter()
        .any(|issue| issue.code == "ramp_at_grade_crossing"));

    let bridged = parse(&format!("{source} elevation(exit, 0.2);")).unwrap();
    let projection = map::project(&bridged);
    let issues = design_rule_issues(
        &bridged,
        &projection,
        &paint_profiles(&bridged, &projection),
    );
    assert!(!issues
        .iter()
        .any(|issue| issue.code == "ramp_at_grade_crossing"));
}

#[test]
fn starter_city_defines_street_intersections_without_filling_the_cloverleaf() {
    let program = starter_program();
    let projection = map::project(&program);
    assert_eq!(projection.unresolved, 0);
    assert_eq!(projection.segments.len(), 84);
    let landscape = project_landscape_features(&program);
    assert_eq!(landscape.features.len(), 3);
    assert!(landscape.errors.is_empty());
    assert!(projection.segments.iter().any(|segment| {
        segment.name.as_deref() == Some("interstate_north_south_03")
            && segment.start_z == 0.24
            && segment.end_z == 0.24
    }));
    let intersections = map::intersections(&program, &projection);
    assert_eq!(intersections.len(), 12);
    for intersection in &intersections {
        assert_eq!(intersection.ports.len(), intersection.kind.arm_count());
        assert!(map::intersection_arms(&projection, &intersection.point_ref).is_empty());
        for port in &intersection.ports {
            assert!(projection
                .segments
                .iter()
                .any(
                    |segment| segment.start_ref.as_deref() == Some(&port.point_ref)
                        || segment.end_ref.as_deref() == Some(&port.point_ref)
                ));
        }
    }
    assert!(existing_design_checks_are_clear(&design_rule_issues(
        &program,
        &projection,
        &paint_profiles(&program, &projection)
    )));
    assert_eq!(
        intersections
            .iter()
            .filter(|intersection| intersection.kind == map::IntersectionKind::ThreeWay)
            .count(),
        5
    );
    assert_eq!(
        intersections
            .iter()
            .filter(|intersection| intersection.kind == map::IntersectionKind::RightTurn)
            .count(),
        3
    );
    assert_eq!(
        intersections
            .iter()
            .filter(|intersection| intersection.kind == map::IntersectionKind::FourWay)
            .count(),
        4
    );
    for reference in ["ew_3", "ew_5", "ns_2", "ns_4"] {
        assert_eq!(map::intersection_arms(&projection, reference).len(), 2);
        assert!(!intersections
            .iter()
            .any(|intersection| intersection.point_ref == reference));
    }
    for (reference, highway, lane, t) in [
        ("nw_exit", "interstate_east_west_04", 1, 0.5),
        ("sw_merge", "interstate_east_west_04", 7, 0.5),
        ("ne_merge", "interstate_east_west_05", 1, 0.5),
        ("se_exit", "interstate_east_west_05", 7, 0.5),
        ("nw_merge", "interstate_north_south_04", 1, 0.5),
        ("ne_exit", "interstate_north_south_04", 7, 0.5),
        ("sw_exit", "interstate_north_south_03", 1, 0.5),
        ("se_merge", "interstate_north_south_03", 7, 0.5),
        ("nw_direct_exit", "interstate_north_south_06", 1, 0.756098),
        ("nw_direct_merge", "interstate_east_west_02", 1, 0.208333),
        ("ne_direct_exit", "interstate_east_west_07", 1, 0.696970),
        ("ne_direct_merge", "interstate_north_south_06", 7, 0.756098),
        ("sw_direct_exit", "interstate_east_west_02", 7, 0.208333),
        ("sw_direct_merge", "interstate_north_south_01", 1, 0.243902),
        ("se_direct_exit", "interstate_north_south_01", 7, 0.243902),
        ("se_direct_merge", "interstate_east_west_07", 7, 0.696970),
    ] {
        assert_eq!(
            map::lane_attachment(&program, reference),
            Some((highway.to_owned(), lane, t))
        );
    }
    for (name, highway_at_start, horizontal) in [
        ("nw_loop_a_mouth", true, true),
        ("nw_loop_c_merge", false, false),
        ("sw_loop_a_mouth", true, false),
        ("sw_loop_c_merge", false, true),
        ("ne_loop_a_mouth", true, false),
        ("ne_loop_c_merge", false, true),
        ("se_loop_a_mouth", true, true),
        ("se_loop_c_merge", false, false),
        ("nw_connector_mouth", true, false),
        ("nw_connector_merge", false, true),
        ("ne_connector_mouth", true, true),
        ("ne_connector_merge", false, false),
        ("sw_connector_mouth", true, true),
        ("sw_connector_merge", false, false),
        ("se_connector_mouth", true, false),
        ("se_connector_merge", false, true),
    ] {
        let mouth = projection
            .segments
            .iter()
            .find(|segment| segment.name.as_deref() == Some(name))
            .unwrap();
        let tangent = mouth.tangent_at(if highway_at_start { 0.0 } else { 1.0 });
        if horizontal {
            assert!(tangent.y.abs() < 0.001, "{name} bends at the highway");
            assert!(
                (mouth.end.x - mouth.start.x).abs() > 0.70,
                "{name} is too short"
            );
        } else {
            assert!(tangent.x.abs() < 0.001, "{name} bends at the highway");
            assert!(
                (mouth.end.y - mouth.start.y).abs() > 0.70,
                "{name} is too short"
            );
        }
    }
    for quadrant in ["nw", "ne", "sw", "se"] {
        let loop_prefix = format!("{quadrant}_loop_");
        let connector_prefix = format!("{quadrant}_connector");
        for loop_segment in projection.segments.iter().filter(|segment| {
            segment
                .name
                .as_deref()
                .is_some_and(|name| name.starts_with(&loop_prefix))
        }) {
            for connector in projection.segments.iter().filter(|segment| {
                segment
                    .name
                    .as_deref()
                    .is_some_and(|name| name.starts_with(&connector_prefix))
            }) {
                for loop_step in 0..=100 {
                    let a = loop_segment.point_at(loop_step as f32 / 100.0);
                    for connector_step in 0..=100 {
                        let b = connector.point_at(connector_step as f32 / 100.0);
                        let separation_squared = (a.x - b.x).powi(2) + (a.y - b.y).powi(2);
                        assert!(
                            separation_squared > 0.22_f32.powi(2),
                            "{quadrant} connector crosses its loop"
                        );
                    }
                }
            }
        }
    }
    assert_eq!(map::intersection_arms(&projection, "ns_3").len(), 2);
    assert_eq!(map::intersection_arms(&projection, "ew_4").len(), 2);
    assert!(!intersections
        .iter()
        .any(|intersection| intersection.point_ref == "ew_2"));
    for reference in ["grid_0_0", "grid_0_3", "grid_2_0"] {
        assert!(intersections
            .iter()
            .any(|intersection| intersection.point_ref == reference
                && intersection.kind == map::IntersectionKind::RightTurn));
    }
    let country_edge = projection
        .segments
        .iter()
        .find(|segment| segment.name.as_deref() == Some("country_road_1_edge"))
        .unwrap();
    let tangent = country_edge.tangent_at(0.0);
    assert!(tangent.x.abs() < 0.001);
    let profiles = paint_profiles(&program, &projection);
    assert!(existing_design_checks_are_clear(&design_rule_issues(
        &program,
        &projection,
        &profiles
    )));
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn design_check_reports_an_approach_that_exceeds_the_intersection_edge() {
    let program = parse(
        "road narrow: sidewalk-carlane-sidewalk; end; \
        road wide: sidewalk-carlane-carlane-sidewalk; end; \
        center = map_point(0, 0); west = map_point(-1, 0); \
        east = map_point(1, 0); north = map_point(0, 1); \
        a = wide(west, -0.7, 0, -0.3, 0, center); \
        b = narrow(center, 0.3, 0, 0.7, 0, east); \
        c = narrow(center, 0, 0.3, 0, 0.7, north); \
        intersection3(center, narrow);",
    )
    .unwrap();
    let projection = map::project(&program);
    let profiles = paint_profiles(&program, &projection);
    let issues = design_rule_issues(&program, &projection, &profiles);
    assert!(issues.iter().any(|issue| {
        issue.code == "intersection_edge_mismatch" && issue.roads == ["a", "narrow"]
    }));
}

#[test]
fn exit_carries_sidewalk_without_shrinking_any_band() {
    let mut program = parse("road boulevard: middle-carlane-carlane-sidewalk-verge; end;").unwrap();
    let road = match &program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let parent = map::append(
        &mut program,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
        WorldPoint { x: 4.0, y: 0.0 },
    );
    let (attachment, start) =
        map::create_lane_point(&mut program, SegmentSource::TopLevel(parent), 2, 0.5).unwrap();
    let exit = map::append_exit(
        &mut program,
        &road,
        map::SplitMode::Dedicated,
        start,
        &attachment,
        WorldPoint { x: 5.0, y: -1.5 },
        None,
    )
    .unwrap();
    assert!(map::create_two_segment_split(
        &mut program,
        &attachment,
        exit.entry,
        exit.full,
        map::SplitMode::Dedicated,
    ));
    let projection = map::project(&program);
    let profiles = paint_profiles(&program, &projection);
    let entry_index = projection
        .segments
        .iter()
        .position(|segment| segment.source == exit.entry)
        .unwrap();
    let through = projection
        .segments
        .iter()
        .find(|segment| Some(segment.source) == exit.through)
        .unwrap();
    let entry_road = road_for_segment(&program, &projection.segments[entry_index]).unwrap();
    let through_road = road_for_segment(&program, through).unwrap();
    assert_eq!(entry_road.elements, road.elements[2..]);
    assert_eq!(
        through_road
            .elements
            .iter()
            .map(|element| element.kind)
            .collect::<Vec<_>>(),
        vec![
            RoadElementKind::Middle,
            RoadElementKind::CarLane,
            RoadElementKind::Barrier
        ]
    );
    let (start_widths, _) = section_widths(entry_road, 100.0, &profiles[entry_index], 0.0);
    let (end_widths, _) = section_widths(entry_road, 100.0, &profiles[entry_index], 1.0);
    assert_eq!(start_widths, end_widths);
    assert!(start_widths.iter().all(|width| *width > 0.0));
    let parent_segment = projection
        .segments
        .iter()
        .find(|segment| segment.source == SegmentSource::TopLevel(parent))
        .unwrap();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    let zoom = 100.0;
    let parent_normal = screen_normal_at(parent_segment, 1.0);
    let parent_center = screen_of(rect, egui::Vec2::ZERO, zoom, parent_segment.end);
    let parent_widths: Vec<_> = road
        .elements
        .iter()
        .map(|element| map_element_width(element.kind, zoom))
        .collect();
    let parent_left = parent_center - parent_normal * (parent_widths.iter().sum::<f32>() / 2.0);
    let entry_segment = &projection.segments[entry_index];
    let entry_center = screen_of(rect, egui::Vec2::ZERO, zoom, entry_segment.start);
    let (_, entry_offset) = section_widths(entry_road, zoom, &profiles[entry_index], 0.0);
    let entry_left = entry_center + screen_normal_at(entry_segment, 0.0) * entry_offset;
    let expected_entry_left = parent_left + parent_normal * parent_widths[..2].iter().sum::<f32>();
    assert!(entry_left.distance(expected_entry_left) < 0.01);
    let through_center = screen_of(rect, egui::Vec2::ZERO, zoom, through.start);
    let through_width: f32 = through_road
        .elements
        .iter()
        .map(|element| map_element_width(element.kind, zoom))
        .sum();
    let through_left = through_center - screen_normal_at(through, 0.0) * (through_width / 2.0);
    assert!(through_left.distance(parent_left) < 0.01);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}
