use opencity::road_profiles::{
    road_for_segment, road_profiles as paint_profiles, AttachmentFootprint,
    RoadProfile as PaintProfile,
};
#[cfg(not(target_arch = "wasm32"))]
use opencity::road_profiles::{road_joins, straight_join};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use eframe::egui::{self, Color32, RichText};
use egui_command_palette::{Command as PaletteCommand, CommandPalette};
use opencity::{
    map, parse, parse_road_file, project_landscape_features, set_grade, Argument, Call,
    LandscapeKind, Program, RoadDefinition, RoadDirection, RoadElement, RoadElementKind, Statement,
};

#[path = "editor/annotations.rs"]
mod annotations;
#[path = "editor/building_options.rs"]
mod building_options;
#[cfg(not(target_arch = "wasm32"))]
#[path = "editor/connection.rs"]
mod connection;
#[path = "editor/elevation_map.rs"]
mod elevation_map;
#[path = "editor/frontage.rs"]
mod frontage;
#[path = "editor/glb.rs"]
mod glb;
#[cfg(not(target_arch = "wasm32"))]
#[path = "editor/mcp.rs"]
mod mcp;
#[path = "editor/minimal.rs"]
mod minimal;
#[path = "editor/model.rs"]
mod model;
#[path = "editor/network_ui.rs"]
mod network_ui;
#[path = "editor/point_options.rs"]
mod point_options;
#[path = "editor/scene3d.rs"]
mod scene3d;
#[path = "editor/spatial_edit.rs"]
mod spatial_edit;
#[path = "editor/svg.rs"]
mod svg;
#[path = "editor/terrain_placement.rs"]
mod terrain_placement;
use map::{SegmentSource, WorldPoint};

const KINDS: [RoadElementKind; 16] = [
    RoadElementKind::BuildingEdge,
    RoadElementKind::Sidewalk,
    RoadElementKind::BikeLane,
    RoadElementKind::ParkingLane,
    RoadElementKind::BusLane,
    RoadElementKind::CarLane,
    RoadElementKind::BusStop,
    RoadElementKind::Middle,
    RoadElementKind::Verge,
    RoadElementKind::Barrier,
    RoadElementKind::TrainTrack,
    RoadElementKind::TramTrack,
    RoadElementKind::SubwayTrack,
    RoadElementKind::Runway,
    RoadElementKind::Taxiway,
    RoadElementKind::Apron,
];

// Persistent viewport annotations are small at the overview zoom and grow with the map.
const ANNOTATION_OVERVIEW_ZOOM: f32 = 75.0;
const ANNOTATION_OVERVIEW_SCALE: f32 = 0.1;
const MIN_MAP_ZOOM: f32 = 18.0;
const MIN_COMPONENT_MAP_ZOOM: f32 = 0.1;
const MAX_MAP_ZOOM: f32 = 2400.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Map,
    Section,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MapTool {
    Select,
    BoxSelect,
    Draw,
    Draft,
    Intersection(map::IntersectionKind),
    Landscape(LandscapeKind),
    AnnotationSegment,
    AnnotationText,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CameraView {
    Top,
    Isometric,
    Vehicular,
}

#[derive(Clone)]
struct CopiedSegment {
    road_name: String,
    name: Option<String>,
    points: [WorldPoint; 4],
    start_z: f32,
    end_z: f32,
}

struct SceneCache {
    program: Program,
    theme: egui::Theme,
    scene: Arc<scene3d::Scene>,
}

#[cfg(not(target_arch = "wasm32"))]
struct DesignAnalysisCache {
    program: Program,
    issues: Vec<DesignIssue>,
}

#[cfg(target_arch = "wasm32")]
enum WebFileEvent {
    Opened { name: String, source: String },
    Saved { name: String, program: Program },
    Message(String),
}

fn landscape_label(kind: LandscapeKind) -> &'static str {
    match kind {
        LandscapeKind::Land => "Land boundary",
        LandscapeKind::Beach => "Beach area",
        LandscapeKind::Concrete => "Concrete area",
        LandscapeKind::Lake => "Lake",
        LandscapeKind::River => "River",
        LandscapeKind::Vegetation => "Vegetation area",
        LandscapeKind::Terrain => "Terrain elevation",
        LandscapeKind::Building => "Building",
    }
}

