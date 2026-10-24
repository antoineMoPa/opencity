use super::*;

#[test]
fn palette_priorities_and_lazy_presets_do_not_edit_on_cancel() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let commands = editor.palette_commands();
    assert_eq!(
        commands
            .iter()
            .take(5)
            .map(|c| c.title.as_str())
            .collect::<Vec<_>>(),
        [
            "Add road",
            "Add highway",
            "Add interstate",
            "Add terrain",
            "Add concrete"
        ]
    );
    assert_eq!(commands.last().unwrap().id, "clear");
    let _ = frame(&mut editor, &ctx, vec![palette_shortcut()]);
    let output = frame(&mut editor, &ctx, vec![]);
    let positions = [
        "Add road",
        "Add highway",
        "Add interstate",
        "Add terrain",
        "Add concrete",
    ]
    .map(|title| text_position(&output, title).y);
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
    let before = editor.program.clone();
    for name in RoadEditor::canonical_road_names() {
        editor.execute_palette_command(&format!("road:{name}"), &ctx);
        assert_eq!(editor.road().unwrap().name, name);
        assert_eq!(editor.program, before);
        editor.cancel_placement();
        assert_eq!(editor.program, before);
        assert!(editor.undo_stack.is_empty());
    }
    editor.execute_palette_command("road:divided_highway", &ctx);
    let name = editor.ensure_placement_road_type().unwrap();
    assert_eq!(name, "divided_highway");
    let preset = editor.road().unwrap().clone();
    editor.cancel_placement();
    editor.execute_palette_command("road:divided_highway", &ctx);
    assert_eq!(editor.road().unwrap(), &preset);
    assert!(editor.minimal.pending_road.is_none());
    assert_eq!(
        editor
            .palette_commands()
            .iter()
            .filter(|c| c.id == "road:divided_highway")
            .count(),
        1
    );
}

#[test]
fn clear_palette_confirmation_cancel_and_atomic_undo_keep_path_and_baseline() {
    for dirty in [false, true] {
        let mut editor = editor();
        editor.path = Some(PathBuf::from("/tmp/current-city.road"));
        if dirty {
            editor.program.statements.extend(
                parse("mark=annotation_text(\"keep\",0,0);")
                    .unwrap()
                    .statements,
            );
        }
        let before = editor.program.clone();
        let baseline = editor.saved_program.clone();
        let path = editor.path.clone();
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        editor.execute_palette_command("road:wide", &ctx);
        editor.map_start = Some(WorldPoint { x: 1.0, y: 2.0 });
        let _ = frame(&mut editor, &ctx, vec![palette_shortcut()]);
        let _ = frame(&mut editor, &ctx, vec![egui::Event::Text("Clear".into())]);
        let _ = frame(&mut editor, &ctx, vec![]);
        let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Enter)]);
        assert_eq!(editor.program, before);
        assert!(editor.pending.is_some());
        let output = frame(&mut editor, &ctx, vec![]);
        assert!(
            labels(&output).contains(&"Clear map?".into()),
            "{:?}",
            labels(&output)
        );
        let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
        assert!(editor.pending.is_none());
        assert_eq!(editor.program, before);
        assert!(editor.map_start.is_some());
        assert!(editor.map_tool == MapTool::Draw);
        editor.file_action(FileAction::New);
        editor.file_action(FileAction::New);
        let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Enter)]);
        assert_eq!(editor.program, before);
        let output = frame(&mut editor, &ctx, vec![]);
        let clear = text_position(&output, "Clear");
        click(&mut editor, &ctx, clear);
        assert!(editor.pending.is_none());
        assert!(editor
            .program
            .statements
            .iter()
            .all(|s| !matches!(s, Statement::Call(_))));
        assert_eq!(editor.path, path);
        assert_eq!(editor.saved_program, baseline);
        assert!(editor.dirty);
        assert_eq!(editor.undo_stack.len(), 1);
        assert!(editor.restore_history(true));
        assert_eq!(editor.program, before);
        assert_eq!(editor.dirty, dirty);
    }
}

#[test]
fn clear_default_cancel_and_modal_input_cannot_reopen_palette_or_move_canvas() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let before = editor.program.clone();
    let camera = (editor.map_pan, editor.map_zoom, editor.orbit_angle);
    editor.file_action(FileAction::New);
    let _ = frame(&mut editor, &ctx, vec![]);
    let _ = frame(&mut editor, &ctx, vec![]);
    let _ = frame(
        &mut editor,
        &ctx,
        vec![palette_shortcut(), key(egui::Key::ArrowUp)],
    );
    assert!(!editor.command_palette.is_open());
    assert_eq!(
        camera,
        (editor.map_pan, editor.map_zoom, editor.orbit_angle)
    );
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Enter)]);
    assert!(
        editor.pending.is_none(),
        "Enter should activate default Cancel"
    );
    assert_eq!(editor.program, before);
    assert!(editor.undo_stack.is_empty());
}

