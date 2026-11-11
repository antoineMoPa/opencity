use super::*;

#[test]
fn expansion_strip_resizes_each_camera_and_returns_space_when_closed() {
    let mut editor = editor();
    let ctx = egui::Context::default();
    for view in [
        CameraView::Top,
        CameraView::Isometric,
        CameraView::Vehicular,
    ] {
        editor.camera_view = view;
        editor.camera_transition = None;
        editor.minimal.expansion.open = false;
        let _ = frame(&mut editor, &ctx, vec![]);
        let full = editor.map_rect.unwrap();
        editor.execute_palette_command("expand-network", &ctx);
        let _ = frame(&mut editor, &ctx, vec![]);
        let editing = editor.map_rect.unwrap();
        assert_eq!(editing.width(), full.width());
        assert_eq!(editing.height(), full.height() - 42.);
        assert!(editor.minimal.toolbar_rect.is_none());
        let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
        let _ = frame(&mut editor, &ctx, vec![]);
        assert!(!editor.minimal.expansion.open);
        assert_eq!(editor.map_rect.unwrap(), full);
    }
}

#[test]
fn follow_frames_an_elevated_tip_in_each_camera_without_changing_zoom() {
    let target = opencity::expansion_ui::FollowTarget {
        position: [12., -8., 0.35],
        heading: 1.2,
        length: 1.,
    };
    for view in [
        CameraView::Top,
        CameraView::Isometric,
        CameraView::Vehicular,
    ] {
        let mut editor = editor();
        let ctx = egui::Context::default();
        editor.camera_view = view;
        editor.camera_transition = None;
        let _ = frame(&mut editor, &ctx, vec![]);
        let zoom = editor.map_zoom;
        let program = editor.program.clone();
        editor.follow_expansion(target);
        assert!(editor.camera_transition.is_some());
        assert_eq!(editor.map_zoom, zoom);
        assert_eq!(editor.program, program);
        // Inspect the settled destination, including the end's authored elevation.
        editor.camera_transition = None;
        let rect = editor.map_rect.unwrap();
        let matrix = editor.camera_clip_matrix(rect);
        let v: [f32; 4] = std::array::from_fn(|row| {
            matrix[row][0] * target.position[0]
                + matrix[row][1] * target.position[1]
                + matrix[row][2] * target.position[2]
                + matrix[row][3]
        });
        assert!(v[3] > 0.);
        let x = v[0] / v[3];
        let y = v[1] / v[3];
        assert!(x.abs() < 0.001, "x={x}");
        match view {
            CameraView::Top | CameraView::Isometric => assert!(y.abs() < 0.001, "y={y}"),
            CameraView::Vehicular => {
                assert!(y > -1. && y < 0.);
                assert_eq!(editor.vehicle_heading, target.heading);
            }
        }
    }
}
