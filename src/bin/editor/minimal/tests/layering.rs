use super::*;

#[test]
fn selected_road_geometry_and_annotations_paint_below_palette() {
    for view in [CameraView::Top, CameraView::Isometric] {
        let mut editor = editor();
        editor.camera_view = view;
        editor.map_pan.y = -120.0;
        editor.program.statements.extend(
            parse(
                "mark=annotation_text(\"Canvas markup\",0,0); line=annotation_segment(-1,0,1,0);",
            )
            .unwrap()
            .statements,
        );
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        let _ = frame(&mut editor, &ctx, vec![palette_shortcut()]);
        let output = frame(&mut editor, &ctx, vec![]);
        let popup = editor.command_palette.rect().unwrap();
        let selected = editor_palette(ctx.theme()).map_selected;
        fn flatten<'a>(shape: &'a egui::Shape, out: &mut Vec<&'a egui::Shape>) {
            if let egui::Shape::Vec(children) = shape {
                for child in children {
                    flatten(child, out);
                }
            } else {
                out.push(shape);
            }
        }
        let mut shapes = Vec::new();
        for shape in &output.shapes {
            flatten(&shape.shape, &mut shapes);
        }
        // FullOutput is in final back-to-front paint order, including layers.
        let panel = shapes.iter().position(|shape| {
            matches!(shape, egui::Shape::Rect(r) if r.rect.contains(popup.center()) && r.rect.width() > 300.0 && r.rect.width() < 600.0 && r.fill != egui::Color32::TRANSPARENT)
        }).expect("palette's opaque background");
        let mut circles = 0;
        let mut controls = 0;
        let mut lines = 0;
        let mut callbacks = 0;
        let mut annotations = 0;
        let mut overlapping_handles = 0;
        for (index, shape) in shapes.iter().enumerate() {
            let canvas_shape = match shape {
                egui::Shape::Circle(c) if c.stroke.color == selected => {
                    circles += 1;
                    overlapping_handles += usize::from(popup.contains(c.center));
                    true
                }
                egui::Shape::Rect(r) if r.fill == selected && r.rect.width() == 10.0 => {
                    controls += 1;
                    true
                }
                egui::Shape::LineSegment { stroke, .. } if stroke.color == selected => {
                    lines += 1;
                    true
                }
                egui::Shape::Callback(_) => {
                    callbacks += 1;
                    true
                }
                egui::Shape::Text(t) if t.galley.job.text == "Canvas markup" => {
                    annotations += 1;
                    true
                }
                _ => false,
            };
            if canvas_shape {
                assert!(
                    index < panel,
                    "canvas primitive {index} paints over palette background {panel}"
                );
            }
        }
        assert!(circles >= 2, "selected endpoint handles remain visible");
        assert_eq!(controls, 2, "Bezier control squares remain visible");
        assert!(lines >= 2, "Bezier control lines remain visible");
        assert_eq!(callbacks, 1, "map scene remains visible");
        assert_eq!(annotations, 1, "persisted annotations remain visible");
        assert!(
            overlapping_handles > 0,
            "fixture must reproduce palette overlap"
        );
    }
}
