use super::*;

#[test]
fn isometric_ground_pick_tracks_orbit_and_pan() {
    let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(640.0, 480.0));
    let target = WorldPoint { x: 2.0, y: -3.0 };
    let point = WorldPoint { x: 4.5, y: 1.25 };
    for angle in [0.0, 0.7, 2.3] {
        for elevation in [MIN_ISO_ELEVATION, ISO_ELEVATION, MAX_ISO_ELEVATION] {
            let screen = iso_screen(
                rect,
                egui::vec2(31.0, -14.0),
                73.0,
                target,
                angle,
                elevation,
                point,
                0.0,
            );
            let recovered = iso_ground_point(
                rect,
                egui::vec2(31.0, -14.0),
                73.0,
                target,
                angle,
                elevation,
                screen,
            );
            assert!((recovered.x - point.x).abs() < 0.0001);
            assert!((recovered.y - point.y).abs() < 0.0001);
        }
    }
}

#[test]
fn orbit_rotation_keeps_viewport_center_fixed_after_pan() {
    let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(640.0, 480.0));
    let mut editor = sample_editor();
    editor.camera_view = CameraView::Isometric;
    editor.map_pan = egui::vec2(83.0, -41.0);
    editor.map_zoom = 73.0;
    editor.orbit_target = WorldPoint { x: 2.0, y: -3.0 };
    let center_ground = iso_ground_point(
        rect,
        editor.map_pan,
        editor.map_zoom,
        editor.orbit_target,
        editor.orbit_angle,
        editor.orbit_elevation,
        rect.center(),
    );
    editor.center_isometric_orbit(rect);
    assert_eq!(editor.map_pan, egui::Vec2::ZERO);
    assert!((editor.orbit_target.x - center_ground.x).abs() < 0.0001);
    assert!((editor.orbit_target.y - center_ground.y).abs() < 0.0001);
    for angle in [0.0, 1.0, 2.0] {
        for elevation in [MIN_ISO_ELEVATION, MAX_ISO_ELEVATION] {
            assert!(
                iso_screen(
                    rect,
                    editor.map_pan,
                    editor.map_zoom,
                    editor.orbit_target,
                    angle,
                    elevation,
                    center_ground,
                    0.0
                )
                .distance(rect.center())
                    < 0.001
            );
        }
    }
}

#[test]
fn elevation_shortcut_steps_and_stays_in_safe_range() {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
    let mut editor = sample_editor();
    editor.camera_view = CameraView::Isometric;
    editor.map_rect = Some(rect);
    editor.map_pan = egui::vec2(30.0, -20.0);
    let center_ground = iso_ground_point(
        rect,
        editor.map_pan,
        editor.map_zoom,
        editor.orbit_target,
        editor.orbit_angle,
        editor.orbit_elevation,
        rect.center(),
    );
    let original = editor.orbit_elevation;
    editor.adjust_isometric_elevation(ISO_ELEVATION_STEP);
    assert!((editor.orbit_elevation - original - ISO_ELEVATION_STEP).abs() < 0.0001);
    assert!(
        iso_screen(
            rect,
            editor.map_pan,
            editor.map_zoom,
            editor.orbit_target,
            editor.orbit_angle,
            editor.orbit_elevation,
            center_ground,
            0.0
        )
        .distance(rect.center())
            < 0.001
    );
    editor.adjust_isometric_elevation(-100.0);
    assert_eq!(editor.orbit_elevation, MIN_ISO_ELEVATION);
    editor.adjust_isometric_elevation(100.0);
    assert_eq!(editor.orbit_elevation, MAX_ISO_ELEVATION);
}

#[test]
fn isometric_zoom_keeps_cursor_ground_point_fixed() {
    let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(640.0, 480.0));
    let pointer = egui::pos2(531.0, 147.0);
    let target = WorldPoint { x: 2.0, y: -3.0 };
    for angle in [0.0, 0.7, 2.3] {
        let mut pan = egui::vec2(31.0, -14.0);
        let mut zoom = 73.0;
        let anchored = iso_ground_point(rect, pan, zoom, target, angle, ISO_ELEVATION, pointer);
        for scroll in [120.0, -90.0, 10000.0, -10000.0] {
            iso_zoom_at_pointer(
                rect,
                &mut pan,
                &mut zoom,
                target,
                angle,
                ISO_ELEVATION,
                pointer,
                scroll,
                MIN_MAP_ZOOM,
            );
            assert!(
                pointer.distance(iso_screen(
                    rect,
                    pan,
                    zoom,
                    target,
                    angle,
                    ISO_ELEVATION,
                    anchored,
                    0.0
                )) < 0.01
            );
        }
        assert_eq!(zoom, MIN_MAP_ZOOM);
        iso_zoom_at_pointer(
            rect,
            &mut pan,
            &mut zoom,
            target,
            angle,
            ISO_ELEVATION,
            pointer,
            100000.0,
            MIN_MAP_ZOOM,
        );
        assert_eq!(zoom, MAX_MAP_ZOOM);
        let close_anchor_error = pointer.distance(iso_screen(
            rect,
            pan,
            zoom,
            target,
            angle,
            ISO_ELEVATION,
            anchored,
            0.0,
        ));
        assert!(
            close_anchor_error < 0.1,
            "anchor drift: {close_anchor_error}"
        );
    }
}

