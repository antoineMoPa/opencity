use super::*;

#[test]
fn copied_curve_preserves_grade_and_round_trips() {
    let mut editor = sample_editor();
    editor.program = parse(
        "road street:\n    carlane;\nend;\nroad_1 = street(0,0,1,1,2,1,3,\
            0);\ngrade(road_1,0.25,0.75);",
    )
    .unwrap();
    editor.selected_segment = Some(SegmentSource::TopLevel(1));
    editor.copy_selection();
    editor.paste_selection();
    let segments = map::project(&editor.program).segments;
    assert_eq!(segments.len(), 2);
    assert!((segments[1].start.x - 1.0).abs() < 0.0001);
    assert!((segments[1].start.y + 1.0).abs() < 0.0001);
    assert!((segments[1].controls[0].x - 2.0).abs() < 0.0001);
    assert!((segments[1].start_z - 0.25).abs() < 0.0001);
    assert!((segments[1].end_z - 0.75).abs() < 0.0001);
    assert_eq!(
        parse(&editor.program.to_road_source()).unwrap(),
        editor.program
    );
}

#[test]
fn box_selected_roads_delete_even_when_junctions_shift_indices() {
    let mut editor = sample_editor();
    editor.program = parse("road street:\n    carlane;\nend;").unwrap();
    let road = match &editor.program.statements[0] {
        Statement::Road(road) => road.clone(),
        _ => unreachable!(),
    };
    let first = map::append_connected(
        &mut editor.program,
        &road,
        WorldPoint { x: 0.0, y: 0.0 },
        None,
        WorldPoint { x: 1.0, y: 0.0 },
        None,
    )
    .0;
    let second = map::append_connected(
        &mut editor.program,
        &road,
        WorldPoint { x: 2.0, y: 0.0 },
        None,
        WorldPoint { x: 3.0, y: 0.0 },
        None,
    )
    .0;
    editor.selected_segments = vec![
        SegmentSource::TopLevel(first),
        SegmentSource::TopLevel(second),
    ];
    assert_eq!(editor.delete_selected_segments(), 2);
    assert!(map::project(&editor.program).segments.is_empty());
    assert_eq!(
        parse(&editor.program.to_road_source()).unwrap(),
        editor.program
    );
}
