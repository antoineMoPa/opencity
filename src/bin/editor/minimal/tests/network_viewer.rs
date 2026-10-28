use super::*;
const SOURCE: &str = "intersection junction { port A.in at (-1,0) heading east; port \
    B.out at (1,0) heading east; path through { from A.in; line to \
    B.out; } allow vehicle A.in -> B.out via through; surface \
    pavement along through width 0.4; } cross=junction(0,0);";

#[test]
fn map_click_opens_read_only_viewer_and_back_restores_both_map_views() {
    for iso in [false, true] {
        let mut editor = editor();
        editor.program = parse(SOURCE).unwrap();
        editor.saved_program = editor.program.clone();
        editor.selected_segment = None;
        editor.selected_segments.clear();
        editor.selected_road = None;
        editor.camera_view = if iso {
            CameraView::Isometric
        } else {
            CameraView::Top
        };
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        let center = editor.spatial_screen(
            editor.map_rect.unwrap(),
            WorldPoint { x: 0., y: 0. },
            0.,
            iso,
        );
        let before = editor.program.clone();
        let pan = editor.map_pan;
        let zoom = editor.map_zoom;
        click(&mut editor, &ctx, center);
        let _ = frame(&mut editor, &ctx, vec![]);
        let popup = frame(&mut editor, &ctx, vec![]);
        assert!(
            labels(&popup).contains(&"Open intersection viewer".into()),
            "iso={iso}: {:?}",
            labels(&popup)
        );
        click(
            &mut editor,
            &ctx,
            text_position(&popup, "Open intersection viewer"),
        );
        let output = frame(&mut editor, &ctx, vec![]);
        assert!(editor.minimal.intersection_viewer.is_some());
        assert!(!labels(&output).contains(&"Intersection viewer".into()));
        let back = text_position(&output, "Back");
        assert!(back.x >= 40. && back.y >= 28.);
        for label in [
            "Read-only",
            "Movements (1)",
            "A.in → B.out · vehicle",
            "Paths (1)",
            "Ports (2)",
        ] {
            assert!(
                labels(&output).contains(&label.into()),
                "{label}: {:?}",
                labels(&output)
            );
        }
        click(
            &mut editor,
            &ctx,
            text_position(&output, "A.in → B.out · vehicle"),
        );
        let output = frame(
            &mut editor,
            &ctx,
            vec![key(egui::Key::Delete), palette_shortcut()],
        );
        assert!(!editor.command_palette.is_open());
        assert_eq!(editor.program, before);
        assert!(editor.undo_stack.is_empty());
        assert!(!editor.dirty);
        click(&mut editor, &ctx, text_position(&output, "Back"));
        assert!(editor.minimal.intersection_viewer.is_none());
        assert!(
            editor.camera_view
                == if iso {
                    CameraView::Isometric
                } else {
                    CameraView::Top
                }
        );
        assert_eq!(editor.map_pan, pan);
        assert_eq!(editor.map_zoom, zoom);
        assert_eq!(editor.program, before);
    }
}
#[test]
fn existing_profile_intersections_also_offer_the_isolated_viewer() {
    for iso in [false, true] {
        let mut editor = editor();
        let road = editor.road().unwrap().clone();
        let id = map::place_intersection(
            &mut editor.program,
            map::IntersectionKind::FourWay,
            &road,
            WorldPoint { x: 0., y: 0. },
        );
        editor.saved_program = editor.program.clone();
        editor.camera_view = if iso {
            CameraView::Isometric
        } else {
            CameraView::Top
        };
        editor.selected_segment = None;
        editor.selected_segments.clear();
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        let center = editor.spatial_screen(
            editor.map_rect.unwrap(),
            WorldPoint { x: 0., y: 0. },
            0.,
            iso,
        );
        let before = editor.program.clone();
        click(&mut editor, &ctx, center);
        let _ = frame(&mut editor, &ctx, vec![]);
        let popup = frame(&mut editor, &ctx, vec![]);
        assert_eq!(editor.selected_intersection.as_deref(), Some(id.as_str()));
        click(
            &mut editor,
            &ctx,
            text_position(&popup, "Open intersection viewer"),
        );
        let output = frame(&mut editor, &ctx, vec![]);
        assert!(labels(&output).contains(&"8 ports".into()));
        assert!(labels(&output).contains(&"Movements (12)".into()));
        assert!(editor.minimal.intersection_viewer.is_some());
        let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
        assert!(editor.minimal.intersection_viewer.is_none());
        assert_eq!(editor.program, before);
    }
}
#[test]
fn network_only_documents_fit_and_palette_blocks_inspection_gestures() {
    let mut editor = editor();
    editor.load_program(parse(SOURCE).unwrap(), None);
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    assert!(editor.map_zoom.is_finite());
    let center = editor.spatial_screen(
        editor.map_rect.unwrap(),
        WorldPoint { x: 0., y: 0. },
        0.,
        true,
    );
    let _ = frame(&mut editor, &ctx, vec![palette_shortcut()]);
    click(&mut editor, &ctx, center);
    assert!(editor.minimal.intersection_viewer.is_none());
    assert!(editor.minimal.network_options.is_none());
}
