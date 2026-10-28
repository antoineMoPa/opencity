use super::*;

#[test]
fn cross_section_full_frame_edit_back_and_undo_share_the_document() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    let original = editor.program.clone();
    editor.execute_palette_command("edit-section", &ctx);
    assert!(editor.view == View::Section);
    let _ = frame(&mut editor, &ctx, vec![]);
    let output = frame(&mut editor, &ctx, vec![]);
    assert!(editor.map_rect.is_none());
    assert!(labels(&output).iter().any(|label| label == "Cross section"));
    assert_eq!(editor.road().unwrap().name, "street");
    let add = text_position(&output, "Add element");
    click(&mut editor, &ctx, add);
    assert_eq!(editor.road().unwrap().elements.len(), 2);
    assert_eq!(editor.undo_stack.len(), 1);
    let output = frame(&mut editor, &ctx, vec![]);
    let back = text_position(&output, "Back to map");
    click(&mut editor, &ctx, back);
    assert!(editor.view == View::Map);
    let _ = frame(&mut editor, &ctx, vec![]);
    assert!(editor.map_rect.is_some());
    assert_eq!(map::project(&editor.program).segments.len(), 2);
    assert!(editor.restore_history(true));
    assert_eq!(editor.program, original);
}

#[test]
fn cross_section_selected_profile_rename_reorder_and_remove_preserve_references() {
    let mut editor = editor();
    editor.program = parse(
        "road lane: sidewalk-carlane-carlane; end; main=lane(0,0,4,0); \
            attach=lane_point(main,2,0.4); end_point=map_point(2,1); \
            branch=lane(attach,1,0,2,0,end_point); road other: carlane; end;",
    )
    .unwrap();
    editor.selected_segment = Some(map::project(&editor.program).segments[0].source);
    editor.selected_road = Some(editor.program.statements.len() - 1);
    editor.enter_section_editor();
    assert_eq!(editor.road().unwrap().name, "lane");
    editor.selected_element = Some(2);
    editor.move_element(-1);
    assert_eq!(
        map::lane_attachment(&editor.program, "attach").unwrap().1,
        1
    );
    editor.remove_element();
    assert!(map::lane_attachment(&editor.program, "attach").is_none());
    assert_eq!(map::project(&editor.program).unresolved, 0);
    assert!(editor.rename_road("profile"));
    assert!(map::project(&editor.program)
        .segments
        .iter()
        .all(|road| road.road_name == "profile"));
    assert_eq!(
        parse(&editor.program.to_road_source()).unwrap(),
        editor.program
    );
}

#[test]
fn map_placement_commands_leave_cross_section_mode() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    for command in [
        "road:street",
        "landscape:terrain",
        "annotation-segment",
        "box-select",
    ] {
        editor.enter_section_editor();
        editor.execute_palette_command(command, &ctx);
        assert!(editor.view == View::Map, "{command}");
    }
}

#[test]
fn section_layout_has_inset_hierarchy_and_wraps_without_text_overlap() {
    for size in [
        egui::vec2(1100.0, 800.0),
        egui::vec2(800.0, 500.0),
        egui::vec2(420.0, 700.0),
    ] {
        let mut editor = editor();
        editor.enter_section_editor();
        editor.selected_element = Some(0);
        let ctx = egui::Context::default();
        let _ = frame_sized(&mut editor, &ctx, vec![], size);
        let output = frame_sized(&mut editor, &ctx, vec![], size);
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some((
                    text.galley.job.text.as_str(),
                    text.galley.mesh_bounds.translate(text.pos.to_vec2()),
                )),
                _ => None,
            })
            .collect();
        let rect = |label| {
            texts
                .iter()
                .find(|(text, _)| *text == label)
                .unwrap_or_else(|| panic!("missing {label} at {size:?}"))
                .1
        };
        let back = rect("Back to map");
        assert!(back.left() >= 24.0 && back.top() >= 24.0);
        assert!(
            rect("Changes apply to every placement of this road profile.").top()
                > rect("Cross section").bottom()
        );
        assert!(rect("Profile").top() > rect("Cross section").bottom());
        assert!(rect("Preview").top() > rect("Add element").bottom());
        assert!(
            rect("Preview").top() < size.y - 24.0,
            "preview visible at {size:?}"
        );
        assert_eq!(
            texts.iter().filter(|(text, _)| *text == "street").count(),
            2,
            "profile selector and editable name only"
        );
        let controls = [
            "Back to map",
            "Cross section",
            "Profile",
            "New profile",
            "Save…",
            "Delete profile…",
            "Road name",
            "Direction",
            "Add element",
            "Show source",
            "Preview",
        ];
        for (index, label) in controls.iter().enumerate() {
            let bounds = rect(label);
            assert!(
                bounds.left() >= 24.0 && bounds.right() <= size.x - 24.0,
                "{label}: {bounds:?} at {size:?}"
            );
            for other in &controls[index + 1..] {
                assert!(
                    !bounds.intersects(rect(other)),
                    "{label} {bounds:?} overlaps {other} {:?} at {size:?}",
                    rect(other)
                );
            }
        }
    }
}
