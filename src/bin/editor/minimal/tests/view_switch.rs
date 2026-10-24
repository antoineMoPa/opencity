use super::*;

#[test]
fn hover_view_switch_pointer_and_tab_preserve_query_and_camera() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let original = editor.program.clone();
    let _ = frame(&mut editor, &ctx, vec![palette_shortcut()]);
    let _ = frame(&mut editor, &ctx, vec![egui::Event::Text("highway".into())]);
    for (view, word) in [
        (CameraView::Top, "Top"),
        (CameraView::Vehicular, "Vehicular"),
        (CameraView::Isometric, "Isometric"),
    ] {
        let _ = frame(
            &mut editor,
            &ctx,
            vec![egui::Event::PointerMoved(egui::pos2(890.0, 50.0))],
        );
        let output = frame(&mut editor, &ctx, vec![]);
        click(&mut editor, &ctx, text_position(&output, word));
        assert!(editor.camera_view == view);
        assert!(editor.command_palette.is_open());
        let output = frame(&mut editor, &ctx, vec![]);
        assert!(labels(&output).contains(&"highway".into()));
        assert!(labels(&output).contains(&"Add highway".into()));
        assert_eq!(editor.program, original);
    }
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Tab)]);
    assert!(editor.camera_view == CameraView::Vehicular);
    editor.camera_transition = None;
    editor.vehicle_position = WorldPoint { x: 7.0, y: 8.0 };
    editor.vehicle_heading = 0.7;
    editor.set_camera_view(CameraView::Top);
    editor.camera_transition = None;
    editor.set_camera_view(CameraView::Vehicular);
    assert_eq!(editor.vehicle_position, WorldPoint { x: 7.0, y: 8.0 });
    assert_eq!(editor.vehicle_heading, 0.7);
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
    let _ = frame(&mut editor, &ctx, vec![]);
    assert!(editor.camera_view == CameraView::Vehicular);
}

#[test]
fn narrow_hover_switch_stays_inside_viewport_and_preserves_palette_query() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let size = egui::vec2(270.0, 520.0);
    let before = editor.program.clone();
    let _ = frame_sized(&mut editor, &ctx, vec![palette_shortcut()], size);
    let _ = frame_sized(
        &mut editor,
        &ctx,
        vec![egui::Event::Text("highway".into())],
        size,
    );
    let _ = frame_sized(
        &mut editor,
        &ctx,
        vec![egui::Event::PointerMoved(egui::pos2(260.0, 50.0))],
        size,
    );
    let output = frame_sized(&mut editor, &ctx, vec![], size);
    let query = text_position(&output, "highway");
    let top = text_position(&output, "Top");
    let switch = editor.minimal.view_switch_rect.unwrap();
    assert!(switch.contains(top));
    assert!(switch.left() >= 0.0 && switch.right() <= size.x);
    assert!(top.y < query.y);
    for word in ["Top", "Isometric", "Vehicular"] {
        let position = text_position(&output, word);
        assert!(position.x > 0.0 && position.x < size.x);
    }
    let pressed = frame_sized(
        &mut editor,
        &ctx,
        vec![egui::Event::PointerMoved(top), pointer(top, true)],
        size,
    );
    assert!(editor.command_palette.is_open(), "dismissed on press");
    assert!(
        (text_position(&pressed, "Top") - top).length() < 1.0,
        "moved {:?} -> {:?}",
        top,
        text_position(&pressed, "Top")
    );
    let _ = frame_sized(&mut editor, &ctx, vec![pointer(top, false)], size);
    assert!(
        editor.camera_view == CameraView::Top,
        "top={top:?}, query={query:?}, palette={:?}, labels={:?}",
        editor.command_palette.rect(),
        labels(&output)
    );
    assert!(editor.command_palette.is_open());
    let output = frame_sized(&mut editor, &ctx, vec![], size);
    assert!(labels(&output).contains(&"highway".into()));
    assert_eq!(editor.program, before);
}

