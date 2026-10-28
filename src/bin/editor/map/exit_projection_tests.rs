use super::*;
use opencity::parse;

#[test]
fn exits_carry_selected_elements_and_leave_a_divided_through_road() {
    for (element_index, target_y, side) in [(3, -2.0, ExitSide::Right), (1, 2.0, ExitSide::Left)] {
        let mut program = parse(
            "road highway: verge-carlane-middle-carlane-verge; end; road ramp: verge-carlane-verge;
role ramp; end;",
        )
        .unwrap();
        let highway = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let ramp = match &program.statements[1] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let parent = append(
            &mut program,
            &highway,
            WorldPoint { x: 0.0, y: 0.0 },
            WorldPoint { x: 4.0, y: 0.0 },
        );
        let (attachment, start) = create_lane_point(
            &mut program,
            SegmentSource::TopLevel(parent),
            element_index,
            0.5,
        )
        .unwrap();
        let exit = append_exit(
            &mut program,
            &ramp,
            SplitMode::Dedicated,
            start,
            &attachment,
            WorldPoint {
                x: 6.0,
                y: target_y,
            },
            None,
        )
        .unwrap();
        assert_eq!(exit.side, side);
        let projection = project(&program);
        let entry = projection
            .segments
            .iter()
            .find(|segment| segment.source == exit.entry)
            .unwrap();
        let travel_x = match highway.lane_direction(element_index) {
            Some(RoadDirection::Reverse) => -1.0,
            Some(RoadDirection::Forward) => 1.0,
            other => panic!("expected a directional lane, got {other:?}"),
        };
        assert!((entry.controls[0].x - entry.start.x) * travel_x > 0.0);
        assert!((entry.controls[0].y - entry.start.y).abs() < 0.0001);
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
        let full = projection
            .segments
            .iter()
            .find(|segment| segment.source == exit.full)
            .unwrap();
        let entry_road = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == entry.road_name => Some(road),
                _ => None,
            })
            .unwrap();
        assert_eq!(full.road_name, entry.road_name);
        match side {
            ExitSide::Right => assert_eq!(entry_road.elements, highway.elements[3..]),
            ExitSide::Left => assert_eq!(entry_road.elements, highway.elements[..=1]),
        }
        let through = projection
            .segments
            .iter()
            .find(|segment| Some(segment.source) == exit.through)
            .unwrap();
        let through_road = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == through.road_name => Some(road),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            through_road
                .elements
                .iter()
                .filter(|element| element.kind == RoadElementKind::Barrier)
                .count(),
            1
        );
        assert_eq!(
            entry_road.elements.len() + through_road.elements.len(),
            highway.elements.len() + 1
        );
        assert_eq!(
            projection
                .segments
                .iter()
                .find(|segment| segment.source == SegmentSource::TopLevel(parent))
                .unwrap()
                .end,
            WorldPoint { x: 2.0, y: 0.0 }
        );
        assert_eq!(entry.end_ref, full.start_ref);
        assert_eq!(entry.end, full.start);
        let incoming = WorldPoint {
            x: entry.end.x - entry.controls[1].x,
            y: entry.end.y - entry.controls[1].y,
        };
        let outgoing = WorldPoint {
            x: full.controls[0].x - full.start.x,
            y: full.controls[0].y - full.start.y,
        };
        let cross = incoming.x * outgoing.y - incoming.y * outgoing.x;
        let handle_product = ((incoming.x.powi(2) + incoming.y.powi(2))
            * (outgoing.x.powi(2) + outgoing.y.powi(2)))
        .sqrt();
        // Stored controls have four decimal places of coordinate precision.
        assert!(cross.abs() < handle_product * 0.0002);
        assert!(incoming.x * outgoing.x + incoming.y * outgoing.y > 0.0);
        assert!(create_two_segment_split(
            &mut program,
            &attachment,
            exit.entry,
            exit.full,
            SplitMode::Dedicated
        ));
        let split = &lane_splits(&program)[0];
        assert_eq!(split.full_ramp.as_deref(), full.name.as_deref());
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
        assert!(set_name(&mut program, exit.full, "renamed_exit".into()));
        assert_eq!(
            lane_splits(&program)[0].full_ramp.as_deref(),
            Some("renamed_exit")
        );
        let entry_name = lane_splits(&program)[0].ramp.clone();
        assert!(set_split_mode(
            &mut program,
            &entry_name,
            SplitMode::KeepLane
        ));
        assert_eq!(lane_splits(&program)[0].mode, SplitMode::KeepLane);
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
        assert!(set_road_type(
            &mut program,
            exit.through.unwrap(),
            &ramp.name
        ));
        assert_eq!(
            project(&program)
                .segments
                .iter()
                .find(|segment| Some(segment.source) == exit.through)
                .unwrap()
                .road_name,
            ramp.name
        );
    }
}

