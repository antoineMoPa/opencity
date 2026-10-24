use super::*;

#[test]
fn minimal_full_frame_has_only_selected_rename_and_no_click_through() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let output = frame(&mut editor, &ctx, vec![]);
    assert_eq!(labels(&output), vec!["main"]);
    assert_eq!(
        editor.map_rect.unwrap(),
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 650.0))
    );
    let original = editor.program.clone();
    editor.execute_palette_command("road:street", &ctx);
    assert!(editor.map_tool == MapTool::Draw);
    assert!(editor.map_start.is_none());
    assert_eq!(editor.program, original);
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
    assert!(editor.map_tool == MapTool::Select);
}

#[test]
fn minimal_rename_commit_cancel_validation_blur_and_focus() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let _ = frame(&mut editor, &ctx, vec![]);
    let id = egui::Id::new("placed-road-name");
    ctx.memory_mut(|m| m.request_focus(id));
    editor.minimal.draft = "renamed".into();
    let camera = (editor.map_pan, editor.orbit_angle, editor.orbit_elevation);
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::ArrowUp)]);
    assert_eq!(
        camera,
        (editor.map_pan, editor.orbit_angle, editor.orbit_elevation)
    );
    assert_eq!(
        map::project(&editor.program).segments[0].name.as_deref(),
        Some("main")
    );
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Enter)]);
    assert_eq!(
        map::project(&editor.program).segments[0].name.as_deref(),
        Some("renamed")
    );
    assert_eq!(editor.undo_stack.len(), 1);
    let original = editor.program.clone();
    ctx.memory_mut(|m| m.request_focus(id));
    editor.minimal.draft = "123 invalid".into();
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Enter)]);
    assert_eq!(editor.program, original);
    assert!(editor.minimal.error.is_some());
    ctx.memory_mut(|m| m.request_focus(id));
    editor.minimal.draft = "other".into();
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Enter)]);
    assert_eq!(editor.program, original);
    ctx.memory_mut(|m| m.request_focus(id));
    editor.minimal.draft = "cancelled".into();
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
    assert_eq!(editor.program, original);
    assert_eq!(editor.minimal.draft, "renamed");
    ctx.memory_mut(|m| m.request_focus(id));
    editor.minimal.draft = "blurred".into();
    let _ = frame(&mut editor, &ctx, vec![]);
    click(&mut editor, &ctx, egui::pos2(850.0, 600.0));
    assert_eq!(
        map::project(&editor.program).segments[0].name.as_deref(),
        Some("blurred")
    );
    assert_eq!(editor.undo_stack.len(), 2);
}

#[test]
fn minimal_external_rename_refreshes_only_when_not_editing() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let source = editor.selected_segment.unwrap();
    assert!(map::set_name(
        &mut editor.program,
        source,
        "external".into()
    ));
    let _ = frame(&mut editor, &ctx, vec![]);
    assert_eq!(editor.minimal.draft, "external");
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("placed-road-name")));
    editor.minimal.draft = "typing".into();
    assert!(map::set_name(
        &mut editor.program,
        source,
        "external_again".into()
    ));
    let _ = frame(&mut editor, &ctx, vec![]);
    assert_eq!(editor.minimal.draft, "typing");
}

#[test]
fn minimal_document_changes_discard_pending_rename_targets() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("placed-road-name")));
    editor.minimal.draft = "must_not_apply".into();
    let next = parse("road next: carlane; end; replacement=next(0,0,1,0);").unwrap();
    editor.load_program(next.clone(), None);
    assert!(editor.minimal.rename_source.is_none());
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Enter)]);
    assert_eq!(editor.program, next);
}

#[test]
fn minimal_rename_blur_keeps_original_target_when_selection_changes() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let _ = frame(&mut editor, &ctx, vec![]);
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("placed-road-name")));
    editor.minimal.draft = "first_only".into();
    let _ = frame(&mut editor, &ctx, vec![]);
    let second = map::project(&editor.program).segments[1].clone();
    let point = editor.spatial_screen(editor.map_rect.unwrap(), second.point_at(0.5), 0.0, true);
    click(&mut editor, &ctx, point);
    let projection = map::project(&editor.program);
    assert_eq!(projection.segments[0].name.as_deref(), Some("first_only"));
    assert_eq!(projection.segments[1].name.as_deref(), Some("other"));
    assert_eq!(editor.selected_segment, Some(second.source));
    let _ = frame(&mut editor, &ctx, vec![]);
    assert_eq!(editor.minimal.draft, "other");
}
