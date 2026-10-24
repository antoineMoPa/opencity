use super::*;

#[test]
fn intersection_palette_queries_preserve_five_priorities() {
    for (query, title) in [
        ("intersection", "Add three-way intersection"),
        ("junction", "Add four-way intersection"),
        ("tee", "Add three-way intersection"),
        ("cross", "Add four-way intersection"),
    ] {
        let mut editor = editor();
        let ctx = egui::Context::default();
        assert_eq!(
            editor.palette_commands()[5].title,
            "Add three-way intersection"
        );
        let _ = frame(&mut editor, &ctx, vec![palette_shortcut()]);
        let output = frame(&mut editor, &ctx, vec![egui::Event::Text(query.into())]);
        assert!(labels(&output).contains(&title.into()), "query {query}");
    }
}

#[test]
fn intersection_clicks_create_native_ports_and_one_undo_in_both_views() {
    for view in [CameraView::Top, CameraView::Isometric] {
        for kind in [
            map::IntersectionKind::ThreeWay,
            map::IntersectionKind::FourWay,
            map::IntersectionKind::RightTurn,
        ] {
            let mut editor = editor();
            editor.camera_view = view;
            let ctx = egui::Context::default();
            let _ = frame(&mut editor, &ctx, vec![]);
            let before = editor.program.clone();
            editor.execute_palette_command(&format!("intersection:{}", kind.call_name()), &ctx);
            let p = editor.map_rect.unwrap().center() + egui::vec2(70.0, 70.0);
            let preview = frame(&mut editor, &ctx, vec![egui::Event::PointerMoved(p)]);
            assert!(labels(&preview).contains(&"Click to place · R rotate · Esc cancel".into()));
            assert_eq!(editor.program, before);
            click(&mut editor, &ctx, p);
            let projection = map::project(&editor.program);
            assert_eq!(projection.unresolved, 0);
            let junctions = map::intersections(&editor.program, &projection);
            assert_eq!(junctions.len(), 1);
            assert_eq!(junctions[0].ports.len(), kind.arm_count());
            assert_eq!(junctions[0].road_name, "street");
            assert!(editor.map_tool == MapTool::Select);
            assert_eq!(editor.undo_stack.len(), 1);
            assert!(editor.restore_history(true));
            assert_eq!(editor.program, before);
        }
    }
}

#[test]
fn intersection_default_profile_is_lazy_cancelled_and_committed_atomically() {
    let mut editor = editor();
    editor.program = Program { statements: vec![] };
    editor.saved_program = editor.program.clone();
    editor.selected_road = None;
    editor.camera_view = CameraView::Top;
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    editor.execute_palette_command("intersection:intersection3", &ctx);
    assert_eq!(editor.road().unwrap().name, "neighborhood_street");
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
    assert!(editor.program.statements.is_empty());
    assert!(editor.minimal.pending_road.is_none());
    assert!(editor.undo_stack.is_empty());
    editor.execute_palette_command("intersection:intersection3", &ctx);
    let p = editor.map_rect.unwrap().center();
    click(&mut editor, &ctx, p);
    assert_eq!(editor.undo_stack.len(), 1);
    assert_eq!(
        editor
            .program
            .statements
            .iter()
            .filter(|s| matches!(s, Statement::Road(_)))
            .count(),
        1
    );
    assert_eq!(map::project(&editor.program).unresolved, 0);
    editor.restore_history(true);
    assert!(editor.program.statements.is_empty());
}

#[test]
fn intersection_raised_terrain_rejects_without_materializing_profile() {
    let mut editor = editor();
    editor.program = parse("hill=terrain(2,-10,-10,10,-10,10,10,-10,10);").unwrap();
    editor.saved_program = editor.program.clone();
    editor.selected_road = None;
    editor.camera_view = CameraView::Top;
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let before = editor.program.clone();
    editor.execute_palette_command("intersection:intersection4", &ctx);
    let p = editor.map_rect.unwrap().center();
    click(&mut editor, &ctx, p);
    assert_eq!(editor.program, before);
    assert!(editor.undo_stack.is_empty());
    assert!(editor.status.contains("ground level"));
}

