use super::*;

#[test]
fn annotation_placement_and_projection_follow_all_camera_views() {
    for view in [CameraView::Top, CameraView::Isometric] {
        let mut editor = editor();
        editor.camera_view = view;
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        editor.execute_palette_command("annotation-text", &ctx);
        click(&mut editor, &ctx, egui::pos2(600.0, 450.0));
        let _ = frame(&mut editor, &ctx, vec![]);
        let _ = frame(
            &mut editor,
            &ctx,
            vec![egui::Event::Text("world label".into())],
        );
        let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Enter)]);
        let output = frame(&mut editor, &ctx, vec![]);
        let p = text_position(&output, "world label");
        editor.map_pan += egui::vec2(23.0, 17.0);
        let output = frame(&mut editor, &ctx, vec![]);
        assert!(
            (text_position(&output, "world label") - p - egui::vec2(23.0, 17.0)).length() < 0.01
        );
    }
    let mut editor = editor();
    editor.program =
        parse("front=annotation_text(\"front\",1,0.25); back=annotation_text(\"behind\",-1,0);")
            .unwrap();
    editor.saved_program = editor.program.clone();
    editor.camera_view = CameraView::Vehicular;
    editor.vehicle_position = WorldPoint { x: 0.0, y: 0.0 };
    editor.vehicle_heading = 0.0;
    editor.vehicle_eye_height = 0.1;
    let ctx = egui::Context::default();
    let output = frame(&mut editor, &ctx, vec![]);
    assert!(!labels(&output).contains(&"behind".into()));
    let projected = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.job.text == "front" => Some(t.pos),
            _ => None,
        })
        .unwrap();
    let rect = editor.map_rect.unwrap();
    assert!(
        (projected - (rect.center() + egui::vec2(0.25, 0.1) * (rect.height() * 0.714))).length()
            < 0.01
    );
    editor.execute_palette_command("box-select", &ctx);
    assert!(editor.camera_view == CameraView::Isometric);
    assert!(editor.map_tool == MapTool::BoxSelect);
    editor.set_camera_view(CameraView::Vehicular);
    assert_eq!(editor.vehicle_position, WorldPoint { x: 0.0, y: 0.0 });
}

#[test]
fn landscape_minimum_commit_cancel_and_history() {
    for (name, minimum) in [("terrain", 3), ("concrete", 3), ("river", 2)] {
        let mut editor = editor();
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        let original = editor.program.clone();
        editor.execute_palette_command(&format!("landscape:{name}"), &ctx);
        click(&mut editor, &ctx, egui::pos2(600.0, 450.0));
        let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Enter)]);
        assert_eq!(editor.program, original);
        assert_eq!(editor.landscape_points.len(), 1);
        click(&mut editor, &ctx, egui::pos2(760.0, 450.0));
        if minimum == 3 {
            click(&mut editor, &ctx, egui::pos2(700.0, 550.0));
        }
        let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Enter)]);
        assert_eq!(
            editor.program.statements.len(),
            original.statements.len() + 1
        );
        assert_eq!(editor.undo_stack.len(), 1);
        assert!(editor.map_tool == MapTool::Select);
        assert!(editor.restore_history(true));
        assert_eq!(editor.program, original);
        assert!(editor.restore_history(false));
        let committed = editor.program.clone();
        editor.execute_palette_command(&format!("landscape:{name}"), &ctx);
        click(&mut editor, &ctx, egui::pos2(600.0, 450.0));
        let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
        assert_eq!(editor.program, committed);
        assert!(editor.landscape_points.is_empty());
    }
}

#[test]
fn annotation_segment_preview_commit_history_cancel() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let original = editor.program.clone();
    editor.execute_palette_command("annotation-segment", &ctx);
    click(&mut editor, &ctx, egui::pos2(600.0, 450.0));
    assert_eq!(editor.program, original);
    let output = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::PointerMoved(egui::pos2(760.0, 500.0))],
    );
    assert!(output.shapes.iter().any(|s| matches!(&s.shape,
        egui::Shape::LineSegment{
        stroke,
        ..
        }
         if stroke.color==egui::Color32::from_rgb(240,
        35,
        45))));

    click(&mut editor, &ctx, egui::pos2(760.0, 500.0));
    assert!(editor.map_tool == MapTool::Select);
    assert_eq!(editor.undo_stack.len(), 1);
    assert_eq!(map::project(&editor.program).unresolved, 0);
    assert!(editor
        .program
        .to_road_source()
        .contains("annotation_segment_1 = annotation_segment("));
    assert_eq!(
        parse(&editor.program.to_road_source()).unwrap(),
        editor.program
    );
    let output = frame(&mut editor, &ctx, vec![]);
    let red_line = |output: &egui::FullOutput| {
        output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::LineSegment { points, stroke }
                    if stroke.color == egui::Color32::from_rgb(240, 35, 45) =>
                {
                    Some(*points)
                }
                _ => None,
            })
            .unwrap()
    };
    let before = red_line(&output);
    editor.map_pan += egui::vec2(30.0, 15.0);
    let output = frame(&mut editor, &ctx, vec![]);
    let after = red_line(&output);
    for i in 0..2 {
        assert!((after[i] - before[i] - egui::vec2(30.0, 15.0)).length() < 0.01);
    }
    let committed = editor.program.clone();

    assert!(editor.restore_history(true));
    assert_eq!(editor.program, original);
    assert!(editor.restore_history(false));
    assert_eq!(editor.program, committed);
    editor.execute_palette_command("annotation-segment", &ctx);
    click(&mut editor, &ctx, egui::pos2(600.0, 450.0));
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
    assert_eq!(editor.program, committed);
    assert!(editor.minimal.annotation_start.is_none());
}

