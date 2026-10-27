use super::*;

fn start_menu(
    editor: &mut RoadEditor,
    ctx: &egui::Context,
    position: egui::Pos2,
) -> egui::FullOutput {
    let _ = frame(editor, ctx, vec![egui::Event::PointerMoved(position)]);
    let button = |pressed| egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let _ = frame(editor, ctx, vec![button(true)]);
    let _ = frame(editor, ctx, vec![button(false)]);
    frame(editor, ctx, vec![])
}

#[test]
fn external_launch_top_view_maps_right_click_and_copies_coordinates() {
    let mut editor = editor();
    editor.camera_view = CameraView::Top;
    editor.map_pan = egui::vec2(37., -21.);
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let rect = editor.map_rect.unwrap();
    let position = rect.center() + egui::vec2(129., -95.);
    let expected = world_of(rect, editor.map_pan, editor.map_zoom, position);
    let output = start_menu(&mut editor, &ctx, position);
    assert!(labels(&output).contains(&format!(
        "Coordinates: {:.3}, {:.3}",
        expected.x, expected.y
    )));
    assert!(!labels(&output).contains(&"Inspect here".into()));
    let copy = text_position(&output, "Copy coordinates");
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::PointerMoved(copy), pointer(copy, true)],
    );
    let output = frame(&mut editor, &ctx, vec![pointer(copy, false)]);
    assert!(output
        .platform_output
        .commands
        .iter()
        .any(|command| matches!(command,
            egui::OutputCommand::CopyText(text) if text == &format!("{},{}", expected.x, expected.y)
        )));
}

#[test]
#[cfg(unix)]
fn external_launch_uses_unsaved_contents_and_reports_child_failure() {
    for camera in [CameraView::Top, CameraView::Isometric] {
        for saved in [false, true] {
            let mut editor = editor();
            editor.launch_action = Ok(Some(
            crate::external_launch::LaunchAction::parse(
                r#"{"label":"Inspect here","program":"/bin/sh","ready_message":"point=","args":["-c","cat \"$1\" >&2; printf '\\npoint=%s\\n' \"$2\" >&2; exit 7","viewer","{map}","{x},{y}"]}"#,
                PathBuf::from("/tmp"),
            )
            .unwrap(),
        ));
            editor.camera_view = camera;
            let original =
                std::env::temp_dir().join(format!("opencity-original-{}.road", std::process::id()));
            if saved {
                std::fs::write(&original, "original file must stay untouched").unwrap();
                editor.path = Some(original.clone());
            }
            editor.program.statements.extend(
                parse("note=annotation_text(\"unsaved\",0,0);")
                    .unwrap()
                    .statements,
            );
            let current = editor.program.clone();
            let saved_program = editor.saved_program.clone();
            let ctx = egui::Context::default();
            let _ = frame(&mut editor, &ctx, vec![]);
            let position = editor.map_rect.unwrap().center() + egui::vec2(121., -92.);
            let output = start_menu(&mut editor, &ctx, position);
            let launch = text_position(&output, "Inspect here");
            click(&mut editor, &ctx, launch);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !editor.status.contains("External application failed")
                && std::time::Instant::now() < deadline
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
                let _ = frame(&mut editor, &ctx, vec![]);
            }
            assert!(
                editor.status.contains("External application failed"),
                "{}",
                editor.status
            );
            assert!(editor.status.contains("unsaved"), "{}", editor.status);
            assert!(editor.status.contains("point="));
            assert_eq!(editor.program, current);
            assert_eq!(editor.saved_program, saved_program);
            assert!(editor.dirty);
            if saved {
                assert_eq!(
                    std::fs::read_to_string(&original).unwrap(),
                    "original file must stay untouched"
                );
                std::fs::remove_file(original).unwrap();
            } else {
                assert!(editor.path.is_none());
            }
            let output = frame(&mut editor, &ctx, vec![]);
            assert!(labels(&output)
                .iter()
                .any(|label| label.contains("External application failed")));
        }
    }
}

#[test]
fn external_launch_isometric_maps_panned_rotated_elevated_terrain() {
    let mut editor = editor();
    editor.program = parse("patch=terrain(1,-10,-10,10,-10,10,10,-10,10);").unwrap();
    editor.map_pan = egui::vec2(37., -21.);
    editor.orbit_angle = 1.2;
    editor.orbit_elevation = 0.8;
    editor.orbit_target = WorldPoint { x: 2., y: -1. };
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let rect = editor.map_rect.unwrap();
    let position = rect.center() + egui::vec2(129., -95.);
    let expected = iso_world_at_height(
        rect,
        editor.map_pan,
        editor.map_zoom,
        editor.orbit_target,
        editor.orbit_angle,
        editor.orbit_elevation,
        position,
        1.,
    );
    let output = start_menu(&mut editor, &ctx, position);
    assert!(labels(&output).contains(&format!(
        "Coordinates: {:.3}, {:.3}",
        expected.x, expected.y
    )));
    let copy = text_position(&output, "Copy coordinates");
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::PointerMoved(copy), pointer(copy, true)],
    );
    let output = frame(&mut editor, &ctx, vec![pointer(copy, false)]);
    assert!(output
        .platform_output
        .commands
        .iter()
        .any(|command| matches!(command,
        egui::OutputCommand::CopyText(text) if text == &format!("{},{}", expected.x, expected.y))));
}

#[test]
fn external_launch_configuration_error_is_visible_and_copy_remains_available() {
    let mut editor = editor();
    editor.camera_view = CameraView::Top;
    editor.launch_action = Err("Invalid launch action: missing program".into());
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let position = editor.map_rect.unwrap().center() + egui::vec2(121., -92.);
    let output = start_menu(&mut editor, &ctx, position);
    assert!(labels(&output).contains(&"Invalid launch action: missing program".into()));
    assert!(labels(&output).contains(&"Copy coordinates".into()));
}