#[test]
fn view_switch_reveal_and_pointer_ownership_keep_canvas_usable() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let output = frame(&mut editor, &ctx, vec![]);
    assert!(editor.minimal.view_switch_rect.is_none());
    assert!(!labels(&output).contains(&"Top".into()));
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::PointerMoved(egui::pos2(850.0, 99.0))],
    );
    let output = frame(&mut editor, &ctx, vec![]);
    let rect = editor.minimal.view_switch_rect.unwrap();
    assert!((rect.right() - 888.0).abs() < 1.0);
    assert!((rect.top() - 12.0).abs() < 1.0);
    assert_eq!(labels(&output).iter().filter(|v| *v == "Top").count(), 1);
    editor.execute_palette_command("road:wide", &ctx);
    click(&mut editor, &ctx, rect.center());
    assert!(editor.map_start.is_none());
    assert!(editor.box_start.is_none());
    let _ = frame(&mut editor, &ctx, vec![pointer(rect.center(), true)]);
    let below = egui::pos2(850.0, 150.0);
    let _ = frame(&mut editor, &ctx, vec![egui::Event::PointerMoved(below)]);
    assert!(
        editor.minimal.view_switch_rect.is_some(),
        "keep the initiating gesture visible"
    );
    let _ = frame(&mut editor, &ctx, vec![pointer(below, false)]);
    assert!(editor.map_start.is_none());
    let _ = frame(&mut editor, &ctx, vec![]);
    assert!(editor.minimal.view_switch_rect.is_none());
    click(&mut editor, &ctx, egui::pos2(450.0, 70.0));
    assert!(
        editor.map_start.is_some(),
        "reveal strip remains usable canvas"
    );
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::PointerMoved(egui::pos2(850.0, 101.0))],
    );
    assert!(editor.minimal.view_switch_rect.is_none());
    editor.cancel_placement();
    editor.file_action(FileAction::New);
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::PointerMoved(rect.center())],
    );
    let _ = frame(&mut editor, &ctx, vec![]);
    click(
        &mut editor,
        &ctx,
        egui::pos2(rect.left() + 10.0, rect.center().y),
    );
    assert!(editor.camera_view == CameraView::Isometric);
    assert!(editor.pending.is_some());
}

#[test]
fn existing_canvas_box_drag_keeps_ownership_when_crossing_view_switch() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::PointerMoved(egui::pos2(850.0, 50.0))],
    );
    let _ = frame(&mut editor, &ctx, vec![]);
    let switch = editor.minimal.view_switch_rect.unwrap();
    editor.execute_palette_command("box-select", &ctx);
    let start = egui::pos2(450.0, 350.0);
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
    );
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::PointerMoved(switch.center())],
    );
    assert!(!editor.minimal.view_switch_gesture);
    assert!(editor.box_start.is_some());
    let _ = frame(&mut editor, &ctx, vec![pointer(switch.center(), false)]);
    assert!(editor.box_start.is_none());
    assert!(editor.map_tool == MapTool::Select);
    assert!(editor.camera_view == CameraView::Isometric);
    assert!(!editor.dirty);
}

#[test]
fn vehicular_translation_is_slow_time_based_and_blocked_by_overlays() {
    for dt in [1.0 / 60.0, 1.0 / 30.0] {
        for (key, sign) in [(egui::Key::W, 1.0), (egui::Key::S, -1.0)] {
            let mut editor = editor();
            editor.set_camera_view(CameraView::Vehicular);
            editor.vehicle_position = WorldPoint { x: 0.0, y: 0.0 };
            editor.vehicle_heading = 0.0;
            let ctx = egui::Context::default();
            let run = |editor: &mut RoadEditor, events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 650.0),
                        )),
                        predicted_dt: dt,
                        events,
                        ..Default::default()
                    },
                    |ui| editor.editor_frame_ui(ui),
                );
                output.textures_delta.clear();
            };
            run(
                &mut editor,
                vec![egui::Event::PointerMoved(egui::pos2(450.0, 350.0))],
            );
            run(&mut editor, vec![self::key(key)]);
            assert!(
                (editor.vehicle_position.x - sign * 0.3 * dt).abs() < 0.00001,
                "actual {:?} transition {} wants {}",
                editor.vehicle_position,
                editor.camera_transition.is_some(),
                ctx.egui_wants_keyboard_input()
            );
            assert_eq!(editor.vehicle_position.y, 0.0);
            let stopped = editor.vehicle_position;
            run(&mut editor, vec![palette_shortcut()]);
            run(&mut editor, vec![]);
            assert_eq!(editor.vehicle_position, stopped);
            run(&mut editor, vec![self::key(egui::Key::Escape)]);
            editor.file_action(FileAction::New);
            run(&mut editor, vec![]);
            assert_eq!(editor.vehicle_position, stopped);
        }
    }
}

#[test]
fn vehicular_wheel_uses_reduced_translation_rate() {
    let mut editor = editor();
    editor.set_camera_view(CameraView::Vehicular);
    editor.vehicle_position = WorldPoint { x: 0.0, y: 0.0 };
    editor.vehicle_heading = 0.0;
    let ctx = egui::Context::default();
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::PointerMoved(egui::pos2(450.0, 350.0))],
    );
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 10.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
    assert!(scroll > 0.0);
    assert!((editor.vehicle_position.x - scroll * 0.0003).abs() < 0.000001);
    assert!(editor.vehicle_position.x < 0.0031);
    assert_eq!(editor.vehicle_position.y, 0.0);
}