#[test]
fn palette_search_keyboard_action_and_escape_own_canvas_input() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let original = editor.program.clone();
    let _ = frame(&mut editor, &ctx, vec![palette_shortcut()]);
    assert!(editor.command_palette.is_open());
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::Text("Add road".into())],
    );
    let output = frame(&mut editor, &ctx, vec![]);
    assert!(labels(&output).contains(&"Add road: street".into()));
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Enter)]);
    assert!(!editor.command_palette.is_open());
    assert!(editor.map_tool == MapTool::Draw);
    assert!(editor.map_start.is_none());
    assert_eq!(editor.program, original);
    let _ = frame(&mut editor, &ctx, vec![palette_shortcut()]);
    click(&mut editor, &ctx, egui::pos2(850.0, 610.0));
    assert!(!editor.command_palette.is_open());
    assert!(editor.map_start.is_none());
    assert_eq!(editor.program, original);
    let _ = frame(&mut editor, &ctx, vec![palette_shortcut()]);
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
    assert!(editor.map_tool == MapTool::Draw);
    assert!(editor.map_start.is_none());
}

#[test]
fn palette_query_does_not_edit_road_name_and_pointer_executes() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("placed-road-name")));
    editor.minimal.draft = "pending_name".into();
    let original = editor.program.clone();
    let _ = frame(&mut editor, &ctx, vec![palette_shortcut()]);
    let _ = frame(
        &mut editor,
        &ctx,
        vec![egui::Event::Text("Add road".into())],
    );
    let output = frame(&mut editor, &ctx, vec![]);
    assert_eq!(editor.minimal.draft, "pending_name");
    let row = text_position(&output, "Place a road with two clicks");
    click(&mut editor, &ctx, row);
    assert!(editor.map_tool == MapTool::Draw);
    assert!(!editor.command_palette.is_open());
    assert_eq!(editor.program, original);
}

#[test]
fn palette_arrows_cancel_and_shared_file_dispatch_preserve_edits() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("placed-road-name")));
    editor.minimal.draft = "pending_name".into();
    let original = editor.program.clone();
    let camera = (
        editor.map_pan,
        editor.orbit_angle,
        editor.orbit_elevation,
        editor.map_zoom,
    );
    let _ = frame(&mut editor, &ctx, vec![palette_shortcut()]);
    let _ = frame(
        &mut editor,
        &ctx,
        vec![key(egui::Key::ArrowDown), key(egui::Key::ArrowUp)],
    );
    let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
    let _ = frame(&mut editor, &ctx, vec![]);
    assert_eq!(editor.minimal.draft, "pending_name");
    assert_eq!(editor.program, original);
    assert_eq!(
        camera,
        (
            editor.map_pan,
            editor.orbit_angle,
            editor.orbit_elevation,
            editor.map_zoom
        )
    );
    editor.dirty = true;
    // Palette and native menus both dispatch the same guarded FileAction.
    editor.execute_palette_command("new", &ctx);
    assert!(matches!(editor.pending, Some(PendingAction::New)));
    assert_eq!(editor.program, original);
    editor.pending = None;
    editor.file_action(FileAction::Open);
    assert!(matches!(editor.pending, Some(PendingAction::Open)));
    assert_eq!(editor.program, original);
}

#[test]
fn palette_lists_only_supported_minimal_actions() {
    let editor = editor();
    let ids: Vec<_> = editor
        .palette_commands()
        .into_iter()
        .map(|c| c.id)
        .collect();
    for id in [
        "clear",
        "open",
        "save",
        "save-as",
        "undo",
        "redo",
        "copy",
        "paste",
        "select",
        "rename",
        "theme",
        "road:street",
        "road:wide",
        "annotation-segment",
        "annotation-text",
    ] {
        assert!(ids.iter().any(|item| item == id), "{id}");
    }
    for kind in RoadEditor::placement_landscapes() {
        assert!(ids.contains(&format!("landscape:{}", kind.call_name())));
    }
}

#[test]
fn minimal_palette_road_identity_survives_statement_changes() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    editor.program.statements.insert(
        0,
        Statement::Call(Call {
            assigns: vec![],
            name: "metadata".into(),
            args: vec![],
        }),
    );
    editor.execute_palette_command("road:wide", &ctx);
    assert_eq!(editor.road().unwrap().name, "wide");
    assert!(editor.map_start.is_none());
}
