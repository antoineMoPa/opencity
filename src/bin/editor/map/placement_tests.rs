use super::*;
use opencity::parse;

#[test]
fn placed_segment_round_trips_and_moves_with_negative_coordinates() {
    let mut program = parse("road lane: verge-carlane-verge; end;").unwrap();
    let road = match &program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let index = append(
        &mut program,
        &road,
        WorldPoint { x: -2.0, y: 1.0 },
        WorldPoint { x: 3.0, y: 1.0 },
    );
    assert!(set_connected_endpoint(
        &mut program,
        SegmentSource::TopLevel(index),
        true,
        WorldPoint { x: 4.0, y: -1.0 }
    ));
    let reparsed = parse(&program.to_road_source()).unwrap();
    assert_eq!(reparsed, program);
    assert_eq!(
        project(&program).segments[0].end,
        WorldPoint { x: 4.0, y: -1.0 }
    );
    assert!(remove(&mut program, SegmentSource::TopLevel(index)));
    assert!(project(&program).segments.is_empty());
}

#[test]
fn named_curves_reuse_cross_sections_and_move_connected_junctions() {
    let mut program = parse("road avenue: sidewalk-carlane-sidewalk; end;").unwrap();
    let road = match &program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let first = append_curve(
        &mut program,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
        WorldPoint { x: 0.0, y: 1.0 },
        WorldPoint { x: 1.0, y: 1.0 },
        WorldPoint { x: 1.0, y: 0.0 },
    );
    let second = append_curve(
        &mut program,
        &road,
        WorldPoint { x: 1.0, y: 0.0 },
        WorldPoint { x: 1.0, y: -1.0 },
        WorldPoint { x: 2.0, y: -1.0 },
        WorldPoint { x: 2.0, y: 0.0 },
    );
    assert_eq!(
        project(&program).segments[0].point_at(0.5),
        WorldPoint { x: 0.5, y: 0.75 }
    );
    assert!(set_name(
        &mut program,
        SegmentSource::TopLevel(first),
        "curved_main".into()
    ));
    assert!(set_name(
        &mut program,
        SegmentSource::TopLevel(second),
        "second_main".into()
    ));
    assert!(set_connected_endpoint(
        &mut program,
        SegmentSource::TopLevel(first),
        true,
        WorldPoint { x: 1.5, y: 0.5 }
    ));
    let segments = project(&program).segments;
    assert_eq!(segments[0].end, segments[1].start);
    assert_eq!(segments[0].controls[1], WorldPoint { x: 1.5, y: 1.5 });
    assert_eq!(segments[0].road_name, segments[1].road_name);
    assert_ne!(segments[0].name, segments[1].name);
    assert!(set_control(
        &mut program,
        SegmentSource::TopLevel(first),
        true,
        WorldPoint { x: 2.0, y: 1.5 }
    ));
    assert_eq!(
        project(&program).segments[1].controls[0],
        WorldPoint { x: 1.0, y: -0.5 }
    );
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn clicked_junction_is_shared_and_moves_both_roads() {
    let mut program = parse("road street: sidewalk-carlane-sidewalk; end;").unwrap();
    let road = match &program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let (first, junction) = append_connected(
        &mut program,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
        None,
        WorldPoint { x: 1.0, y: 0.0 },
        None,
    );
    let (second, _) = append_connected(
        &mut program,
        &road,
        WorldPoint { x: 1.0, y: 0.0 },
        Some(&junction),
        WorldPoint { x: 2.0, y: 0.0 },
        None,
    );
    let segments = project(&program).segments;
    assert_eq!(segments[0].end_ref.as_deref(), Some(junction.as_str()));
    assert_eq!(segments[1].start_ref.as_deref(), Some(junction.as_str()));
    assert!(set_connected_endpoint(
        &mut program,
        SegmentSource::TopLevel(first),
        true,
        WorldPoint { x: 1.0, y: 0.5 }
    ));
    let segments = project(&program).segments;
    assert_eq!(segments[0].end, segments[1].start);
    assert_eq!(segments[1].start, WorldPoint { x: 1.0, y: 0.5 });
    assert!(remove(&mut program, SegmentSource::TopLevel(second)));
    assert_eq!(project(&program).segments.len(), 1);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn continuation_inherits_curve_direction_at_shared_end() {
    let mut program = parse("road street: sidewalk-carlane-sidewalk; end;").unwrap();
    let road = match &program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let (first, junction) = append_connected(
        &mut program,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
        None,
        WorldPoint { x: 2.0, y: 0.0 },
        None,
    );
    assert!(set_control_direct(
        &mut program,
        SegmentSource::TopLevel(first),
        true,
        WorldPoint { x: 1.5, y: -0.5 }
    ));
    let (second, _) = append_connected(
        &mut program,
        &road,
        WorldPoint { x: 2.0, y: 0.0 },
        Some(&junction),
        WorldPoint { x: 3.0, y: 1.0 },
        None,
    );
    let segments = project(&program).segments;
    let incoming = WorldPoint {
        x: segments[0].end.x - segments[0].controls[1].x,
        y: segments[0].end.y - segments[0].controls[1].y,
    };
    let outgoing = WorldPoint {
        x: segments[1].controls[0].x - segments[1].start.x,
        y: segments[1].controls[0].y - segments[1].start.y,
    };
    assert!((incoming.x * outgoing.y - incoming.y * outgoing.x).abs() < 0.0001);
    assert!(incoming.x * outgoing.x + incoming.y * outgoing.y > 0.0);
    assert_eq!(segments[1].source, SegmentSource::TopLevel(second));
}

#[test]
fn lane_branch_follows_parent_shape_and_name() {
    let mut program = parse(
        "road highway: verge-carlane-carlane-verge; end; road ramp: verge-carlane-verge;
role ramp; end;",
    )
    .unwrap();
    let parent_road = match &program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let ramp = match &program.statements[1] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let (parent, _) = append_connected(
        &mut program,
        &parent_road,
        WorldPoint { x: 0.0, y: 0.0 },
        None,
        WorldPoint { x: 4.0, y: 0.0 },
        None,
    );
    let (attachment, position) =
        create_lane_point(&mut program, SegmentSource::TopLevel(parent), 1, 0.5).unwrap();
    let (branch, _) = append_connected(
        &mut program,
        &ramp,
        position,
        Some(&attachment),
        WorldPoint { x: 3.0, y: -2.0 },
        None,
    );
    let before = project(&program)
        .segments
        .into_iter()
        .find(|segment| segment.source == SegmentSource::TopLevel(branch))
        .unwrap();
    assert!(set_control(
        &mut program,
        SegmentSource::TopLevel(parent),
        false,
        WorldPoint { x: 1.0, y: 1.0 }
    ));
    let after = project(&program)
        .segments
        .into_iter()
        .find(|segment| segment.source == SegmentSource::TopLevel(branch))
        .unwrap();
    assert_ne!(before.start, after.start);
    let parent_after = project(&program)
        .segments
        .into_iter()
        .find(|segment| segment.source == SegmentSource::TopLevel(parent))
        .unwrap();
    let direction = element_direction(&parent_after, &parent_road, 1, 0.5).unwrap();
    let handle = WorldPoint {
        x: after.controls[0].x - after.start.x,
        y: after.controls[0].y - after.start.y,
    };
    assert!((handle.x * direction.y - handle.y * direction.x).abs() < 0.001);
    assert!(handle.x * direction.x + handle.y * direction.y > 0.0);
    assert!(set_name(
        &mut program,
        SegmentSource::TopLevel(parent),
        "interstate_west".into()
    ));
    assert_eq!(
        project(&program)
            .segments
            .iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(branch))
            .unwrap()
            .start,
        after.start
    );
    assert!(set_lane_attachment(&mut program, &attachment, 2, 0.75));
    let relocated = project(&program)
        .segments
        .into_iter()
        .find(|segment| segment.source == SegmentSource::TopLevel(branch))
        .unwrap();
    assert_ne!(relocated.start, after.start);
    assert!(remove(&mut program, SegmentSource::TopLevel(parent)));
    let remaining = project(&program).segments;
    assert_eq!(remaining.len(), 1);
    assert!((remaining[0].start.x - relocated.start.x).abs() < 0.01);
    assert!((remaining[0].start.y - relocated.start.y).abs() < 0.01);
    assert!((remaining[0].controls[0].x - relocated.controls[0].x).abs() < 0.01);
    assert!((remaining[0].controls[0].y - relocated.controls[0].y).abs() < 0.01);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn branch_uses_visible_element_side_and_parent_tangent() {
    let mut program = parse(
        "road highway: verge-carlane-middle-carlane-verge; end; road ramp: verge-carlane-verge;
role ramp; end;",
    )
    .unwrap();
    let parent_road = match &program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let ramp = match &program.statements[1] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let (parent, _) = append_connected(
        &mut program,
        &parent_road,
        WorldPoint { x: 0.0, y: 0.0 },
        None,
        WorldPoint { x: 4.0, y: 0.0 },
        None,
    );
    let segment = project(&program)
        .segments
        .into_iter()
        .find(|segment| segment.source == SegmentSource::TopLevel(parent))
        .unwrap();
    let upper = element_position(&segment, &parent_road, 1, 0.5).unwrap();
    let lower = element_position(&segment, &parent_road, 3, 0.5).unwrap();
    assert!(upper.y > 0.0);
    assert!(lower.y < 0.0);
    assert!((upper.x - 2.0).abs() < 0.0001);

    for (element_index, position, travel_x) in [(1, upper, -1.0), (3, lower, 1.0)] {
        let expected_direction = if travel_x < 0.0 {
            RoadDirection::Reverse
        } else {
            RoadDirection::Forward
        };
        assert_eq!(
            parent_road.lane_direction(element_index),
            Some(expected_direction)
        );
        let (attachment, start) = create_lane_point(
            &mut program,
            SegmentSource::TopLevel(parent),
            element_index,
            0.5,
        )
        .unwrap();
        let (branch, _) = append_connected(
            &mut program,
            &ramp,
            start,
            Some(&attachment),
            WorldPoint {
                x: 3.0,
                y: position.y * 2.0,
            },
            None,
        );
        let projected = project(&program)
            .segments
            .into_iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(branch))
            .unwrap();
        assert!((projected.start.y - position.y).abs() < 0.0001);
        assert!((projected.controls[0].y - projected.start.y).abs() < 0.0001);
        assert!((projected.controls[0].x - projected.start.x) * travel_x > 0.0);
    }
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn lane_branch_tracks_cross_section_edits_and_detaches_when_lane_is_removed() {
    use opencity::{RoadElement, RoadElementKind};

    let mut program = parse(
        "road highway: verge-carlane-carlane-verge; end; road ramp: verge-carlane-verge;
role ramp; end;",
    )
    .unwrap();
    let parent_road = match &program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let ramp = match &program.statements[1] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let (parent, _) = append_connected(
        &mut program,
        &parent_road,
        WorldPoint { x: 0.0, y: 0.0 },
        None,
        WorldPoint { x: 4.0, y: 0.0 },
        None,
    );
    let (attachment, position) =
        create_lane_point(&mut program, SegmentSource::TopLevel(parent), 1, 0.5).unwrap();
    let (branch, _) = append_connected(
        &mut program,
        &ramp,
        position,
        Some(&attachment),
        WorldPoint { x: 3.0, y: -2.0 },
        None,
    );

    let before = project(&program);
    if let Statement::Road(road) = &mut program.statements[0] {
        road.elements.insert(
            1,
            RoadElement {
                kind: RoadElementKind::Sidewalk,
                catch: None,
            },
        );
    }
    update_lane_attachments(&mut program, "highway", ElementEdit::Inserted(1), &before);
    assert_eq!(lane_attachment(&program, &attachment).unwrap().1, 2);

    let before = project(&program);
    if let Statement::Road(road) = &mut program.statements[0] {
        road.elements.swap(2, 3);
    }
    update_lane_attachments(&mut program, "highway", ElementEdit::Swapped(2, 3), &before);
    assert_eq!(lane_attachment(&program, &attachment).unwrap().1, 3);

    let before = project(&program);
    let branch_before = before
        .segments
        .iter()
        .find(|segment| segment.source == SegmentSource::TopLevel(branch))
        .unwrap()
        .clone();
    if let Statement::Road(road) = &mut program.statements[0] {
        road.elements.remove(3);
    }
    update_lane_attachments(&mut program, "highway", ElementEdit::Removed(3), &before);
    let branch_after = project(&program)
        .segments
        .into_iter()
        .find(|segment| segment.source == SegmentSource::TopLevel(branch))
        .unwrap();
    assert!(lane_attachment(&program, &attachment).is_none());
    assert!((branch_after.start.x - branch_before.start.x).abs() < 0.01);
    assert!((branch_after.start.y - branch_before.start.y).abs() < 0.01);
    assert!((branch_after.controls[0].x - branch_before.controls[0].x).abs() < 0.01);
    assert!((branch_after.controls[0].y - branch_before.controls[0].y).abs() < 0.01);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}
