use super::*;
use serde_json::json;

fn editor() -> RoadEditor {
    let mut editor = crate::tests::sample_editor();
    editor.program = parse(include_str!(
        "../../../../road_library/street_to_interstate_flat.road"
    ))
    .unwrap();
    editor.saved_program = editor.program.clone();
    editor
}

#[test]
fn connection_discovery_focus_and_atomic_attachment_edits() {
    let mut editor = editor();
    let items = connections(&editor.program);
    assert!(!items.is_empty());
    let item = items
        .iter()
        .find(|item| {
            item.segments.iter().any(|road| {
                road.start_ref
                    .as_deref()
                    .into_iter()
                    .chain(road.end_ref.as_deref())
                    .any(|reference| map::lane_attachment(&editor.program, reference).is_some())
            })
        })
        .unwrap()
        .clone();
    editor.focus_connection(&item.id).unwrap();
    assert_eq!(editor.active_connection().unwrap().id, item.id);
    let road = item.segments.first().unwrap();
    let reference = [&road.start_ref, &road.end_ref]
        .into_iter()
        .flatten()
        .find(|reference| map::lane_attachment(&editor.program, reference).is_some())
        .unwrap();
    let before = editor.program.clone();
    for t in [-0.1, 1.1] {
        assert!(editor.call_mcp("edit_connection", &json!({"connection_id":item.id,"edit":"attachment","reference":reference,"lane":0,"t":t})).is_err());
        assert_eq!(editor.program, before);
    }
    let (_, lane, t) = map::lane_attachment(&editor.program, reference).unwrap();
    editor.call_mcp("edit_connection", &json!({"connection_id":item.id,"edit":"attachment","reference":reference,"lane":lane,"t":t * 0.9})).unwrap();
    assert!(map::lane_attachment(&editor.program, reference).is_some());
    assert_eq!(
        parse(&editor.program.to_road_source()).unwrap(),
        editor.program
    );
    editor.record_history(before.clone(), false);
    assert!(editor.restore_history(true));
    assert_eq!(editor.program, before);
    assert!(editor.restore_history(false));
    assert_ne!(editor.program, before);
}

#[test]
fn connection_handles_and_anchored_endpoints_share_validated_api() {
    let mut editor = editor();
    let item = connections(&editor.program)
        .into_iter()
        .find(|item| {
            item.segments[0]
                .start_ref
                .as_deref()
                .into_iter()
                .chain(item.segments[0].end_ref.as_deref())
                .any(|reference| map::lane_attachment(&editor.program, reference).is_some())
        })
        .unwrap();
    let road = &item.segments[0];
    let before = editor.program.clone();
    let end = road
        .end_ref
        .as_deref()
        .is_some_and(|reference| map::lane_attachment(&editor.program, reference).is_some());
    assert!(editor.call_mcp("edit_connection", &json!({"connection_id":item.id,"edit":if end {"end"}else{"start"},"segment":road.name,"x":1,"y":2})).is_err());
    assert_eq!(editor.program, before);
    editor.call_mcp("edit_connection", &json!({"connection_id":item.id,"edit":"handle1","segment":road.name,"x":road.controls[0].x + 0.01,"y":road.controls[0].y})).unwrap();
    assert_ne!(editor.program, before);
    let edited = editor.program.clone();
    assert!(editor
        .edit_connection(
            &item.id,
            ConnectionEdit::Grade {
                segment: road.name.clone().unwrap(),
                start: f32::NAN,
                end: 0.0
            }
        )
        .is_err());
    assert_eq!(editor.program, edited);
}

#[test]
fn connection_real_port_intersection_moves_without_losing_ports() {
    let mut editor = editor();
    editor.program = parse(include_str!("../../../../road_library/four_way.road")).unwrap();
    let item = connections(&editor.program).remove(0);
    let before = editor.program.clone();
    let original = item.intersection.unwrap();
    editor
        .edit_connection(
            &item.id,
            ConnectionEdit::Intersection {
                position: WorldPoint { x: 0.25, y: 0.5 },
                kind: original.kind,
                profile: original.road_name.clone(),
            },
        )
        .unwrap();
    let moved = connections(&editor.program).remove(0).intersection.unwrap();
    assert_eq!(moved.ports.len(), 4);
    assert_eq!(moved.position, WorldPoint { x: 0.25, y: 0.5 });
    assert_eq!(map::project(&editor.program).unresolved, 0);
    let edited = editor.program.clone();
    assert!(editor
        .edit_connection(
            &item.id,
            ConnectionEdit::Intersection {
                position: moved.position,
                kind: map::IntersectionKind::ThreeWay,
                profile: original.road_name
            }
        )
        .is_err());
    assert_eq!(editor.program, edited);
    assert_ne!(editor.program, before);
}

