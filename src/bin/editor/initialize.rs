//! Initialize for the city editor.
use super::*;

impl RoadEditor {
    pub(super) fn new(cc: &eframe::CreationContext<'_>, open_path: Option<PathBuf>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let (web_event_sender, web_events) = std::sync::mpsc::channel();
        egui_extras::install_image_loaders(&cc.egui_ctx);
        cc.egui_ctx
            .set_visuals_of(egui::Theme::Dark, editor_visuals(egui::Theme::Dark));
        cc.egui_ctx
            .set_visuals_of(egui::Theme::Light, editor_visuals(egui::Theme::Light));
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        let mut app = Self {
            #[cfg(not(target_arch = "wasm32"))]
            launch_action: external_launch::LaunchAction::from_environment(),
            minimal: minimal::MinimalEditor::default(),
            #[cfg(not(target_arch = "wasm32"))]
            connection_id: None,
            #[cfg(not(target_arch = "wasm32"))]
            connection_member: None,
            command_palette: CommandPalette::default(),
            program: boot_program(),
            saved_program: boot_program(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            history_drag_active: false,
            skip_history: false,
            path: None,
            dirty: false,
            selected_road: None,
            selected_element: None,
            selected_segment: None,
            selected_intersection: None,
            selected_endpoint: None,
            point_options: None,
            segment_name_draft: String::new(),
            road_name_draft: "neighborhood_street".into(),
            new_kind: RoadElementKind::CarLane,
            view: View::Map,
            map_tool: MapTool::Select,
            camera_view: CameraView::Isometric,
            camera_transition: None,
            render_format: cc
                .wgpu_render_state
                .as_ref()
                .map(|state| state.target_format)
                .unwrap_or(eframe::wgpu::TextureFormat::Bgra8Unorm),
            projection_cache: None,
            interaction_cache: None,
            landscape_cache: None,
            selected_zone: None,
            frontage_cache: None,
            scene_cache: None,
            #[cfg(target_arch = "wasm32")]
            web_scene_build: None,
            #[cfg(not(target_arch = "wasm32"))]
            scene_build: None,
            #[cfg(not(target_arch = "wasm32"))]
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
            map_zoom: 55.0,
            fit_scene_once: true,
            dragging_endpoint: None,
            dragging_control: None,
            dragging_intersection: None,
            dragging_background: false,
            show_source: false,
            status: String::new(),
            pending: None,
            confirm_delete_road: false,
            #[cfg(target_os = "macos")]
            native_menu: NativeFileMenu::install(),
            #[cfg(not(target_arch = "wasm32"))]
            mcp: None,
            #[cfg(target_arch = "wasm32")]
            web_events,
            #[cfg(target_arch = "wasm32")]
            web_event_sender,
            #[cfg(target_arch = "wasm32")]
            web_ctx: cc.egui_ctx.clone(),
            map_rect: None,
        };
        if let Some(path) = open_path {
            app.open_path(path);
        }
        app
    }
}