#[test]
fn close_zoom_is_available_to_the_mcp_viewport() {
    let mut editor = sample_editor();
    editor.camera_view = CameraView::Isometric;
    editor
        .call_mcp(
            "set_viewport",
            &serde_json::json!({"center_x":2.0,"center_y":-3.0,"zoom":MAX_MAP_ZOOM}),
        )
        .unwrap();
    assert_eq!(editor.map_zoom, MAX_MAP_ZOOM);
    assert_eq!(editor.orbit_target, WorldPoint { x: 2.0, y: -3.0 });
    assert!(editor
        .call_mcp(
            "set_viewport",
            &serde_json::json!({"center_x":2.0,"center_y":-3.0,"zoom":MAX_MAP_ZOOM + 1.0}),
        )
        .is_err());
}

#[test]
fn switching_views_preserves_focus_and_scale_after_pan_and_zoom() {
    let mut editor = sample_editor();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    editor.map_rect = Some(rect);
    editor.map_pan = egui::vec2(240.0, -90.0);
    editor.map_zoom = 91.0;
    let focus = editor.camera_focus(rect);
    for _ in 0..5 {
        editor.set_camera_view(CameraView::Isometric);
        assert_eq!(editor.map_zoom, 91.0);
        assert!((editor.camera_focus(rect).x - focus.x).abs() < 0.0001);
        assert!((editor.camera_focus(rect).y - focus.y).abs() < 0.0001);
        editor.set_camera_view(CameraView::Top);
        assert!((editor.camera_focus(rect).x - focus.x).abs() < 0.0001);
        assert!((editor.camera_focus(rect).y - focus.y).abs() < 0.0001);
    }
}

#[test]
fn vehicle_switch_uses_viewport_focus_and_overview_follows_exploration() {
    let mut editor = sample_editor();
    editor.program = parse("road lane: carlane; end; lane(0,0,10,0); lane(30,30,40,30);").unwrap();
    let projection = map::project(&editor.program);
    editor.selected_segment = Some(projection.segments[1].source);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    editor.map_rect = Some(rect);
    editor.map_zoom = 100.0;
    editor.map_pan = egui::vec2(-725.0, 10.0);
    editor.set_camera_view(CameraView::Vehicular);
    assert!((editor.vehicle_position.x - 7.25).abs() < 0.05);
    assert!(editor.vehicle_position.y.abs() < 0.001);
    editor.camera_transition = None;
    editor.vehicle_position = WorldPoint { x: 9.0, y: 2.0 };
    editor.set_camera_view(CameraView::Isometric);
    assert_eq!(editor.camera_focus(rect), editor.vehicle_position);
    assert_eq!(editor.map_zoom, 100.0);
}

#[test]
fn transitions_start_at_visible_projection_and_finish_without_state_reset() {
    let mut editor = sample_editor();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    editor.map_rect = Some(rect);
    let before = editor.camera_clip_matrix(rect);
    editor.set_camera_view(CameraView::Isometric);
    assert_eq!(editor.camera_transition_matrix(rect), Some(before));
    let ctx = egui::Context::default();
    for _ in 0..30 {
        let mut output = ctx.run_ui(Default::default(), |ui| {
            editor.advance_camera_transition(ui.ctx())
        });
        output.textures_delta.clear();
    }
    assert!(editor.camera_transition.is_none());
    assert!(editor.camera_view == CameraView::Isometric);
    editor.set_camera_view(CameraView::Top);
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerMoved(rect.center()));
    input.events.push(egui::Event::PointerButton {
        pos: rect.center(),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    let mut output = ctx.run_ui(input, |ui| editor.advance_camera_transition(ui.ctx()));
    output.textures_delta.clear();
    assert!(
        editor.camera_transition.is_none(),
        "camera input takes ownership immediately"
    );
}

#[test]
fn isometric_primary_drag_routes_modifiers() {
    for (modifiers, expected) in [
        (egui::Modifiers::CTRL, IsoPrimaryDrag::Orbit),
        (egui::Modifiers::COMMAND, IsoPrimaryDrag::Edit),
        (egui::Modifiers::SHIFT, IsoPrimaryDrag::Pan),
        (egui::Modifiers::ALT, IsoPrimaryDrag::Box),
    ] {
        assert_eq!(iso_primary_drag(modifiers, false), expected);
    }
    assert_eq!(
        iso_primary_drag(egui::Modifiers::NONE, true),
        IsoPrimaryDrag::Box
    );
    assert_eq!(
        iso_primary_drag(egui::Modifiers::NONE, false),
        IsoPrimaryDrag::Edit
    );
}
