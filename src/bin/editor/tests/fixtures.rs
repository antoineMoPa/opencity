use super::*;

pub(super) fn existing_design_checks_are_clear(issues: &[DesignIssue]) -> bool {
    issues.iter().all(|issue| {
        matches!(
            issue.code,
            "invalid_merge_connection"
                | "vehicle_path_obstructed"
                | "vehicle_lane_discontinuity"
                | "vehicle_lane_crosses_non_car_band"
                | "vehicle_lane_merge_height_mismatch"
        )
    })
}

pub(crate) fn sample_editor() -> RoadEditor {
    RoadEditor {
        minimal: minimal::MinimalEditor::default(),
        connection_id: None,
        connection_member: None,
        command_palette: CommandPalette::default(),
        program: parse(include_str!("../../../../examples/city.road")).unwrap(),
        saved_program: parse(include_str!("../../../../examples/city.road")).unwrap(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        history_drag_active: false,
        skip_history: false,
        path: None,
        dirty: false,
        selected_road: Some(0),
        selected_element: None,
        selected_segment: None,
        selected_intersection: None,
        selected_endpoint: None,
        point_options: None,
        segment_name_draft: String::new(),
        road_name_draft: "normal_road".into(),
        new_kind: RoadElementKind::BikeLane,
        view: View::Map,
        map_tool: MapTool::Select,
        camera_view: CameraView::Top,
        camera_transition: None,
        render_format: eframe::wgpu::TextureFormat::Bgra8Unorm,
        projection_cache: None,
        scene_cache: None,
        #[cfg(target_arch = "wasm32")]
        web_scene_build: None,
        #[cfg(not(target_arch = "wasm32"))]
        scene_build: None,
        design_analysis_cache: None,
        #[cfg(not(target_arch = "wasm32"))]
        design_analysis_job: None,
        orbit_angle: std::f32::consts::FRAC_PI_4,
        orbit_elevation: ISO_ELEVATION,
        orbit_target: WorldPoint { x: 0.0, y: 0.0 },
        vehicle_position: WorldPoint { x: 0.0, y: 0.0 },
        vehicle_heading: std::f32::consts::FRAC_PI_2,
        vehicle_eye_height: 0.45,
        selected_segments: Vec::new(),
        clipboard: Vec::new(),
        box_start: None,
        iso_drag_pan: false,
        iso_drag_orbit: false,
        map_start: None,
        map_start_ref: None,
        map_start_height: None,
        map_placement_before: None,
        map_placement_draft: None,
        landscape_points: Vec::new(),
        selected_landscape: None,
        branch_mode: map::SplitMode::Dedicated,
        map_pan: egui::Vec2::ZERO,
        map_zoom: 100.0,
        fit_scene_once: false,
        dragging_endpoint: None,
        dragging_control: None,
        dragging_intersection: None,
        dragging_background: false,
        show_source: false,
        status: String::new(),
        pending: None,
        confirm_delete_road: false,
        #[cfg(target_os = "macos")]
        native_menu: None,
        mcp: None,
        map_rect: None,
    }
}
