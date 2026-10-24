use super::*;

#[test]
fn box_select_top_and_iso_are_one_shot_and_escape_keeps_map_unchanged() {
    for view in [CameraView::Top, CameraView::Isometric] {
        let mut editor = editor();
        editor.camera_view = view;
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        let original = editor.program.clone();
        let pan = editor.map_pan;
        editor.execute_palette_command("box-select", &ctx);
        let a = egui::pos2(100.0, 100.0);
        let b = egui::pos2(850.0, 600.0);
        let _ = frame(
            &mut editor,
            &ctx,
            vec![egui::Event::PointerMoved(a), pointer(a, true)],
        );
        let _ = frame(&mut editor, &ctx, vec![egui::Event::PointerMoved(b)]);
        let _ = frame(&mut editor, &ctx, vec![pointer(b, false)]);
        assert!(!editor.selected_segments.is_empty());
        assert!(editor.map_tool == MapTool::Select);
        assert_eq!(editor.program, original);
        assert_eq!(editor.map_pan, pan);
        assert!(editor.undo_stack.is_empty());
        assert!(!editor.dirty);
        editor.execute_palette_command("box-select", &ctx);
        let _ = frame(
            &mut editor,
            &ctx,
            vec![egui::Event::PointerMoved(a), pointer(a, true)],
        );
        let _ = frame(&mut editor, &ctx, vec![egui::Event::PointerMoved(b)]);
        let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
        let _ = frame(&mut editor, &ctx, vec![pointer(b, false)]);
        assert!(editor.box_start.is_none());
        assert_eq!(editor.program, original);
        assert!(editor.undo_stack.is_empty());
    }
}

#[test]
fn minimal_two_ground_clicks_add_one_road_and_undo() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let _ = frame(&mut editor, &ctx, vec![]);
    let _ = frame(&mut editor, &ctx, vec![palette_shortcut()]);
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::Text("Add road: wide".into())],
    );
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Enter)]);
    let _ = frame(&mut editor, &ctx, vec![]);
    assert_eq!(editor.road().unwrap().name, "wide");

    let original = editor.program.clone();
    click(&mut editor, &ctx, egui::pos2(600.0, 500.0));
    assert!(editor.map_start.is_some());
    click(&mut editor, &ctx, egui::pos2(750.0, 550.0));
    assert_eq!(map::project(&editor.program).segments.len(), 3);
    assert_eq!(
        map::project(&editor.program)
            .segments
            .last()
            .unwrap()
            .road_name,
        "wide"
    );
    assert!(editor.map_tool == MapTool::Select);
    assert!(editor.map_start.is_none());
    assert_eq!(editor.undo_stack.len(), 1);
    assert!(editor.restore_history(true));
    assert_eq!(editor.program, original);
}

#[test]
fn minimal_full_frame_endpoint_and_curve_drags_each_have_one_undo() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let _ = frame(&mut editor, &ctx, vec![]);
    for control in [false, true] {
        let before = editor.program.clone();
        let segment = map::project(&editor.program).segments[0].clone();
        let point = if control {
            segment.controls[0]
        } else {
            segment.start
        };
        let start = editor.spatial_screen(editor.map_rect.unwrap(), point, 0.0, true);
        let target = start + egui::vec2(30.0, 25.0);
        let history = editor.undo_stack.len();
        let _ = frame(
            &mut editor,
            &ctx,
            vec![egui::Event::PointerMoved(start), pointer(start, true)],
        );
        let _ = frame(&mut editor, &ctx, vec![egui::Event::PointerMoved(target)]);
        let _ = frame(&mut editor, &ctx, vec![pointer(target, false)]);
        assert_ne!(editor.program, before, "control={control}");
        assert_eq!(editor.undo_stack.len(), history + 1, "control={control}");
        let changed = map::project(&editor.program).segments[0].clone();
        if control {
            assert_ne!(changed.controls[0], segment.controls[0]);
            assert_eq!(changed.start, segment.start);
        } else {
            assert_ne!(changed.start, segment.start);
        }
        assert!(editor.restore_history(true));
        assert_eq!(editor.program, before);
        editor.selected_segment = Some(segment.source);
        editor.selected_segments = vec![segment.source];
        let _ = frame(&mut editor, &ctx, vec![]);
    }
}