#[test]
fn an_exit_carrying_the_whole_section_ends_the_parent_at_the_split() {
    let mut program = parse("road path: sidewalk; end;").unwrap();
    let road = match &program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let parent = append(
        &mut program,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
        WorldPoint { x: 4.0, y: 0.0 },
    );
    let (attachment, start) =
        create_lane_point(&mut program, SegmentSource::TopLevel(parent), 0, 0.5).unwrap();
    let exit = append_exit(
        &mut program,
        &road,
        SplitMode::Dedicated,
        start,
        &attachment,
        WorldPoint { x: 4.0, y: -2.0 },
        None,
    )
    .unwrap();
    assert!(exit.through.is_none());
    let projection = project(&program);
    assert_eq!(
        projection
            .segments
            .iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(parent))
            .unwrap()
            .end,
        WorldPoint { x: 2.0, y: 0.0 }
    );
    assert_eq!(projection.unresolved, 0);
}

#[test]
fn a_start_end_exit_transfers_elements_without_a_zero_length_segment() {
    let mut program = parse("road highway: carlane-carlane-sidewalk; end;").unwrap();
    let road = match &program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let parent = append(
        &mut program,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
        WorldPoint { x: 4.0, y: 0.0 },
    );
    let (attachment, start) =
        create_lane_point(&mut program, SegmentSource::TopLevel(parent), 1, 0.0).unwrap();
    let exit = append_exit(
        &mut program,
        &road,
        SplitMode::Dedicated,
        start,
        &attachment,
        WorldPoint { x: 3.0, y: -2.0 },
        None,
    )
    .unwrap();
    assert!(exit.through.is_some());
    let projection = project(&program);
    assert_eq!(projection.unresolved, 0);
    assert!(
        projection
            .segments
            .iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(parent))
            .unwrap()
            .end
            .x
            > 0.0
    );
    assert_eq!(lane_attachment(&program, &attachment).unwrap().2, 1.0);
}

#[test]
fn sample_block_instance_is_visible_without_changing_it() {
    let program = parse(include_str!("../../../../examples/city.road")).unwrap();
    let projection = project(&program);
    assert_eq!(projection.segments.len(), 2);
    assert_eq!(projection.segments[0].start, WorldPoint { x: 0.0, y: 1.0 });
    assert!(!projection.segments[0].source.editable());
}

#[test]
fn reusable_crossing_instances_keep_local_levels() {
    let program = parse(include_str!(
        "../../../../examples/interstate_crossing.road"
    ))
    .unwrap();
    let projection = project(&program);
    assert_eq!(projection.segments.len(), 6);
    assert_eq!(projection.unresolved, 0);
    let west_lower = &projection.segments[0];
    let west_upper = &projection.segments[1];
    let west_ramp = &projection.segments[2];
    let east_lower = &projection.segments[3];
    let east_ramp = &projection.segments[5];
    assert_eq!(west_lower.elevation, 0.0);
    assert_eq!(west_upper.elevation, 0.25);
    assert_eq!((west_ramp.start_z, west_ramp.end_z), (0.0, 0.25));
    assert_eq!(west_ramp.height_at(0.5), 0.125);
    assert_eq!((east_ramp.start_z, east_ramp.end_z), (0.0, 0.25));
    assert_eq!(west_lower.start.x + 6.0, east_lower.start.x);
}

#[test]
fn top_level_and_nested_road_grades_add_instance_heights() {
    let program = parse(
        "road ramp: carlane;\nrole ramp; end; direct = ramp(0, 0, 2, 0); \
        grade(direct, 0, 0.25); block inner: local = ramp(0, 0, 2, 0); \
        grade(local, 0, 0.25); end; block outer: nested = inner(0, 0, \
        0.5); end; outer(4, 0, 1);",
    )
    .unwrap();

    let projection = project(&program);
    assert_eq!(projection.unresolved, 0);
    assert_eq!(
        (projection.segments[0].start_z, projection.segments[0].end_z),
        (0.0, 0.25)
    );
    assert_eq!(
        (projection.segments[1].start_z, projection.segments[1].end_z),
        (1.5, 1.75)
    );
}

