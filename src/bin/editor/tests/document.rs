use super::*;

#[test]
fn terrain_tool_creates_named_feature_and_concave_fill() {
    let mut editor = sample_editor();
    editor.program = Program::default();
    editor.start_landscape(LandscapeKind::Lake);
    editor.landscape_points = vec![
        WorldPoint { x: 0.0, y: 0.0 },
        WorldPoint { x: 4.0, y: 0.0 },
        WorldPoint { x: 4.0, y: 4.0 },
        WorldPoint { x: 2.0, y: 2.0 },
        WorldPoint { x: 0.0, y: 4.0 },
    ];
    editor.finish_landscape();
    assert_eq!(editor.selected_landscape.as_deref(), Some("lake_1"));
    assert_eq!(
        project_landscape_features(&editor.program).features.len(),
        1
    );
    assert_eq!(
        parse(&editor.program.to_road_source()).unwrap(),
        editor.program
    );
    let points = [
        egui::pos2(0.0, 0.0),
        egui::pos2(4.0, 0.0),
        egui::pos2(4.0, 4.0),
        egui::pos2(2.0, 2.0),
        egui::pos2(0.0, 4.0),
    ];
    assert!(!landscape_is_convex(&points));
    assert_eq!(landscape_triangles(&points).len(), 3);
    let rect = egui::Rect::from_center_size(egui::pos2(300.0, 300.0), egui::vec2(600.0, 600.0));
    let inside = screen_of(rect, egui::Vec2::ZERO, 50.0, WorldPoint { x: 1.0, y: 1.0 });
    let inlet = screen_of(rect, egui::Vec2::ZERO, 50.0, WorldPoint { x: 2.0, y: 3.0 });
    assert_eq!(
        landscape_at(&editor.program, rect, egui::Vec2::ZERO, 50.0, inside),
        Some("lake_1".into())
    );
    assert_eq!(
        landscape_at(&editor.program, rect, egui::Vec2::ZERO, 50.0, inlet),
        None
    );
}

#[test]
fn history_groups_drag_and_tracks_saved_state() {
    let mut editor = sample_editor();
    let original = editor.program.clone();
    editor.add_road();
    editor.record_history(original.clone(), true);
    let first_edit = editor.program.clone();
    editor.add_road();
    editor.record_history(first_edit, true);
    assert_eq!(editor.undo_stack.len(), 1);
    editor.record_history(editor.program.clone(), false);
    editor.restore_history(true);
    assert_eq!(editor.program, original);
    assert!(!editor.dirty);
    editor.restore_history(false);
    assert!(editor.dirty);
}

#[test]
fn mcp_edits_share_history_and_reject_invalid_move() {
    let mut editor = sample_editor();
    let original = editor.program.clone();
    let result = editor
        .call_mcp(
            "add_road",
            &serde_json::json!({
                "road_type":"normal_road","start_x":0,"start_y":0,"end_x":3,"end_y":1
            }),
        )
        .unwrap();
    let index = result["segment_index"].as_u64().unwrap();
    editor.record_history(original.clone(), false);
    let before_move = editor.program.clone();
    assert!(editor
        .call_mcp(
            "move_road",
            &serde_json::json!({
                "segment_index":index,"dx":1,"dy":2
            })
        )
        .is_ok());
    let moved = map::project(&editor.program)
        .segments
        .into_iter()
        .find(|segment| segment.source == SegmentSource::TopLevel(index as usize))
        .unwrap();
    assert_eq!(moved.start, WorldPoint { x: 1.0, y: 2.0 });
    assert_eq!(moved.end, WorldPoint { x: 4.0, y: 3.0 });
    editor.record_history(before_move.clone(), false);
    assert_eq!(editor.undo_stack.len(), 2);
    editor.restore_history(true);
    assert_eq!(editor.program, before_move);
    assert!(editor
        .call_mcp(
            "move_road",
            &serde_json::json!({
                "segment_index":index,"dx":"bad","dy":2
            })
        )
        .is_err());
    assert_eq!(editor.program, before_move);
    editor.restore_history(true);
    assert_eq!(editor.program, original);
}