#[test]
fn connection_shared_endpoint_does_not_move_coincident_overpass() {
    let mut program = parse("road lane: carlane; end; a=map_point(0,0); b=map_point(0,0); c=map_point(1,0); d=map_point(0,1); first=lane(a,0.3,0,0.7,0,c); second=lane(b,0,0.3,0,0.7,d); elevation(second,1);").unwrap();
    let projection = map::project(&program);
    let first = projection.segments[0].source;
    let second = projection.segments[1].clone();
    assert!(map::set_connected_endpoint(
        &mut program,
        first,
        false,
        WorldPoint { x: -1.0, y: 0.0 }
    ));
    let after = map::project(&program);
    assert_eq!(after.segments[1].start, second.start);
    assert_eq!(after.segments[1].controls, second.controls);
}

#[test]
fn connection_three_way_profile_and_grade_edits_round_trip() {
    let mut editor = editor();
    editor.program = parse(include_str!("../../../../road_library/three_way.road")).unwrap();
    let mut profile = editor
        .program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Road(road) => Some(road.clone()),
            _ => None,
        })
        .unwrap();
    profile.name = "local_edge".into();
    editor.program.statements.push(Statement::Road(profile));
    let definitions: Vec<_> = editor
        .program
        .statements
        .iter()
        .filter(|statement| matches!(statement, Statement::Road(_)))
        .cloned()
        .collect();
    let item = connections(&editor.program).remove(0);
    let intersection = item.intersection.unwrap();
    editor.call_mcp("edit_connection", &json!({"connection_id":item.id,"edit":"intersection","x":intersection.position.x,"y":intersection.position.y,"kind":"three_way","profile":"local_edge"})).unwrap();
    let item = connections(&editor.program).remove(0);
    assert_eq!(item.intersection.as_ref().unwrap().road_name, "local_edge");
    let name = item.segments[0].name.as_ref().unwrap();
    editor.call_mcp("edit_connection", &json!({"connection_id":item.id,"edit":"grade","segment":name,"start_height":0.125,"end_height":0.25})).unwrap();
    let road = map::project(&editor.program)
        .segments
        .into_iter()
        .find(|road| road.name.as_ref() == Some(name))
        .unwrap();
    assert_eq!((road.start_z, road.end_z), (0.125, 0.25));
    assert_eq!(
        definitions,
        editor
            .program
            .statements
            .iter()
            .filter(|statement| matches!(statement, Statement::Road(_)))
            .cloned()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        parse(&editor.program.to_road_source()).unwrap(),
        editor.program
    );
    let before = editor.program.clone();
    assert!(editor
        .call_mcp(
            "edit_connection",
            &json!({"connection_id":item.id,"edit":"handle1","segment":"unrelated","x":0,"y":0})
        )
        .is_err());
    assert_eq!(editor.program, before);
}

#[test]
fn connection_road_camera_uses_lane_scale_and_keeps_selection_name() {
    let mut editor = editor();
    let item = connections(&editor.program).remove(0);
    editor.camera_view = CameraView::Vehicular;
    editor.focus_connection(&item.id).unwrap();
    let segment = &item.segments[0];
    assert_eq!(editor.segment_name_draft, segment.name.clone().unwrap());
    assert_eq!(editor.selected_segment, Some(segment.source));
    assert!(editor.vehicle_eye_height >= segment.start_z.min(segment.end_z) + 0.01);
    assert!(editor.vehicle_eye_height <= segment.start_z.max(segment.end_z) + 0.03);
    let toward_start = segment
        .start_ref
        .as_deref()
        .is_some_and(|reference| map::lane_attachment(&editor.program, reference).is_some());
    let endpoint = if toward_start {
        segment.start
    } else {
        segment.end
    };
    let direction = WorldPoint {
        x: endpoint.x - editor.vehicle_position.x,
        y: endpoint.y - editor.vehicle_position.y,
    };
    assert!(
        direction.x * editor.vehicle_heading.cos() + direction.y * editor.vehicle_heading.sin()
            > 0.0
    );
}

#[test]
fn connection_member_selection_is_scoped_and_does_not_move_camera() {
    let mut editor = editor();
    let item = connections(&editor.program)
        .into_iter()
        .find(|item| item.segments.len() > 1)
        .unwrap();
    editor.focus_connection(&item.id).unwrap();
    let road = &item.segments[1];
    let name = road.name.as_ref().unwrap();
    let camera = (editor.map_pan, editor.map_zoom, editor.orbit_target);
    editor.select_connection_member(&item, name).unwrap();
    assert_eq!(editor.selected_segments, vec![road.source]);
    assert_eq!(editor.segment_name_draft, *name);
    assert_eq!(
        editor.connection_member(&item).unwrap().name.as_ref(),
        Some(name)
    );
    assert_eq!(
        (editor.map_pan, editor.map_zoom, editor.orbit_target),
        camera
    );
    editor.focus_connection(&item.id).unwrap();
    assert_eq!(editor.selected_segment, Some(road.source));
    let mut other = item.clone();
    other.id = "other".into();
    editor.selected_segment = None;
    assert_eq!(
        editor.connection_member(&other).unwrap().source,
        other.segments[0].source
    );
}