#[test]
fn a_block_instance_can_be_raised_independently() {
    let program = parse(
        "road rail: traintrack; end; block crossing: a = rail(-1, 0, 1, \
            0); elevation(a, -0.5); end; one = crossing(0, 0); two = \
            crossing(2, 0, 1);",
    )
    .unwrap();
    let projection = project(&program);
    assert_eq!(
        projection
            .segments
            .iter()
            .map(|segment| segment.elevation)
            .collect::<Vec<_>>(),
        vec![-0.5, 0.5]
    );
}

#[test]
fn nested_block_roads_translate_and_stack_elevation() {
    let program = parse(
        "road rail: traintrack; end; block span: rail_line = rail(-1, 0, \
            1, 0); elevation(rail_line, -0.5); end; block crossing: left = \
            span(-2, 0, 0.25); elevation(left, 0.5); right = span(2, 0); end; \
            crossing(10, 3, 1);",
    )
    .unwrap();
    let projection = project(&program);
    assert_eq!(projection.unresolved, 0);
    assert_eq!(projection.segments.len(), 2);
    assert_eq!(projection.segments[0].start, WorldPoint { x: 7.0, y: 3.0 });
    assert_eq!(projection.segments[1].start, WorldPoint { x: 11.0, y: 3.0 });
    assert_eq!(projection.segments[0].elevation, 1.25);
    assert_eq!(projection.segments[1].elevation, 0.5);
    assert_ne!(projection.segments[0].source, projection.segments[1].source);
    assert!(!projection.segments[0].source.editable());
}

#[test]
fn nested_block_recursion_and_bad_placement_are_unresolved() {
    let program = parse(
        "road rail: traintrack; end; block loop: rail(0, 0, 1, 0); loop(2, \
            0); end; block outer: loop(0, 0); loop(bad, 0); end; outer(0, 0);",
    )
    .unwrap();
    let projection = project(&program);
    assert_eq!(projection.segments.len(), 1);
    assert_eq!(projection.unresolved, 2);
}

#[test]
fn landscape_and_mobility_example_projects_every_road() {
    let program = parse(include_str!(
        "../../../../examples/landscape_and_mobility.road"
    ))
    .unwrap();
    let projection = project(&program);
    assert_eq!(projection.segments.len(), 6);
    assert_eq!(projection.unresolved, 0);
    assert_eq!(
        opencity::project_landscape_features(&program)
            .features
            .len(),
        3
    );
}

#[test]
fn curved_exit_preserves_earlier_lane_attachment_and_split_grade() {
    let mut program = parse("road path: sidewalk; end;").unwrap();
    let Statement::Road(road) = &program.statements[0] else {
        unreachable!()
    };
    let road = road.clone();
    let parent = SegmentSource::TopLevel(append_curve(
        &mut program,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
        WorldPoint { x: 1.0, y: 1.0 },
        WorldPoint { x: 3.0, y: 1.0 },
        WorldPoint { x: 4.0, y: 0.0 },
    ));
    assert!(set_name(&mut program, parent, "parent".into()));
    assert!(opencity::elevation::set_grade(
        &mut program,
        "parent",
        2.0,
        6.0
    ));
    let (earlier, position) = create_lane_point(&mut program, parent, 0, 0.25).unwrap();
    let (attachment, start) = create_lane_point(&mut program, parent, 0, 0.5).unwrap();
    let exit = append_exit(
        &mut program,
        &road,
        SplitMode::Dedicated,
        start,
        &attachment,
        WorldPoint { x: 5.0, y: -2.0 },
        None,
    )
    .unwrap();
    let projection = project(&program);
    assert_eq!(projection.unresolved, 0);
    let point = projection
        .points
        .iter()
        .find(|point| point.name == earlier)
        .unwrap();
    assert!((point.position.x - position.x).abs() < 0.001);
    assert!((point.position.y - position.y).abs() < 0.001);
    assert_eq!(
        lane_attachment(&program, &earlier),
        Some(("parent".into(), 0, 0.5))
    );
    assert_eq!(
        lane_attachment(&program, &attachment),
        Some(("parent".into(), 0, 1.0))
    );
    let shortened = projection
        .segments
        .iter()
        .find(|segment| segment.source == parent)
        .unwrap();
    assert_eq!((shortened.start_z, shortened.end_z), (2.0, 4.0));
    let entry = projection
        .segments
        .iter()
        .find(|segment| segment.source == exit.entry)
        .unwrap();
    assert_eq!(entry.start_ref.as_deref(), Some(attachment.as_str()));
    assert_eq!(entry.start_z, 4.0);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}