#[test]
fn intersection_vehicle_command_returns_to_last_edit_view() {
    let mut editor = editor();
    editor.camera_view = CameraView::Vehicular;
    editor.minimal.last_edit_view = Some(CameraView::Top);
    editor.execute_palette_command("intersection:intersection4", &egui::Context::default());
    assert!(editor.camera_view == CameraView::Top);
    assert!(editor.map_tool == MapTool::Intersection(map::IntersectionKind::FourWay));
    assert!(editor.camera_transition.is_some());
}

#[test]
fn intersection_rotation_is_preview_only_and_ports_remain_connectable() {
    let mut editor = editor();
    editor.camera_view = CameraView::Top;
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let before = editor.program.clone();
    editor.execute_palette_command("intersection:intersection3", &ctx);
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::R)]);
    assert_eq!(editor.program, before);
    assert!(editor.undo_stack.is_empty());
    let p = editor.map_rect.unwrap().center() + egui::vec2(70.0, 70.0);
    click(&mut editor, &ctx, p);
    let projection = map::project(&editor.program);
    let junction = map::intersections(&editor.program, &projection).remove(0);
    assert!(junction.ports.iter().any(|p| p.direction.y == -1.0));
    assert!(!junction.ports.iter().any(|p| p.direction.x == -1.0));
    let port = &junction.ports[0];
    let end = WorldPoint {
        x: port.position.x + port.direction.x,
        y: port.position.y + port.direction.y,
    };
    let road = editor.road().unwrap().clone();
    map::append_connected(
        &mut editor.program,
        &road,
        port.position,
        Some(&port.point_ref),
        end,
        None,
    );
    assert_eq!(map::project(&editor.program).unresolved, 0);
    assert!(map::port_in_use(
        &map::project(&editor.program),
        &port.point_ref
    ));
}

#[test]
fn intersection_terrain_footprint_checks_edges_and_isometric_visible_top() {
    let terrain = parse("terrain(0.5,0.1,-0.2,0.2,-0.2,0.2,0.2,0.1,0.2);").unwrap();
    let origin = WorldPoint { x: 0.0, y: 0.0 };
    assert_eq!(terrain_placement::height(&terrain, origin), 0.0);
    assert!(terrain_placement::intersection_obstructed(
        &terrain, origin, 0.15
    ));
    let mut editor = editor();
    editor.program = parse("terrain(0.5,-10,-10,10,-10,10,10,-10,10);").unwrap();
    editor.saved_program = editor.program.clone();
    editor.selected_road = None;
    editor.camera_view = CameraView::Isometric;
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let before = editor.program.clone();
    editor.execute_palette_command("intersection:intersection3", &ctx);
    let p = editor.map_rect.unwrap().center();
    let output = frame(&mut editor, &ctx, vec![egui::Event::PointerMoved(p)]);
    assert!(labels(&output).contains(
        &"Intersections need ground level; choose a place clear of raised terrain".into()
    ));
    click(&mut editor, &ctx, p);
    assert_eq!(editor.program, before);
    assert!(editor.undo_stack.is_empty());
}

#[test]
fn intersection_existing_road_and_junction_overlap_reject_atomically() {
    for view in [CameraView::Top, CameraView::Isometric] {
        let mut editor = editor();
        editor.camera_view = view;
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        let rect = editor.map_rect.unwrap();
        let at = |editor: &RoadEditor, point| {
            if view == CameraView::Top {
                screen_of(rect, editor.map_pan, editor.map_zoom, point)
            } else {
                iso_screen(
                    rect,
                    editor.map_pan,
                    editor.map_zoom,
                    editor.orbit_target,
                    editor.orbit_angle,
                    editor.orbit_elevation,
                    point,
                    0.0,
                )
            }
        };
        let before = editor.program.clone();
        editor.execute_palette_command("intersection:intersection4", &ctx);
        let p = at(&editor, WorldPoint { x: 0.0, y: 0.0 });
        click(&mut editor, &ctx, p);
        assert_eq!(editor.program, before);
        assert!(editor.undo_stack.is_empty());
        assert!(editor.status.contains("empty site"));
        let p = at(&editor, WorldPoint { x: 0.0, y: -1.0 });
        click(&mut editor, &ctx, p);
        assert_eq!(editor.undo_stack.len(), 1);
        let placed = editor.program.clone();
        editor.execute_palette_command("intersection:intersection3", &ctx);
        click(&mut editor, &ctx, p);
        assert_eq!(editor.program, placed);
        assert_eq!(editor.undo_stack.len(), 1);
    }
}
