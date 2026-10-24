use super::*;

#[test]
fn narrow_box_crosses_a_visible_road_between_samples() {
    let box_rect = egui::Rect::from_min_max(egui::pos2(49.0, 49.0), egui::pos2(51.0, 51.0));
    assert!(line_crosses_rect(
        egui::pos2(0.0, 0.0),
        egui::pos2(100.0, 100.0),
        box_rect
    ));
    assert!(!line_crosses_rect(
        egui::pos2(0.0, 0.0),
        egui::pos2(100.0, 20.0),
        box_rect
    ));
}

#[test]
fn isometric_handle_pick_and_drag_preserve_road_height() {
    let mut program = parse("road lane: sidewalk-carlane-sidewalk; end;").unwrap();
    let Statement::Road(road) = &program.statements[0] else {
        unreachable!()
    };
    let road = road.clone();
    let index = map::append(
        &mut program,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
        WorldPoint { x: 3.0, y: 0.0 },
    );
    let source = SegmentSource::TopLevel(index);
    let projection = map::project(&program);
    let segment = projection
        .segments
        .iter()
        .find(|segment| segment.source == source)
        .unwrap();
    let rect = egui::Rect::from_min_size(egui::pos2(100.0, 60.0), egui::vec2(800.0, 600.0));
    let pan = egui::vec2(27.0, -19.0);
    let target = WorldPoint { x: 1.0, y: -0.5 };
    let angle = 0.72;
    let endpoint = iso_screen(
        rect,
        pan,
        90.0,
        target,
        angle,
        ISO_ELEVATION,
        segment.start,
        segment.start_z,
    );
    assert_eq!(
        iso_endpoint_near(
            &projection.segments,
            &program,
            rect,
            pan,
            90.0,
            target,
            angle,
            ISO_ELEVATION,
            endpoint
        ),
        Some((source, false))
    );
    let control = iso_screen(
        rect,
        pan,
        90.0,
        target,
        angle,
        ISO_ELEVATION,
        segment.controls[1],
        segment.height_at(2.0 / 3.0),
    );
    assert_eq!(
        iso_control_near(
            &projection.segments,
            rect,
            pan,
            90.0,
            target,
            angle,
            ISO_ELEVATION,
            control
        ),
        Some((source, true))
    );
    let moved = WorldPoint { x: 0.4, y: 0.7 };
    let projected = iso_screen(rect, pan, 90.0, target, angle, ISO_ELEVATION, moved, 1.25);
    let picked = iso_world_at_height(
        rect,
        pan,
        90.0,
        target,
        angle,
        ISO_ELEVATION,
        projected,
        1.25,
    );
    assert!((picked.x - moved.x).abs() < 0.0001);
    assert!((picked.y - moved.y).abs() < 0.0001);
    assert!(map::set_control(&mut program, source, true, picked));
    let changed = map::project(&program)
        .segments
        .into_iter()
        .find(|segment| segment.source == source)
        .unwrap();
    assert!((changed.controls[1].x - moved.x).abs() < 0.0001);
    assert!((changed.controls[1].y - moved.y).abs() < 0.0001);
}

#[test]
fn isometric_element_end_picks_branch_source() {
    let mut program = parse("road lane: sidewalk-carlane-sidewalk; end;").unwrap();
    let Statement::Road(road) = &program.statements[0] else {
        unreachable!()
    };
    let road = road.clone();
    map::append(
        &mut program,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
        WorldPoint { x: 4.0, y: 0.0 },
    );
    let projection = map::project(&program);
    let candidate = element_end_candidates(&projection, &program)
        .into_iter()
        .find(|candidate| candidate.element_index == 0 && candidate.t == 1.0)
        .unwrap();
    let segment = projection
        .segments
        .iter()
        .find(|segment| segment.source == candidate.source)
        .unwrap();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 500.0));
    let target = WorldPoint { x: 2.0, y: 0.0 };
    let screen = iso_screen(
        rect,
        egui::Vec2::ZERO,
        80.0,
        target,
        0.8,
        ISO_ELEVATION,
        candidate.position,
        segment.height_at(candidate.t),
    );
    let found = iso_element_end_near(
        &element_end_candidates(&projection, &program),
        &projection,
        rect,
        egui::Vec2::ZERO,
        80.0,
        target,
        0.8,
        ISO_ELEVATION,
        screen,
    )
    .unwrap();
    assert_eq!(found.source, candidate.source);
    assert_eq!(found.element_index, candidate.element_index);
    assert_eq!(found.t, candidate.t);
}

