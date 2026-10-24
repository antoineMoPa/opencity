use super::*;

#[test]
fn point_options_click_lane_branch_and_cancel_in_both_views() {
    for iso in [false, true] {
        let mut editor = editor();
        editor.program = parse(
            "road street: carlane-middle-carlane; end; main=street(-2,0,2,0); grade(main,0.2,0.5);",
        )
        .unwrap();
        editor.saved_program = editor.program.clone();
        editor.selected_segment = Some(map::project(&editor.program).segments[0].source);
        editor.camera_view = if iso {
            CameraView::Isometric
        } else {
            CameraView::Top
        };
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        let projection = map::project(&editor.program);
        let segment = &projection.segments[0];
        let road = road_for_segment(&editor.program, segment).unwrap();
        let point = map::element_position(segment, road, 2, 1.0).unwrap();
        let screen = editor.spatial_screen(editor.map_rect.unwrap(), point, segment.end_z, iso);
        let original = editor.program.clone();
        click(&mut editor, &ctx, screen);
        assert!(editor.point_options.is_some(), "iso={iso}");
        let _ = frame(&mut editor, &ctx, vec![]);
        let output = frame(&mut editor, &ctx, vec![]);
        let branch = text_position(&output, "Branch from lane");
        click(&mut editor, &ctx, branch);
        assert!(editor.map_tool == MapTool::Draw);
        assert_eq!(
            editor.program, original,
            "placement start stays outside source"
        );
        assert!(editor.map_start_ref.is_some());
        let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
        assert_eq!(editor.program, original);
        assert!(editor.undo_stack.is_empty());
        click(&mut editor, &ctx, screen);
        let _ = frame(
            &mut editor,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
        assert!(editor.point_options.is_none());
        assert_eq!(editor.program, original);
        click(&mut editor, &ctx, screen);
        let _ = frame(&mut editor, &ctx, vec![]);
        let output = frame(&mut editor, &ctx, vec![]);
        click(
            &mut editor,
            &ctx,
            text_position(&output, "Branch from lane"),
        );
        let destination = editor.spatial_screen(
            editor.map_rect.unwrap(),
            WorldPoint { x: 4.0, y: 2.0 },
            0.0,
            iso,
        );
        click(&mut editor, &ctx, destination);
        assert!(
            map::project(&editor.program).segments.len() > 1,
            "iso={iso}: {}",
            editor.status
        );
        assert_eq!(editor.undo_stack.len(), 1);
        assert!(editor.restore_history(true));
        assert_eq!(editor.program, original);
    }
}

#[test]
fn point_options_low_zoom_lane_identity_and_blank_dismissal() {
    for iso in [false, true] {
        let mut editor = editor();
        editor.program =
            parse("road street: carlane-middle-carlane; end; main=street(-2,0,2,0);").unwrap();
        editor.selected_segment = Some(map::project(&editor.program).segments[0].source);
        editor.map_zoom = 28.0;
        editor.camera_view = if iso {
            CameraView::Isometric
        } else {
            CameraView::Top
        };
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        let projection = map::project(&editor.program);
        let segment = &projection.segments[0];
        let point = map::element_position(
            segment,
            road_for_segment(&editor.program, segment).unwrap(),
            2,
            1.0,
        )
        .unwrap();
        let screen = editor.spatial_screen(editor.map_rect.unwrap(), point, 0.0, iso);
        click(&mut editor, &ctx, screen);
        let _ = frame(&mut editor, &ctx, vec![]);
        let output = frame(&mut editor, &ctx, vec![]);
        assert!(
            labels(&output).contains(&"Branch from lane".to_owned()),
            "iso={iso}"
        );
        let original = editor.program.clone();
        click(&mut editor, &ctx, egui::pos2(30.0, 500.0));
        assert!(editor.point_options.is_none());
        assert_eq!(editor.program, original);
    }
}

#[test]
fn point_options_endpoint_extend_click_and_drag_have_distinct_results() {
    for iso in [false, true] {
        let mut editor = editor();
        editor.camera_view = if iso {
            CameraView::Isometric
        } else {
            CameraView::Top
        };
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        let segment = map::project(&editor.program).segments[0].clone();
        let screen =
            editor.spatial_screen(editor.map_rect.unwrap(), segment.end, segment.end_z, iso);
        let original = editor.program.clone();
        click(&mut editor, &ctx, screen);
        let _ = frame(&mut editor, &ctx, vec![]);
        let output = frame(&mut editor, &ctx, vec![]);
        click(&mut editor, &ctx, text_position(&output, "Extend road"));
        assert!(editor.map_tool == MapTool::Draw);
        assert_eq!(editor.program, original);
        let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
        let _ = frame(
            &mut editor,
            &ctx,
            vec![egui::Event::PointerMoved(screen), pointer(screen, true)],
        );
        let target = screen + egui::vec2(45.0, 15.0);
        let _ = frame(&mut editor, &ctx, vec![egui::Event::PointerMoved(target)]);
        let _ = frame(&mut editor, &ctx, vec![pointer(target, false)]);
        assert!(editor.point_options.is_none());
        assert_ne!(editor.program, original);
    }
}