#[test]
fn annotation_text_roundtrip_color_and_world_anchor() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    editor.execute_palette_command("annotation-text", &ctx);
    click(&mut editor, &ctx, egui::pos2(600.0, 450.0));
    let _ = frame(&mut editor, &ctx, vec![]);
    let camera = (
        editor.map_pan,
        editor.orbit_angle,
        editor.orbit_elevation,
        editor.map_zoom,
    );
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::ArrowUp)]);
    assert_eq!(
        camera,
        (
            editor.map_pan,
            editor.orbit_angle,
            editor.orbit_elevation,
            editor.map_zoom
        )
    );
    let text = "Rouge 東京 \"quoted\" \\ path";
    let _ = frame(&mut editor, &ctx, vec![egui::Event::Text(text.into())]);
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Enter)]);
    assert!(editor.map_tool == MapTool::Select);
    assert_eq!(editor.undo_stack.len(), 1);
    assert_eq!(
        parse(&editor.program.to_road_source()).unwrap(),
        editor.program
    );
    let output = frame(&mut editor, &ctx, vec![]);
    let before = text_position(&output, text);
    assert!(output.shapes.iter().any(|s| matches!(&s.shape,
        egui::Shape::Text(t) if t.galley.job.text==text &&
         t.fallback_color==egui::Color32::from_rgb(240,
        35,
        45))));

    editor.map_pan += egui::vec2(35.0, 20.0);
    let output = frame(&mut editor, &ctx, vec![]);
    assert!((text_position(&output, text) - before - egui::vec2(35.0, 20.0)).length() < 0.01);
    assert_eq!(map::project(&editor.program).segments.len(), 2);
}

#[test]
fn annotation_text_empty_cancel_palette_and_document_reset() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let original = editor.program.clone();
    for action in [egui::Key::Enter, egui::Key::Escape] {
        editor.execute_palette_command("annotation-text", &ctx);
        click(&mut editor, &ctx, egui::pos2(600.0, 450.0));
        let _ = frame(&mut editor, &ctx, vec![]);
        let _ = frame(
            &mut editor,
            &ctx,
            vec![egui::Event::Text("   ".into()), key(action)],
        );
        assert_eq!(editor.program, original);
        assert!(editor.minimal.annotation_start.is_none());
    }
    editor.execute_palette_command("annotation-text", &ctx);
    click(&mut editor, &ctx, egui::pos2(600.0, 450.0));
    let _ = frame(&mut editor, &ctx, vec![]);
    let _ = frame(&mut editor, &ctx, vec![egui::Event::Text("Draft".into())]);
    let _ = frame(&mut editor, &ctx, vec![palette_shortcut()]);
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::Text("concrete".into())],
    );
    assert_eq!(editor.minimal.annotation_text, "Draft");
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
    assert_eq!(editor.minimal.annotation_text, "Draft");
    assert_eq!(editor.program, original);
    let _ = frame(&mut editor, &ctx, vec![palette_shortcut()]);
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::Text("concrete".into())],
    );
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Enter)]);
    assert!(editor.map_tool == MapTool::Landscape(LandscapeKind::Concrete));
    assert!(editor.minimal.annotation_start.is_none());
    assert!(editor.landscape_points.is_empty());
    assert_eq!(editor.program, original);
    let _ = frame(&mut editor, &ctx, vec![]);
    click(&mut editor, &ctx, egui::pos2(600.0, 450.0));
    assert_eq!(editor.landscape_points.len(), 1);
    editor.load_program(original.clone(), None);
    assert!(editor.minimal.annotation_start.is_none());
    assert!(editor.map_tool == MapTool::Select);
}

#[test]
fn placement_switch_prunes_unused_road_start_and_leaves_empty_canvas() {
    for command in ["landscape:concrete", "annotation-segment"] {
        let mut editor = editor();
        let ctx = egui::Context::default();
        let original = editor.program.clone();
        let source = map::project(&editor.program).segments[0].source;
        let (reference, point) =
            map::create_lane_point(&mut editor.program, source, 0, 0.5).unwrap();
        editor.map_tool = MapTool::Draw;
        editor.map_start = Some(point);
        editor.map_start_ref = Some(reference);
        editor.execute_palette_command(command, &ctx);
        assert_eq!(editor.program, original);
        assert!(editor.map_start_ref.is_none());
    }
    let mut editor = editor();
    let ctx = egui::Context::default();
    editor.selected_segment = None;
    editor.selected_segments.clear();
    let _ = frame(&mut editor, &ctx, vec![]);
    let output = frame(&mut editor, &ctx, vec![]);
    assert!(labels(&output).is_empty());
    assert!(editor.minimal.toolbar_rect.is_none());
}
