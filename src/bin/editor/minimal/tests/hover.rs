use super::*;

#[test]
fn hovered_road_name_is_tiny_at_bottom_right_and_clears_when_pointer_leaves() {
    for view in [
        CameraView::Top,
        CameraView::Isometric,
        CameraView::Vehicular,
    ] {
        let mut editor = editor();
        editor.camera_view = view;
        editor.vehicle_position = WorldPoint { x: -3.0, y: 3.0 };
        editor.vehicle_heading = 0.0;
        editor.vehicle_eye_height = 0.15;
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        let _ = frame(&mut editor, &ctx, vec![]);
        let rect = editor.map_rect.unwrap();
        let point = WorldPoint { x: 0.0, y: 3.0 };
        let pointer = match view {
            CameraView::Top => screen_of(rect, editor.map_pan, editor.map_zoom, point),
            CameraView::Isometric => iso_screen(
                rect,
                editor.map_pan,
                editor.map_zoom,
                editor.orbit_target,
                editor.orbit_angle,
                editor.orbit_elevation,
                point,
                0.0,
            ),
            CameraView::Vehicular => {
                rect.center() + egui::vec2(0.0, rect.height() * 0.714 * 0.15 / 3.0)
            }
        };
        let program = editor.program.clone();
        let selected = editor.selected_segment;
        let output = frame(&mut editor, &ctx, vec![egui::Event::PointerMoved(pointer)]);
        let label = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == "other" => Some(t),
                _ => None,
            })
            .unwrap_or_else(|| panic!("hover label missing: {:?}", labels(&output)));
        let text_bounds = label.galley.rect.translate(label.pos.to_vec2());
        let corner = text_bounds.right_bottom();
        assert!(corner.distance(rect.right_bottom() - egui::vec2(8.0, 8.0)) < 0.1);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(background)
                if background.rect == text_bounds.expand(2.0))));
        assert!(label.galley.size().y <= 12.0);
        assert_eq!(editor.program, program);
        assert_eq!(editor.selected_segment, selected);
        assert!(!editor.dirty);
        let output = frame(&mut editor, &ctx, vec![egui::Event::PointerGone]);
        assert!(!labels(&output).iter().any(|s| s == "other"));
    }
}

#[test]
fn unnamed_hover_uses_profile_name_and_palette_suppresses_the_readout() {
    let mut editor = editor();
    editor.camera_view = CameraView::Top;
    editor.program = parse("road local: carlane; end local(-2, 0, 2, 0);").unwrap();
    editor.selected_segment = None;
    editor.selected_segments.clear();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let pointer = editor.map_rect.unwrap().center();
    let output = frame(&mut editor, &ctx, vec![egui::Event::PointerMoved(pointer)]);
    assert!(labels(&output).iter().any(|s| s == "local"));
    editor.command_palette.open();
    let output = frame(&mut editor, &ctx, vec![]);
    assert!(!output.shapes.iter().any(|s| matches!(&s.shape,
        egui::Shape::Text(t) if t.galley.job.text == "local" && t.pos.y > 600.0)));
}

#[test]
fn hover_shows_semantic_hierarchy_then_leaf_and_wraps_inside_a_narrow_viewport() {
    let mut editor = editor();
    editor.camera_view = CameraView::Top;
    let path = vec![
        "Route 7".into(),
        "Long River Crossing Bridge and its eastbound approach".into(),
    ];
    opencity::road_labels::set_road_path(&mut editor.program, "main", &path).unwrap();
    let ctx = egui::Context::default();
    let size = egui::vec2(210.0, 220.0);
    let _ = frame_sized(&mut editor, &ctx, vec![], size);
    let pointer = editor.map_rect.unwrap().center();
    let output = frame_sized(
        &mut editor,
        &ctx,
        vec![egui::Event::PointerMoved(pointer)],
        size,
    );
    let expected = concat!(
        "Route 7 > Long River Crossing Bridge and its eastbound approach",
        " > main"
    );
    let text = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.job.text == expected => Some(t),
            _ => None,
        })
        .expect("full semantic path should be visible");
    assert!(text.galley.rows.len() > 1);
    let text_bounds = text.galley.rect.translate(text.pos.to_vec2());
    assert!(text_bounds.left() >= 8.0);
    assert!(
        text_bounds
            .right_bottom()
            .distance(size.to_pos2() - egui::vec2(8.0, 8.0))
            < 0.1
    );
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Rect(background)
            if background.rect == text_bounds.expand(2.0))));
}

#[test]
fn hover_reads_a_cached_inferred_name_on_a_segment_without_an_anchor() {
    let mut editor = editor();
    editor.camera_view = CameraView::Top;
    editor.program = parse(
        "road local: carlane; end
        p = map_point(0, 0); a = map_point(-4, 0); b = map_point(4, 0);
        first = local(a, -3, 0, -1, 0, p);
        second = local(p, 1, 0, 3, 0, b);
        road_path(first, \"River Bridge\"); road_routes(first, \"Route 7\");",
    )
    .unwrap();
    editor.selected_segment = None;
    editor.selected_segments.clear();
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let rect = editor.map_rect.unwrap();
    let pointer = screen_of(
        rect,
        editor.map_pan,
        editor.map_zoom,
        WorldPoint { x: 2.0, y: 0.0 },
    );
    let output = frame(&mut editor, &ctx, vec![egui::Event::PointerMoved(pointer)]);
    assert!(labels(&output)
        .iter()
        .any(|s| s == "Route 7 > River Bridge > second"));
}