fn default_program() -> Program {
    parse(include_str!("../../examples/default_roads.road")).expect("bundled road library parses")
}

#[cfg(test)]
fn starter_program() -> Program {
    parse(include_str!("../../examples/starter_city.road")).expect("bundled starter city parses")
}

fn boot_program() -> Program {
    parse(include_str!("../../examples/default_map.road")).expect("bundled default map parses")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FileAction {
    New,
    Open,
    Save,
    SaveAs,
}

#[derive(Clone, Copy)]
enum PendingAction {
    New,
    Open,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IsoPrimaryDrag {
    Orbit,
    Pan,
    Box,
    Edit,
}

fn iso_primary_drag(modifiers: egui::Modifiers, b_down: bool) -> IsoPrimaryDrag {
    if modifiers.ctrl {
        IsoPrimaryDrag::Orbit
    } else if modifiers.shift {
        IsoPrimaryDrag::Pan
    } else if modifiers.alt || b_down {
        IsoPrimaryDrag::Box
    } else {
        IsoPrimaryDrag::Edit
    }
}

struct RoadEditor {
    #[cfg(not(target_arch = "wasm32"))]
    launch_action: Result<Option<external_launch::LaunchAction>, String>,
    minimal: minimal::MinimalEditor,
    #[cfg(not(target_arch = "wasm32"))]
    connection_id: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    connection_member: Option<(String, String)>,
    command_palette: CommandPalette,
    program: Program,
    saved_program: Program,
    undo_stack: Vec<Program>,
    redo_stack: Vec<Program>,
    history_drag_active: bool,
    skip_history: bool,
    path: Option<PathBuf>,
    dirty: bool,
    selected_road: Option<usize>,
    selected_element: Option<usize>,
    selected_segment: Option<SegmentSource>,
    selected_intersection: Option<String>,
    selected_endpoint: Option<(SegmentSource, bool)>,
    point_options: Option<point_options::PointOptions>,
    segment_name_draft: String,
    road_name_draft: String,
    new_kind: RoadElementKind,
    view: View,
    map_tool: MapTool,
    camera_view: CameraView,
    camera_transition: Option<camera_transition::CameraTransition>,
    render_format: eframe::wgpu::TextureFormat,
    projection_cache: Option<(Program, Arc<map::Projection>)>,
    interaction_cache: Option<Arc<scene_cache::InteractionGeometry>>,
    landscape_cache: Option<(Program, Arc<opencity::LandscapeProjection>)>,
    selected_zone: Option<String>,
    frontage_cache: Option<(Program, Arc<opencity::FrontageProjection>)>,
    scene_cache: Option<SceneCache>,
    #[cfg(target_arch = "wasm32")]
    web_scene_build: Option<scene3d::WebSceneBuild>,
    #[cfg(not(target_arch = "wasm32"))]
    scene_build: Option<std::sync::mpsc::Receiver<SceneCache>>,
    #[cfg(not(target_arch = "wasm32"))]
    design_analysis_cache: Option<DesignAnalysisCache>,
    #[cfg(not(target_arch = "wasm32"))]
    design_analysis_job: Option<std::sync::mpsc::Receiver<DesignAnalysisCache>>,
    orbit_angle: f32,
    orbit_elevation: f32,
    orbit_target: WorldPoint,
    vehicle_position: WorldPoint,
    vehicle_heading: f32,
    vehicle_eye_height: f32,
    selected_segments: Vec<SegmentSource>,
    clipboard: Vec<CopiedSegment>,
    box_start: Option<egui::Pos2>,
    iso_drag_pan: bool,
    iso_drag_orbit: bool,
    map_start: Option<WorldPoint>,
    map_start_ref: Option<String>,
    map_start_height: Option<f32>,
    map_placement_before: Option<Program>,
    map_placement_draft: Option<Program>,
    landscape_points: Vec<WorldPoint>,
    selected_landscape: Option<String>,
    branch_mode: map::SplitMode,
    map_pan: egui::Vec2,
    map_zoom: f32,
    fit_scene_once: bool,
    dragging_endpoint: Option<(SegmentSource, bool)>,
    dragging_control: Option<(SegmentSource, bool)>,
    dragging_intersection: Option<String>,
    dragging_background: bool,
    show_source: bool,
    status: String,
    pending: Option<PendingAction>,
    confirm_delete_road: bool,
    #[cfg(target_os = "macos")]
    native_menu: Option<NativeFileMenu>,
    #[cfg(not(target_arch = "wasm32"))]
    mcp: Option<mcp::Bridge>,
    #[cfg(target_arch = "wasm32")]
    web_events: std::sync::mpsc::Receiver<WebFileEvent>,
    #[cfg(target_arch = "wasm32")]
    web_event_sender: std::sync::mpsc::Sender<WebFileEvent>,
    #[cfg(target_arch = "wasm32")]
    web_ctx: egui::Context,
    map_rect: Option<egui::Rect>,
}

#[path = "editor/commands.rs"]
mod commands;
#[path = "editor/intersection_placement.rs"]
mod intersection_placement;
use commands::preset_label;

#[path = "editor/scene_cache.rs"]
mod scene_cache;

#[path = "editor/camera.rs"]
mod camera;
#[path = "editor/camera_transition.rs"]
mod camera_transition;

#[path = "editor/selection.rs"]
mod selection;

#[path = "editor/initialize.rs"]
mod initialize;

#[path = "editor/history.rs"]
mod history;

#[path = "editor/section.rs"]
mod section;
#[path = "editor/section_ui.rs"]
mod section_ui;

#[path = "editor/document.rs"]
mod document;

#[path = "editor/dialogs.rs"]
mod dialogs;

#[path = "editor/landscape.rs"]
mod landscape;
use landscape::*;

#[path = "editor/viewport_iso.rs"]
mod viewport_iso;

#[path = "editor/viewport_top.rs"]
mod viewport_top;

#[path = "editor/external_launch.rs"]
mod external_launch;

#[path = "editor/viewport_vehicle.rs"]
mod viewport_vehicle;

#[path = "editor/coordinates.rs"]
mod coordinates;
use coordinates::*;

#[path = "editor/picking_iso.rs"]
mod picking_iso;
use picking_iso::*;

#[path = "editor/picking.rs"]
mod picking;
use picking::*;

#[path = "editor/road_paint.rs"]
mod road_paint;
use road_paint::*;

#[cfg(not(target_arch = "wasm32"))]
#[path = "editor/design_rules.rs"]
mod design_rules;
#[cfg(not(target_arch = "wasm32"))]
use design_rules::*;

#[path = "editor/intersection_paint.rs"]
mod intersection_paint;
use intersection_paint::*;

#[path = "editor/markings.rs"]
mod markings;
use markings::*;

#[path = "editor/frame.rs"]
mod frame;

#[path = "editor/appearance.rs"]
mod appearance;
use appearance::*;

#[cfg(target_os = "macos")]
#[path = "editor/native_menu.rs"]
mod native_menu;
#[cfg(target_os = "macos")]
use native_menu::*;

#[cfg(test)]
#[path = "editor/tests/mod.rs"]
mod tests;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    if let Some(result) = opencity::cli::run(std::env::args_os().skip(1).collect()) {
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    if matches!(first.as_deref(), Some(value) if value == "mcp" || value == "--mcp") {
        mcp::run_stdio();
        return Ok(());
    }
    let path = first.map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "OpenCity Roads",
        options,
        Box::new(move |cc| {
            let mut app = RoadEditor::new(cc, path.clone());
            match mcp::Bridge::listen(&cc.egui_ctx) {
                Ok(bridge) => app.mcp = Some(bridge),
                Err(error) => eprintln!("OpenCity MCP: {error}"),
            }
            Ok(Box::new(app))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub struct WebHandle {
    runner: eframe::WebRunner,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
impl WebHandle {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            runner: eframe::WebRunner::new(),
        }
    }

    pub async fn start(
        &self,
        canvas: web_sys::HtmlCanvasElement,
    ) -> Result<(), wasm_bindgen::JsValue> {
        self.runner
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(RoadEditor::new(cc, None)))),
            )
            .await
    }

    pub fn has_panicked(&self) -> bool {
        self.runner.has_panicked()
    }

    pub fn panic_message(&self) -> Option<String> {
        self.runner.panic_summary().map(|summary| summary.message())
    }
}
