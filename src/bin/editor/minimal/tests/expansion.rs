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