#[test]
fn map_hit_testing_respects_pan_and_zoom() {
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
    let pan = egui::vec2(45.0, -30.0);
    let world = WorldPoint { x: 2.0, y: -1.0 };
    let screen = screen_of(rect, pan, 80.0, world);
    assert_eq!(world_of(rect, pan, 80.0, screen), world);
    let segment = map::Segment {
        source: SegmentSource::TopLevel(1),
        name: Some("road_1".into()),
        road_name: "neighborhood_street".into(),
        direction: RoadDirection::Both,
        start: world,
        end: WorldPoint { x: 4.0, y: -1.0 },
        controls: [
            WorldPoint { x: 2.67, y: -1.0 },
            WorldPoint { x: 3.33, y: -1.0 },
        ],
        start_ref: None,
        end_ref: None,
        elevation: 0.0,
        start_z: 0.0,
        end_z: 0.0,
    };
    assert_eq!(
        segment_near(
            &[segment.clone()],
            &default_program(),
            rect,
            pan,
            80.0,
            screen
        ),
        Some(segment.source)
    );
    assert_eq!(
        endpoint_near(&[segment], &default_program(), rect, pan, 80.0, screen),
        Some((SegmentSource::TopLevel(1), false))
    );
}

#[test]
fn map_hit_picks_upper_road_at_local_grade_height() {
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
    let program = parse("road lane: carlane; end; low = lane(0, 0, 2, 0); high = lane(0, 0, 2, 0); grade(high, -1, 1);").unwrap();
    let segments = map::project(&program).segments;
    let left = screen_of(rect, egui::Vec2::ZERO, 80.0, WorldPoint { x: 0.25, y: 0.0 });
    let right = screen_of(rect, egui::Vec2::ZERO, 80.0, WorldPoint { x: 1.75, y: 0.0 });
    assert_eq!(
        segment_near(&segments, &program, rect, egui::Vec2::ZERO, 80.0, left),
        Some(segments[0].source)
    );
    assert_eq!(
        segment_near(&segments, &program, rect, egui::Vec2::ZERO, 80.0, right),
        Some(segments[1].source)
    );
}

#[test]
fn every_cross_section_element_has_a_pickable_end() {
    let mut program = parse("road highway: verge-carlane-middle-carlane-verge; end;").unwrap();
    let road = match &program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    map::append(
        &mut program,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
        WorldPoint { x: 4.0, y: 0.0 },
    );
    let candidates = element_end_candidates(&map::project(&program), &program);
    assert_eq!(candidates.len(), road.elements.len() * 2);
    let target = candidates
        .iter()
        .find(|candidate| candidate.element_index == 3 && candidate.t == 1.0)
        .unwrap();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    let pointer = screen_of(rect, egui::Vec2::ZERO, 420.0, target.position);
    let picked = element_end_near(&candidates, rect, egui::Vec2::ZERO, 420.0, pointer).unwrap();
    assert_eq!((picked.element_index, picked.t), (3, 1.0));
}

#[test]
fn hover_points_show_four_nearest_visible_candidates_across_kinds() {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
    let pointer = egui::pos2(50.0, 50.0);
    let points = [
        (HoverPoint::Element(0), egui::pos2(90.0, 50.0)),
        (HoverPoint::Junction(0), egui::pos2(52.0, 50.0)),
        (HoverPoint::Element(1), egui::pos2(49.0, 50.0)),
        (HoverPoint::Junction(1), egui::pos2(53.0, 50.0)),
        (HoverPoint::Element(2), egui::pos2(54.0, 50.0)),
        (HoverPoint::Junction(2), egui::pos2(50.0, 101.0)),
    ];
    assert_eq!(
        nearest_visible_points(points.into_iter(), rect, pointer),
        vec![
            HoverPoint::Element(1),
            HoverPoint::Junction(0),
            HoverPoint::Junction(1),
            HoverPoint::Element(2),
        ]
    );
}

#[test]
fn older_lane_branch_can_switch_to_keep_lane_mode() {
    let mut program = parse(
        "road highway: verge-carlane-verge; end; road ramp: verge-carlane-verge;
role ramp; end;",
    )
    .unwrap();
    let highway = match &program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let ramp_road = match &program.statements[1] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let (parent, _) = map::append_connected(
        &mut program,
        &highway,
        WorldPoint { x: 0.0, y: 0.0 },
        None,
        WorldPoint { x: 4.0, y: 0.0 },
        None,
    );
    let (reference, position) =
        map::create_lane_point(&mut program, SegmentSource::TopLevel(parent), 1, 0.5).unwrap();
    let (ramp, _) = map::append_connected(
        &mut program,
        &ramp_road,
        position,
        Some(&reference),
        WorldPoint { x: 6.0, y: -1.0 },
        None,
    );
    let ramp_name = map::project(&program)
        .segments
        .into_iter()
        .find(|segment| segment.source == SegmentSource::TopLevel(ramp))
        .unwrap()
        .name
        .unwrap();
    assert_eq!(
        map::lane_splits(&program)[0].mode,
        map::SplitMode::Dedicated
    );
    assert!(map::set_split_mode(
        &mut program,
        &ramp_name,
        map::SplitMode::KeepLane
    ));
    assert_eq!(map::lane_splits(&program).len(), 1);
    assert_eq!(map::lane_splits(&program)[0].mode, map::SplitMode::KeepLane);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}