#[test]
fn mcp_sets_and_reports_an_intersection() {
    let mut editor = sample_editor();
    editor.program = parse(
        "road street: carlane; end; center = map_point(0, 0); \
        west = map_point(-1, 0); east = map_point(1, 0); north = map_point(0, 1); \
        a = street(west, -0.7, 0, -0.3, 0, center); \
        b = street(center, 0.3, 0, 0.7, 0, east); \
        c = street(center, 0, 0.3, 0, 0.7, north);",
    )
    .unwrap();
    assert!(editor
        .call_mcp(
            "set_intersection",
            &serde_json::json!({
                "point_ref":"center", "kind":"four_way"
            })
        )
        .is_err());
    editor
        .call_mcp(
            "set_intersection",
            &serde_json::json!({
                "point_ref":"center", "kind":"three_way", "road_type":"street"
            }),
        )
        .unwrap();
    let state = editor
        .call_mcp("get_road_state", &serde_json::json!({}))
        .unwrap();
    assert_eq!(state["intersections"][0]["point_ref"], "center");
    assert_eq!(state["intersections"][0]["kind"], "three_way");
    assert_eq!(state["intersections"][0]["road_type"], "street");
    editor
        .call_mcp(
            "set_intersection",
            &serde_json::json!({
                "point_ref":"center", "kind":"none"
            }),
        )
        .unwrap();
    assert_eq!(
        editor
            .call_mcp("get_road_state", &serde_json::json!({}))
            .unwrap()["intersections"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn mcp_document_replacement_is_validated_and_undoable() {
    let mut editor = sample_editor();
    let original = editor.program.clone();
    let source = format!(
        "{}\nneighborhood_street(0, 0, 1, 1);\n",
        default_program().to_road_source()
    );
    let result = editor
        .call_mcp("replace_document", &serde_json::json!({"source": source}))
        .unwrap();
    assert_eq!(result["segments"], 1);
    editor.record_history(original.clone(), false);
    assert!(editor.dirty);
    let replacement = editor.program.clone();
    assert!(editor
        .call_mcp(
            "replace_document",
            &serde_json::json!({"source": "road broken:"})
        )
        .is_err());
    assert_eq!(editor.program, replacement);
    editor.restore_history(true);
    assert_eq!(editor.program, original);
}

#[test]
fn mcp_add_road_reuses_a_named_junction() {
    let mut editor = sample_editor();
    editor.program =
        parse("road lane: sidewalk-carlane-sidewalk; end; a = map_point(0, 0);").unwrap();
    let result = editor
        .call_mcp(
            "add_road",
            &serde_json::json!({
                "road_type":"lane", "name":"curved_lane", "start_ref":"a",
                "start_x":0, "start_y":0, "end_x":2, "end_y":1,
                "control1_x":0.5, "control1_y":0, "control2_x":1.5, "control2_y":1
            }),
        )
        .unwrap();
    let segment = map::project(&editor.program)
        .segments
        .into_iter()
        .find(|segment| {
            segment.source
                == SegmentSource::TopLevel(result["segment_index"].as_u64().unwrap() as usize)
        })
        .unwrap();
    assert_eq!(segment.name.as_deref(), Some("curved_lane"));
    assert_eq!(segment.start_ref.as_deref(), Some("a"));
    assert_eq!(segment.controls[0], WorldPoint { x: 0.5, y: 0.0 });
    let before = editor.program.clone();
    assert!(editor
        .call_mcp(
            "add_road",
            &serde_json::json!({
                "road_type":"lane", "start_ref":"a", "start_x":1, "start_y":0,
                "end_x":3, "end_y":1
            })
        )
        .is_err());
    assert_eq!(editor.program, before);
}

#[test]
fn names_follow_language_rules() {
    assert!(RoadEditor::valid_name("road_2"));
    assert!(!RoadEditor::valid_name("2_road"));
    assert!(!RoadEditor::valid_name("road"));
    assert!(!RoadEditor::valid_name("road-name"));
}

#[test]
fn default_library_has_city_rural_and_interstate_profiles() {
    let program = default_program();
    let mut names = std::collections::HashSet::new();
    for statement in &program.statements {
        let Statement::Road(road) = statement else {
            panic!("default library must contain only roads")
        };
        assert!(!road.elements.is_empty());
        assert!(
            names.insert(&road.name),
            "duplicate preset name: {}",
            road.name
        );
    }
    assert!(program.statements.len() >= 12);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn target_boot_map_projects_all_road_placements_and_junctions() {
    let program = boot_program();
    let projection = map::project(&program);
    assert_eq!(projection.unresolved, 0);
    assert!(projection.segments.len() >= 700);
    assert!(map::intersections(&program, &projection).len() >= 200);
    let landscape = project_landscape_features(&program);
    assert!(landscape.errors.is_empty());
    let buildings = landscape
        .features
        .iter()
        .filter(|feature| feature.kind == LandscapeKind::Building)
        .count();
    assert!(buildings >= 400, "the boot map should fill the city grids");
    assert_eq!(landscape.features.len(), buildings + 56);
    assert_eq!(
        landscape
            .features
            .iter()
            .filter(|feature| feature.kind == LandscapeKind::Land)
            .count(),
        7
    );
    assert_eq!(
        landscape
            .features
            .iter()
            .filter(|feature| feature.kind == LandscapeKind::Beach)
            .count(),
        7
    );
    let concrete: std::collections::HashSet<_> = landscape
        .features
        .iter()
        .filter(|feature| feature.kind == LandscapeKind::Concrete)
        .filter_map(|feature| feature.name.as_deref())
        .collect();
    assert_eq!(concrete.len(), 34);
    for name in [
        "west_port_north_landing",
        "west_port_south_quay",
        "west_industrial_inner_1",
        "airport_primary_shoulder",
        "airport_terminal_hardstand",
        "east_port_main_apron",
        "east_port_long_pier",
        "east_industrial_west_1",
    ] {
        assert!(concrete.contains(name), "missing concrete area {name}");
    }
    let issues = design_rule_issues(
        &program,
        &projection,
        &paint_profiles(&program, &projection),
    );
    assert!(
        existing_design_checks_are_clear(&issues),
        "target design issues: {:?}",
        issues
            .iter()
            .map(|issue| (&issue.code, &issue.message))
            .take(12)
            .collect::<Vec<_>>()
    );
    for road_name in [
        "freeway",
        "ramp",
        "local",
        "airport_runway",
        "airport_taxiway",
        "airport_apron",
    ] {
        assert!(
            projection
                .segments
                .iter()
                .any(|segment| segment.road_name == road_name),
            "missing {road_name}"
        );
    }
}

#[test]
fn default_library_has_dedicated_transport_profiles() {
    let program = default_program();
    for (name, kind) in [
        ("dedicated_bike_path", RoadElementKind::BikeLane),
        ("train_line", RoadElementKind::TrainTrack),
        ("tram_line", RoadElementKind::TramTrack),
        ("subway_line", RoadElementKind::SubwayTrack),
        ("airport_runway", RoadElementKind::Runway),
        ("airport_taxiway", RoadElementKind::Taxiway),
        ("airport_apron", RoadElementKind::Apron),
    ] {
        let road = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == name => Some(road),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            road.elements
                .iter()
                .filter(|element| element.kind == kind)
                .count(),
            if matches!(
                kind,
                RoadElementKind::Runway | RoadElementKind::Taxiway | RoadElementKind::Apron
            ) {
                1
            } else {
                2
            }
        );
        assert!(map::element_world_width(kind) > 0.0);
        assert!(!kind_label(kind).is_empty());
    }
}

#[test]
fn file_new_clears_the_demo_and_keeps_the_road_library() {
    let mut editor = sample_editor();
    editor.program = starter_program();
    editor.selected_endpoint = Some((map::project(&editor.program).segments[0].source, true));
    editor.dirty = true;
    editor.new_document();
    assert!(map::project(&editor.program).segments.is_empty());
    assert_eq!(editor.program, default_program());
    assert!(matches!(editor.map_tool, MapTool::Select));
    assert!(matches!(editor.camera_view, CameraView::Isometric));
    assert_eq!(editor.map_zoom, 55.0);
    editor.set_camera_view(CameraView::Top);
    assert!(matches!(editor.camera_view, CameraView::Top));
    assert_eq!(editor.map_zoom, 55.0);
    assert!(editor.selected_endpoint.is_none());
    assert!(!editor.dirty);
}

#[test]
fn opening_a_map_uses_isometric_view() {
    let mut editor = sample_editor();
    let program = editor.program.clone();
    editor.load_program(program, None);
    assert!(matches!(editor.camera_view, CameraView::Isometric));
    assert_eq!(editor.map_zoom, 55.0);
    editor.set_camera_view(CameraView::Top);
    assert!(matches!(editor.camera_view, CameraView::Top));
    assert_eq!(editor.map_zoom, 55.0);
}

#[test]
fn graphical_edits_preserve_other_statements_and_round_trip() {
    let mut editor = sample_editor();
    let rest = editor.program.statements[1..].to_vec();
    editor.selected_element = Some(0);
    editor.add_element();
    assert_eq!(
        editor.road().unwrap().elements[1].kind,
        RoadElementKind::BikeLane
    );
    editor.move_element(1);
    assert_eq!(
        editor.road().unwrap().elements[2].kind,
        RoadElementKind::BikeLane
    );
    editor.remove_element();
    editor.add_road();
    assert_eq!(&editor.program.statements[1..3], &rest);
    assert_eq!(
        parse(&editor.program.to_road_source()).unwrap(),
        editor.program
    );
}

#[test]
fn deleting_road_keeps_block_and_calls() {
    let mut editor = sample_editor();
    let rest = editor.program.statements[1..].to_vec();
    editor.remove_road();
    assert_eq!(editor.program.statements, rest);
    assert!(editor.selected_road.is_none());
    assert_eq!(
        parse(&editor.program.to_road_source()).unwrap(),
        editor.program
    );
}

#[test]
fn renaming_a_road_updates_calls_in_blocks() {
    let mut editor = sample_editor();
    editor
        .program
        .statements
        .push(Statement::Call(opencity::Call {
            assigns: Vec::new(),
            name: "intersection3".into(),
            args: vec![
                Argument::Identifier("junction".into()),
                Argument::Identifier("normal_road".into()),
            ],
        }));
    assert!(editor.rename_road("main_road"));
    let Statement::Block(block) = &editor.program.statements[1] else {
        panic!("expected block")
    };
    assert_eq!(block.calls[0].name, "main_road");
    assert_eq!(block.calls[1].name, "main_road");
    let Statement::Call(intersection) = editor.program.statements.last().unwrap() else {
        unreachable!()
    };
    assert_eq!(
        intersection.args[1],
        Argument::Identifier("main_road".into())
    );
    assert_eq!(
        parse(&editor.program.to_road_source()).unwrap(),
        editor.program
    );
}
