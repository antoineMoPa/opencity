use std::{collections::HashMap, path::PathBuf};

use eframe::egui::{self, Color32, RichText};
use opencity::{
    parse, parse_road_file, Argument, Program, RoadDefinition, RoadElement, RoadElementKind,
    Statement,
};

#[path = "editor/map.rs"]
mod map;
#[path = "editor/mcp.rs"]
mod mcp;
use map::{SegmentSource, WorldPoint};

const KINDS: [RoadElementKind; 10] = [
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
];

#[derive(Clone, Copy)]
struct EditorPalette {
    theme: egui::Theme,
    toolbar: Color32,
    brand: Color32,
    warning: Color32,
    map_background: Color32,
    map_axis: Color32,
    map_major: Color32,
    map_minor: Color32,
    map_text: Color32,
    map_subtle: Color32,
    map_outline: Color32,
    map_selected: Color32,
    map_preview: Color32,
    map_centerline: Color32,
    tile_text: Color32,
}

fn editor_palette(theme: egui::Theme) -> EditorPalette {
    match theme {
        egui::Theme::Dark => EditorPalette {
            theme,
            toolbar: Color32::from_rgb(29, 35, 45),
            brand: Color32::from_rgb(105, 190, 220),
            warning: Color32::from_rgb(245, 150, 110),
            map_background: Color32::from_rgb(10, 27, 34),
            map_axis: Color32::from_rgb(28, 48, 55),
            map_major: Color32::from_rgb(18, 36, 43),
            map_minor: Color32::from_rgb(12, 30, 36),
            map_text: Color32::from_rgb(226, 237, 235),
            map_subtle: Color32::from_rgb(153, 177, 175),
            map_outline: Color32::from_rgb(11, 18, 22),
            map_selected: Color32::from_rgb(124, 207, 245),
            map_preview: Color32::from_rgb(127, 216, 199),
            map_centerline: Color32::from_white_alpha(155),
            tile_text: Color32::WHITE,
        },
        egui::Theme::Light => EditorPalette {
            theme,
            toolbar: Color32::from_rgb(235, 242, 245),
            brand: Color32::from_rgb(21, 104, 132),
            warning: Color32::from_rgb(153, 69, 26),
            map_background: Color32::from_rgb(239, 246, 243),
            map_axis: Color32::from_rgb(155, 184, 179),
            map_major: Color32::from_rgb(196, 216, 210),
            map_minor: Color32::from_rgb(222, 234, 228),
            map_text: Color32::from_rgb(36, 60, 59),
            map_subtle: Color32::from_rgb(80, 108, 106),
            map_outline: Color32::from_rgb(72, 86, 88),
            map_selected: Color32::from_rgb(15, 105, 151),
            map_preview: Color32::from_rgb(22, 124, 107),
            map_centerline: Color32::from_black_alpha(90),
            tile_text: Color32::from_rgb(25, 43, 49),
        },
    }
}

fn editor_visuals(theme: egui::Theme) -> egui::Visuals {
    match theme {
        egui::Theme::Dark => {
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = Color32::from_rgb(24, 29, 37);
            visuals.window_fill = Color32::from_rgb(24, 29, 37);
            visuals.widgets.active.bg_fill = Color32::from_rgb(57, 87, 111);
            visuals
        }
        egui::Theme::Light => {
            let mut visuals = egui::Visuals::light();
            visuals.panel_fill = Color32::from_rgb(250, 252, 252);
            visuals.window_fill = Color32::from_rgb(250, 252, 252);
            visuals.widgets.active.bg_fill = Color32::from_rgb(190, 216, 224);
            visuals
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Map,
    Section,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MapTool {
    Select,
    Draw,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum PresetCategory {
    City,
    Rural,
    Interstate,
    Custom,
}

impl PresetCategory {
    fn label(self) -> &'static str {
        match self {
            Self::City => "CITY",
            Self::Rural => "COUNTRYSIDE",
            Self::Interstate => "HIGHWAYS & INTERSTATES",
            Self::Custom => "CUSTOM",
        }
    }
}

fn preset_category(name: &str) -> PresetCategory {
    match name {
        "neighborhood_street"
        | "main_street"
        | "downtown_avenue"
        | "protected_bikeway"
        | "transit_corridor"
        | "market_street"
        | "pedestrian_mall" => PresetCategory::City,
        "country_lane" | "farm_track" | "rural_road" | "rural_highway" | "mountain_road" => {
            PresetCategory::Rural
        }
        "divided_highway" | "exit_ramp" | "interstate_three_lane" | "interstate_four_lane" => {
            PresetCategory::Interstate
        }
        _ => PresetCategory::Custom,
    }
}

fn preset_label(name: &str) -> &str {
    match name {
        "neighborhood_street" => "Neighborhood street",
        "main_street" => "Main street",
        "downtown_avenue" => "Downtown avenue",
        "protected_bikeway" => "Protected bikeway",
        "transit_corridor" => "Transit corridor",
        "market_street" => "Market street",
        "pedestrian_mall" => "Pedestrian mall",
        "country_lane" => "Country lane",
        "farm_track" => "Farm track",
        "rural_road" => "Rural road",
        "rural_highway" => "Rural highway",
        "mountain_road" => "Mountain road",
        "divided_highway" => "Divided highway",
        "exit_ramp" => "Exit ramp",
        "interstate_three_lane" => "Interstate | 3 lanes/way",
        "interstate_four_lane" => "Interstate | 4 lanes/way",
        _ => name,
    }
}

fn default_program() -> Program {
    parse(include_str!("../../examples/default_roads.road")).expect("bundled road library parses")
}

fn starter_program() -> Program {
    parse(include_str!("../../examples/starter_city.road")).expect("bundled starter city parses")
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

struct RoadEditor {
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
    lane_end_choice: Option<ElementEndCandidate>,
    segment_name_draft: String,
    road_name_draft: String,
    new_kind: RoadElementKind,
    view: View,
    map_tool: MapTool,
    map_start: Option<WorldPoint>,
    map_start_ref: Option<String>,
    branch_element: usize,
    branch_t: f32,
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
    mcp: Option<mcp::Bridge>,
    map_rect: Option<egui::Rect>,
}

impl RoadEditor {
    fn new(cc: &eframe::CreationContext<'_>, open_path: Option<PathBuf>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        cc.egui_ctx
            .set_visuals_of(egui::Theme::Dark, editor_visuals(egui::Theme::Dark));
        cc.egui_ctx
            .set_visuals_of(egui::Theme::Light, editor_visuals(egui::Theme::Light));
        cc.egui_ctx.set_theme(egui::ThemePreference::System);
        let mut app = Self {
            program: starter_program(),
            saved_program: starter_program(),
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
            lane_end_choice: None,
            segment_name_draft: String::new(),
            road_name_draft: "neighborhood_street".into(),
            new_kind: RoadElementKind::CarLane,
            view: View::Map,
            map_tool: MapTool::Select,
            map_start: None,
            map_start_ref: None,
            branch_element: usize::MAX,
            branch_t: 0.75,
            branch_mode: map::SplitMode::Dedicated,
            map_pan: egui::Vec2::ZERO,
            map_zoom: 75.0,
            fit_scene_once: true,
            dragging_endpoint: None,
            dragging_control: None,
            dragging_intersection: None,
            dragging_background: false,
            show_source: false,
            status:
                "Explore the little city and its winding lanes | File > New starts a blank map."
                    .into(),
            pending: None,
            confirm_delete_road: false,
            #[cfg(target_os = "macos")]
            native_menu: NativeFileMenu::install(),
            mcp: None,
            map_rect: None,
        };
        if let Some(path) = open_path {
            app.open_path(path);
        }
        app
    }

    fn road(&self) -> Option<&RoadDefinition> {
        match self.program.statements.get(self.selected_road?)? {
            Statement::Road(road) => Some(road),
            _ => None,
        }
    }

    fn add_intersection_from_library(&mut self, kind: map::IntersectionKind) -> Option<String> {
        if self.road().is_none() {
            self.selected_road = self
                .program
                .statements
                .iter()
                .position(|statement| matches!(statement, Statement::Road(_)));
        }
        let road = self.road()?.clone();
        self.road_name_draft = road.name.clone();
        let visible_center = self
            .map_rect
            .map(|rect| world_of(rect, self.map_pan, self.map_zoom, rect.center()))
            .unwrap_or(WorldPoint { x: 0.0, y: 0.0 });
        let width: f32 = road
            .elements
            .iter()
            .map(|element| map::element_world_width(element.kind))
            .sum();
        let projection = map::project(&self.program);
        let existing = map::intersections(&self.program, &projection);
        let spacing = width * 1.8;
        let center = [
            (0.0, 0.0),
            (1.0, 0.0),
            (-1.0, 0.0),
            (0.0, 1.0),
            (0.0, -1.0),
            (1.0, 1.0),
            (-1.0, 1.0),
            (1.0, -1.0),
            (-1.0, -1.0),
        ]
        .into_iter()
        .map(|(x, y)| WorldPoint {
            x: visible_center.x + x * spacing,
            y: visible_center.y + y * spacing,
        })
        .find(|candidate| {
            existing.iter().all(|intersection| {
                (candidate.x - intersection.position.x).abs() > spacing * 0.8
                    || (candidate.y - intersection.position.y).abs() > spacing * 0.8
            })
        })
        .unwrap_or(visible_center);
        let reference = map::place_intersection(&mut self.program, kind, &road, center);
        if width > 0.0 && width * self.map_zoom < 64.0 {
            self.map_zoom = (64.0 / width).clamp(18.0, 420.0);
            self.map_pan = egui::vec2(-center.x * self.map_zoom, center.y * self.map_zoom);
        }
        self.map_tool = MapTool::Select;
        self.map_start = None;
        self.map_start_ref = None;
        self.selected_segment = None;
        self.selected_endpoint = None;
        self.selected_intersection = Some(reference.clone());
        self.dirty = true;
        self.status = "Intersection placed | click an edge port to draw a road".into();
        Some(reference)
    }

    fn restore_history(&mut self, undo: bool) -> bool {
        let target = if undo {
            &mut self.undo_stack
        } else {
            &mut self.redo_stack
        };
        if let Some(previous) = target.pop() {
            let current = std::mem::replace(&mut self.program, previous);
            let destination = if undo {
                &mut self.redo_stack
            } else {
                &mut self.undo_stack
            };
            destination.push(current);
            self.selected_segment = None;
            self.selected_intersection = None;
            self.selected_endpoint = None;
            self.lane_end_choice = None;
            self.map_start = None;
            self.map_start_ref = None;
            self.selected_road = self.selected_road.filter(|index| {
                matches!(
                    self.program.statements.get(*index),
                    Some(Statement::Road(_))
                )
            });
            self.road_name_draft = self
                .road()
                .map(|road| road.name.clone())
                .unwrap_or_default();
            self.dirty = self.program != self.saved_program;
            self.status = if undo { "Undid edit" } else { "Redid edit" }.into();
            self.skip_history = true;
            true
        } else {
            false
        }
    }

    fn record_history(&mut self, before: Program, dragging: bool) {
        if self.skip_history {
            self.skip_history = false;
        } else if self.program != before {
            if !self.history_drag_active {
                self.undo_stack.push(before);
                if self.undo_stack.len() > 100 {
                    self.undo_stack.remove(0);
                }
                self.redo_stack.clear();
            }
            self.history_drag_active = dragging;
        }
        if !dragging {
            self.history_drag_active = false;
        }
        self.dirty = self.program != self.saved_program;
    }

    fn start_extension(&mut self, source: SegmentSource, is_end: bool) -> bool {
        let projection = map::project(&self.program);
        let Some(segment) = projection
            .segments
            .iter()
            .find(|segment| segment.source == source)
        else {
            return false;
        };
        let Some(road_index) = self.program.statements.iter().position(|statement|
            matches!(statement, Statement::Road(road) if road.name == segment.road_name)) else { return false };
        let Some((point, reference)) = map::link_endpoint(&mut self.program, source, is_end) else {
            return false;
        };
        self.selected_road = Some(road_index);
        self.road_name_draft = segment.road_name.clone();
        self.map_tool = MapTool::Draw;
        self.map_start = Some(point);
        self.map_start_ref = Some(reference);
        self.selected_endpoint = None;
        self.dirty = true;
        self.status =
            "Continuing road | click another road end to join, or click the map to extend".into();
        true
    }

    fn start_branch(&mut self, source: SegmentSource, element: usize, t: f32) -> bool {
        let Some((reference, position)) =
            map::create_lane_point(&mut self.program, source, element, t)
        else {
            return false;
        };
        if let Some((index, name)) = map::project(&self.program)
            .segments
            .iter()
            .find(|segment| segment.source == source)
            .and_then(|segment| {
                self.program
                    .statements
                    .iter()
                    .enumerate()
                    .find_map(|(index, statement)| match statement {
                        Statement::Road(road) if road.name == segment.road_name => {
                            Some((index, road.name.clone()))
                        }
                        _ => None,
                    })
            })
        {
            self.selected_road = Some(index);
            self.road_name_draft = name;
        }
        self.map_tool = MapTool::Draw;
        self.map_start = Some(position);
        self.map_start_ref = Some(reference);
        self.selected_endpoint = None;
        self.dirty = true;
        self.status = "Branch started | click an endpoint to place the exit or merge".into();
        true
    }

    fn road_mut(&mut self) -> Option<&mut RoadDefinition> {
        match self.program.statements.get_mut(self.selected_road?)? {
            Statement::Road(road) => Some(road),
            _ => None,
        }
    }

    fn unique_road_name(&self) -> String {
        for number in 1.. {
            let name = format!("road_{number}");
            if !self
                .program
                .statements
                .iter()
                .any(|statement| matches!(statement, Statement::Road(road) if road.name == name))
            {
                return name;
            }
        }
        unreachable!()
    }

    fn add_road(&mut self) {
        let name = self.unique_road_name();
        self.program
            .statements
            .push(Statement::Road(RoadDefinition {
                name: name.clone(),
                elements: vec![
                    RoadElement {
                        kind: RoadElementKind::Sidewalk,
                        catch: None,
                    },
                    RoadElement {
                        kind: RoadElementKind::CarLane,
                        catch: None,
                    },
                    RoadElement {
                        kind: RoadElementKind::Middle,
                        catch: None,
                    },
                    RoadElement {
                        kind: RoadElementKind::CarLane,
                        catch: None,
                    },
                    RoadElement {
                        kind: RoadElementKind::Sidewalk,
                        catch: None,
                    },
                ],
            }));
        self.selected_road = Some(self.program.statements.len() - 1);
        self.selected_element = None;
        self.road_name_draft = name.clone();
        self.view = View::Section;
        self.dirty = true;
        self.status = format!("Created {name}");
    }

    fn add_element(&mut self) {
        let kind = self.new_kind;
        let selected = self.selected_element;
        let before = map::project(&self.program);
        let road_name = self.road().map(|road| road.name.clone());
        let mut inserted = None;
        if let Some(road) = self.road_mut() {
            let at = selected
                .map_or(road.elements.len(), |index| index + 1)
                .min(road.elements.len());
            road.elements.insert(at, RoadElement { kind, catch: None });
            inserted = Some(at);
            self.selected_element = Some(at);
            self.dirty = true;
        }
        if let (Some(name), Some(at)) = (road_name, inserted) {
            map::update_lane_attachments(
                &mut self.program,
                &name,
                map::ElementEdit::Inserted(at),
                &before,
            );
        }
    }

    fn move_element(&mut self, offset: isize) {
        let Some(index) = self.selected_element else {
            return;
        };
        let before = map::project(&self.program);
        let road_name = self.road().map(|road| road.name.clone());
        let mut swapped = None;
        if let Some(road) = self.road_mut() {
            let target = index as isize + offset;
            if target >= 0 && (target as usize) < road.elements.len() {
                road.elements.swap(index, target as usize);
                swapped = Some(target as usize);
                self.selected_element = Some(target as usize);
                self.dirty = true;
            }
        }
        if let (Some(name), Some(target)) = (road_name, swapped) {
            map::update_lane_attachments(
                &mut self.program,
                &name,
                map::ElementEdit::Swapped(index, target),
                &before,
            );
        }
    }

    fn remove_element(&mut self) {
        let Some(index) = self.selected_element else {
            return;
        };
        let before = map::project(&self.program);
        let road_name = self.road().map(|road| road.name.clone());
        let mut removed = false;
        if let Some(road) = self.road_mut() {
            if road.elements.len() > 1 && index < road.elements.len() {
                road.elements.remove(index);
                removed = true;
                self.selected_element = Some(index.min(road.elements.len() - 1));
                self.dirty = true;
            } else {
                self.status = "A road needs at least one element.".into();
            }
        }
        if let (Some(name), true) = (road_name, removed) {
            map::update_lane_attachments(
                &mut self.program,
                &name,
                map::ElementEdit::Removed(index),
                &before,
            );
        }
    }

    fn remove_road(&mut self) {
        let Some(index) = self.selected_road else {
            return;
        };
        if index >= self.program.statements.len() {
            return;
        }
        self.program.statements.remove(index);
        self.selected_road = self
            .program
            .statements
            .iter()
            .position(|statement| matches!(statement, Statement::Road(_)));
        self.selected_element = None;
        self.selected_segment = None;
        self.selected_endpoint = None;
        self.lane_end_choice = None;
        self.road_name_draft = self
            .road()
            .map(|road| road.name.clone())
            .unwrap_or_default();
        self.dirty = true;
        self.status = "Road removed. Review any calls that used it before saving.".into();
    }

    fn valid_name(name: &str) -> bool {
        let mut chars = name.chars();
        matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !matches!(name, "road" | "block" | "end")
    }

    fn rename_road(&mut self, new_name: &str) -> bool {
        let Some(index) = self.selected_road else {
            return false;
        };
        if !Self::valid_name(new_name)
            || self
                .program
                .statements
                .iter()
                .enumerate()
                .any(|(other, statement)| {
                    other != index
                        && matches!(statement, Statement::Road(road) if road.name == new_name)
                })
        {
            return false;
        }
        let Some(Statement::Road(road)) = self.program.statements.get_mut(index) else {
            return false;
        };
        if road.name == new_name {
            return false;
        }
        let old_name = std::mem::replace(&mut road.name, new_name.to_owned());
        for statement in &mut self.program.statements {
            match statement {
                Statement::Call(call) if call.name == old_name => call.name = new_name.to_owned(),
                Statement::Call(call)
                    if map::IntersectionKind::from_call_name(&call.name).is_some() =>
                {
                    if let Some(Argument::Identifier(name)) = call.args.get_mut(1) {
                        if *name == old_name {
                            *name = new_name.to_owned();
                        }
                    }
                }
                Statement::Block(block) => {
                    for call in &mut block.calls {
                        if call.name == old_name {
                            call.name = new_name.to_owned();
                        }
                    }
                }
                _ => {}
            }
        }
        self.dirty = true;
        true
    }

    fn road_name_editor(&mut self, ui: &mut egui::Ui) {
        if self.road().is_none() {
            return;
        }
        ui.label("Road name");
        let response = ui.text_edit_singleline(&mut self.road_name_draft);
        let valid = Self::valid_name(&self.road_name_draft);
        let duplicate = self.program.statements.iter().enumerate().any(|(index, statement)| {
            Some(index) != self.selected_road
                && matches!(statement, Statement::Road(road) if road.name == self.road_name_draft)
        });
        if !valid || duplicate {
            ui.colored_label(
                editor_palette(ui.ctx().theme()).warning,
                if duplicate {
                    "A road already has this name."
                } else {
                    "Use letters, digits, underscores; start with a letter or _."
                },
            );
        }
        if response.changed() && valid && !duplicate {
            let name = self.road_name_draft.clone();
            self.rename_road(&name);
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        match parse_road_file(&path) {
            Ok(program) => {
                self.selected_road = program
                    .statements
                    .iter()
                    .position(|statement| matches!(statement, Statement::Road(_)));
                self.selected_element = None;
                self.selected_segment = None;
                self.selected_intersection = None;
                self.selected_endpoint = None;
                self.lane_end_choice = None;
                self.segment_name_draft.clear();
                self.map_start = None;
                self.map_start_ref = None;
                self.dragging_endpoint = None;
                self.dragging_control = None;
                self.dragging_intersection = None;
                self.dragging_background = false;
                self.road_name_draft = self
                    .selected_road
                    .and_then(|index| match &program.statements[index] {
                        Statement::Road(road) => Some(road.name.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                self.program = program;
                self.saved_program = self.program.clone();
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.skip_history = true;
                self.fit_scene_once = true;
                self.path = Some(path.clone());
                self.dirty = false;
                self.confirm_delete_road = false;
                self.status = format!("Opened {}", path.display());
            }
            Err(error) => self.status = format!("Could not open {}: {error}", path.display()),
        }
    }

    fn save_to(&mut self, path: PathBuf) {
        if let Err(error) = parse(&self.program.to_road_source()) {
            self.status = format!("Fix the document before saving: {error}");
            return;
        }
        match self.program.write_road_file(&path) {
            Ok(()) => {
                self.path = Some(path.clone());
                self.saved_program = self.program.clone();
                self.dirty = false;
                self.status = format!("Saved {}", path.display());
            }
            Err(error) => self.status = format!("Could not save {}: {error}", path.display()),
        }
    }

    fn file_action(&mut self, action: FileAction) {
        match action {
            FileAction::New | FileAction::Open if self.dirty => {
                self.pending = Some(if action == FileAction::New {
                    PendingAction::New
                } else {
                    PendingAction::Open
                });
            }
            FileAction::New => self.new_document(),
            FileAction::Open => self.pick_open(),
            FileAction::Save => {
                if let Some(path) = self.path.clone() {
                    self.save_to(path);
                } else {
                    self.pick_save();
                }
            }
            FileAction::SaveAs => self.pick_save(),
        }
    }

    fn new_document(&mut self) {
        self.program = default_program();
        self.saved_program = self.program.clone();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.skip_history = true;
        self.path = None;
        self.selected_road = Some(0);
        self.selected_element = None;
        self.selected_segment = None;
        self.selected_intersection = None;
        self.selected_endpoint = None;
        self.lane_end_choice = None;
        self.segment_name_draft.clear();
        self.road_name_draft = "neighborhood_street".into();
        self.view = View::Map;
        self.map_tool = MapTool::Draw;
        self.map_start = None;
        self.map_start_ref = None;
        self.branch_element = usize::MAX;
        self.branch_t = 0.75;
        self.branch_mode = map::SplitMode::Dedicated;
        self.map_pan = egui::Vec2::ZERO;
        self.map_zoom = 100.0;
        self.fit_scene_once = false;
        self.dragging_endpoint = None;
        self.dragging_control = None;
        self.dragging_intersection = None;
        self.dragging_background = false;
        self.dirty = false;
        self.confirm_delete_road = false;
        self.status = "New blank map".into();
    }

    fn pick_open(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("OpenCity roads", &["road"])
            .pick_file()
        {
            self.open_path(path);
        }
    }

    fn pick_save(&mut self) {
        let mut dialog = rfd::FileDialog::new().add_filter("OpenCity roads", &["road"]);
        if let Some(path) = &self.path {
            if let Some(parent) = path.parent() {
                dialog = dialog.set_directory(parent);
            }
            if let Some(name) = path.file_name() {
                dialog = dialog.set_file_name(name.to_string_lossy());
            }
        } else {
            dialog = dialog.set_file_name("roads.road");
        }
        if let Some(path) = dialog.save_file() {
            self.save_to(path);
        }
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            for (label, action) in [
                ("New    Cmd+N", FileAction::New),
                ("Open…    Cmd+O", FileAction::Open),
                ("Save    Cmd+S", FileAction::Save),
                ("Save As…    Shift+Cmd+S", FileAction::SaveAs),
            ] {
                if ui.button(label).clicked() {
                    self.file_action(action);
                    ui.close();
                }
            }
        });
    }

    fn top_bar(&mut self, root: &mut egui::Ui) {
        let palette = editor_palette(root.ctx().theme());
        egui::Panel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(palette.toolbar)
                    .inner_margin(egui::Margin::symmetric(14, 9)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("OPEN CITY").strong().color(palette.brand));
                    ui.separator();
                    self.file_menu(ui);
                    ui.separator();
                    if ui
                        .add_enabled(!self.undo_stack.is_empty(), egui::Button::new("Undo"))
                        .clicked()
                    {
                        self.restore_history(true);
                    }
                    if ui
                        .add_enabled(!self.redo_stack.is_empty(), egui::Button::new("Redo"))
                        .clicked()
                    {
                        self.restore_history(false);
                    }
                    ui.separator();
                    ui.menu_button("Theme", |ui| {
                        let current = ui.options(|options| options.theme_preference);
                        for (preference, label) in [
                            (egui::ThemePreference::System, "System"),
                            (egui::ThemePreference::Light, "Light"),
                            (egui::ThemePreference::Dark, "Dark"),
                        ] {
                            if ui.selectable_label(current == preference, label).clicked() {
                                ui.ctx().set_theme(preference);
                                ui.ctx().request_repaint();
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                    if ui
                        .selectable_value(&mut self.view, View::Map, "Map")
                        .clicked()
                    {
                        self.map_start = None;
                        self.map_start_ref = None;
                    }
                    ui.selectable_value(&mut self.view, View::Section, "Cross section");
                    ui.separator();
                    if ui
                        .add(egui::Button::new((
                            icon("plus", ui.visuals().text_color()),
                            "New type",
                        )))
                        .clicked()
                    {
                        self.add_road();
                    }
                    ui.separator();
                    if self.view == View::Map {
                        if ui
                            .selectable_value(&mut self.map_tool, MapTool::Select, "Select")
                            .clicked()
                        {
                            self.map_start = None;
                            self.map_start_ref = None;
                        }
                        if ui
                            .add_enabled(
                                self.road().is_some(),
                                egui::Button::new((
                                    icon("plus", ui.visuals().text_color()),
                                    "Draw road",
                                )),
                            )
                            .clicked()
                        {
                            self.map_tool = MapTool::Draw;
                            self.map_start = None;
                            self.map_start_ref = None;
                        }
                    } else {
                        let enabled = self.road().is_some();
                        egui::ComboBox::from_id_salt("new-kind")
                            .selected_text(kind_label(self.new_kind))
                            .show_ui(ui, |ui| {
                                for kind in KINDS {
                                    ui.selectable_value(&mut self.new_kind, kind, kind_label(kind));
                                }
                            });
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Button::new((
                                    icon("plus", ui.visuals().text_color()),
                                    "Add element",
                                )),
                            )
                            .clicked()
                        {
                            self.add_element();
                        }
                        ui.toggle_value(&mut self.show_source, "Source");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let name = self
                            .path
                            .as_ref()
                            .and_then(|path| path.file_name())
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Untitled.road".into());
                        ui.label(format!("{}{}", name, if self.dirty { " •" } else { "" }));
                    });
                });
            });
    }

    fn sidebar(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("roads")
            .default_size(238.0)
            .min_size(190.0)
            .show(root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("ROAD LIBRARY")
                            .small()
                            .strong()
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.label(
                        RichText::new(if self.view == View::Map {
                            "Choose a type to draw"
                        } else {
                            "Choose a type to edit"
                        })
                        .small()
                        .color(ui.visuals().weak_text_color()),
                    );
                    let mut chosen = None;
                    for category in [
                        PresetCategory::City,
                        PresetCategory::Rural,
                        PresetCategory::Interstate,
                        PresetCategory::Custom,
                    ] {
                        let roads: Vec<_> = self
                            .program
                            .statements
                            .iter()
                            .enumerate()
                            .filter_map(|(index, statement)| match statement {
                                Statement::Road(road)
                                    if preset_category(&road.name) == category =>
                                {
                                    Some((index, road))
                                }
                                _ => None,
                            })
                            .collect();
                        if roads.is_empty() {
                            continue;
                        }
                        ui.add_space(16.0);
                        ui.label(
                            RichText::new(category.label())
                                .small()
                                .strong()
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.add_space(5.0);
                        for (index, road) in roads {
                            let response = ui.selectable_label(
                                self.selected_road == Some(index),
                                preset_label(&road.name),
                            );
                            if response.clicked() {
                                chosen = Some(index);
                            }
                            let mut tooltip = egui::Tooltip::for_enabled(&response);
                            tooltip.popup = tooltip
                                .popup
                                .align(egui::RectAlign::RIGHT_START)
                                .width(240.0);
                            tooltip.show(|ui| {
                                ui.label(RichText::new(preset_label(&road.name)).strong());
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(220.0, 34.0),
                                    egui::Sense::hover(),
                                );
                                let total: f32 = road
                                    .elements
                                    .iter()
                                    .map(|element| element_width(element.kind))
                                    .sum();
                                let mut left = rect.left();
                                let palette = editor_palette(ui.ctx().theme());
                                if total > 0.0 {
                                    for element in &road.elements {
                                        let width =
                                            rect.width() * element_width(element.kind) / total;
                                        let band = egui::Rect::from_min_size(
                                            egui::pos2(left, rect.top()),
                                            egui::vec2(width, rect.height()),
                                        );
                                        ui.painter().rect_filled(
                                            band.shrink(1.0),
                                            2.0,
                                            kind_color(element.kind, palette.theme),
                                        );
                                        if width >= 28.0 {
                                            ui.painter().text(
                                                band.center(),
                                                egui::Align2::CENTER_CENTER,
                                                kind_short(element.kind),
                                                egui::FontId::proportional(9.0),
                                                palette.tile_text,
                                            );
                                        }
                                        left += width;
                                    }
                                }
                                ui.label(format!(
                                    "{} elements | shared cross section",
                                    road.elements.len()
                                ));
                            });
                        }
                    }
                    if self.view == View::Map {
                        ui.add_space(16.0);
                        ui.label(
                            RichText::new("INTERSECTIONS")
                                .small()
                                .strong()
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.label(
                            RichText::new(format!(
                                "Edge cross section: {}",
                                self.road()
                                    .map(|road| preset_label(&road.name))
                                    .unwrap_or("first library road type")
                            ))
                            .small()
                            .color(ui.visuals().weak_text_color()),
                        );
                        for (kind, label) in [
                            (map::IntersectionKind::RightTurn, "90° turn"),
                            (map::IntersectionKind::ThreeWay, "3-way intersection"),
                            (map::IntersectionKind::FourWay, "4-way intersection"),
                        ] {
                            if ui
                                .add_enabled(
                                    self.program
                                        .statements
                                        .iter()
                                        .any(|statement| matches!(statement, Statement::Road(_))),
                                    egui::Button::new(format!("+ {label}")),
                                )
                                .clicked()
                            {
                                self.add_intersection_from_library(kind);
                            }
                        }
                    }
                    if let Some(index) = chosen {
                        self.selected_road = Some(index);
                        self.selected_element = None;
                        self.road_name_draft = self
                            .road()
                            .map(|road| road.name.clone())
                            .unwrap_or_default();
                        if self.view == View::Map {
                            self.map_tool = MapTool::Draw;
                            self.map_start = None;
                            self.map_start_ref = None;
                        }
                    }
                    if self.view == View::Map {
                        let projection = map::project(&self.program);
                        ui.add_space(18.0);
                        ui.separator();
                        ui.label(
                            RichText::new(format!(
                                "PLACED SEGMENTS | {}",
                                projection.segments.len()
                            ))
                            .small()
                            .strong()
                            .color(ui.visuals().weak_text_color()),
                        );
                        if projection.segments.is_empty() {
                            ui.label(
                                RichText::new("The map is empty. Pick a type above.")
                                    .small()
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                        for (number, segment) in projection.segments.iter().enumerate() {
                            let label = format!(
                                "{}  {} | {}",
                                number + 1,
                                segment.name.as_deref().unwrap_or("Unnamed road"),
                                preset_label(&segment.road_name)
                            );
                            if ui
                                .selectable_label(
                                    self.selected_segment == Some(segment.source),
                                    label,
                                )
                                .clicked()
                            {
                                if self.selected_segment != Some(segment.source) {
                                    self.branch_element = usize::MAX;
                                }
                                self.selected_segment = Some(segment.source);
                                self.selected_endpoint = None;
                                self.lane_end_choice = None;
                                self.segment_name_draft = segment.name.clone().unwrap_or_default();
                                self.map_tool = MapTool::Select;
                                self.map_start = None;
                                self.map_start_ref = None;
                            }
                        }
                        if projection.unresolved > 0 {
                            ui.colored_label(
                                editor_palette(ui.ctx().theme()).warning,
                                format!(
                                    "{} road call(s) need valid geometry or junctions",
                                    projection.unresolved
                                ),
                            );
                        }
                    } else {
                        let other_count = self
                            .program
                            .statements
                            .iter()
                            .filter(|statement| match statement {
                                Statement::Road(_) => false,
                                Statement::Call(call)
                                    if matches!(
                                        call.name.as_str(),
                                        "map_point"
                                            | "lane_point"
                                            | "lane_split"
                                            | "intersection2"
                                            | "intersection3"
                                            | "intersection4"
                                    ) =>
                                {
                                    false
                                }
                                _ => true,
                            })
                            .count();
                        if other_count > 0 {
                            ui.add_space(18.0);
                            ui.separator();
                            ui.label(
                                RichText::new("OTHER STATEMENTS")
                                    .small()
                                    .strong()
                                    .color(ui.visuals().weak_text_color()),
                            );
                            for statement in &self.program.statements {
                                match statement {
                                    Statement::Block(block) => {
                                        ui.label(format!(
                                            "▦  {} | {} calls",
                                            block.name,
                                            block.calls.len()
                                        ));
                                    }
                                    Statement::Call(call)
                                        if !matches!(
                                            call.name.as_str(),
                                            "map_point"
                                                | "lane_point"
                                                | "lane_split"
                                                | "intersection2"
                                                | "intersection3"
                                                | "intersection4"
                                        ) =>
                                    {
                                        ui.label(format!("↳  {}(…)", call.name));
                                    }
                                    Statement::Call(_) => {}
                                    Statement::Road(_) => {}
                                }
                            }
                        }
                    }
                });
            });
    }

    fn inspector(&mut self, root: &mut egui::Ui) {
        egui::Panel::right("inspector")
            .default_size(250.0)
            .min_size(220.0)
            .show(root, |ui| {
                ui.add_space(12.0);
                ui.label(
                    RichText::new("INSPECTOR")
                        .small()
                        .strong()
                        .color(ui.visuals().weak_text_color()),
                );
                ui.add_space(12.0);
                if self.view == View::Map {
                    self.map_inspector(ui);
                    return;
                }
                if self.road().is_none() {
                    ui.label("Select or create a road.");
                    return;
                }
                let selected = self.selected_element;
                let mut dirty = false;
                let mut move_by = 0;
                let mut remove = false;
                let mut delete_road = false;
                self.road_name_editor(ui);
                let Some(road) = self.road_mut() else {
                    ui.label("Select or create a road.");
                    return;
                };
                ui.add_space(16.0);
                ui.separator();
                if let Some(index) = selected {
                    let Some(element) = road.elements.get_mut(index) else {
                        return;
                    };
                    ui.label(
                        RichText::new(format!("ELEMENT {}", index + 1))
                            .small()
                            .strong()
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(8.0);
                    ui.label("Type");
                    let before = element.kind;
                    egui::ComboBox::from_id_salt("kind-inspector")
                        .selected_text(kind_label(element.kind))
                        .show_ui(ui, |ui| {
                            for kind in KINDS {
                                ui.selectable_value(&mut element.kind, kind, kind_label(kind));
                            }
                        });
                    if before != element.kind {
                        dirty = true;
                    }
                    ui.add_space(10.0);
                    let mut has_catch = element.catch.is_some();
                    if ui.checkbox(&mut has_catch, "Capture parameter").changed() {
                        element.catch = has_catch.then_some(1);
                        dirty = true;
                    }
                    if let Some(catch) = &mut element.catch {
                        ui.horizontal(|ui| {
                            ui.label("$");
                            if ui
                                .add(egui::DragValue::new(catch).range(0..=u32::MAX))
                                .changed()
                            {
                                dirty = true;
                            }
                        });
                    }
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if ui.button("← Move").clicked() {
                            move_by = -1;
                        }
                        if ui.button("Move →").clicked() {
                            move_by = 1;
                        }
                    });
                    if ui
                        .add(egui::Button::new((
                            icon("minus", ui.visuals().text_color()),
                            "Remove element",
                        )))
                        .clicked()
                    {
                        remove = true;
                    }
                } else {
                    ui.label("Select an element in the cross section to edit it.");
                }
                if dirty {
                    self.dirty = true;
                }
                if move_by != 0 {
                    self.move_element(move_by);
                }
                if remove {
                    self.remove_element();
                }
                ui.add_space(18.0);
                ui.separator();
                if ui
                    .add(egui::Button::new((
                        icon("x", ui.visuals().text_color()),
                        "Delete road",
                    )))
                    .clicked()
                {
                    delete_road = true;
                }
                if delete_road {
                    self.confirm_delete_road = true;
                }
            });
    }

    fn map_inspector(&mut self, ui: &mut egui::Ui) {
        let projection = map::project(&self.program);
        let profiles = paint_profiles(&self.program, &projection);
        let issues = design_rule_issues(&self.program, &projection, &profiles);
        ui.collapsing(
            format!(
                "Design checks: {} issue{}",
                issues.len(),
                if issues.len() == 1 { "" } else { "s" }
            ),
            |ui| {
                if issues.is_empty() {
                    ui.label("Road joins and intersection edges are aligned.");
                }
                for issue in &issues {
                    if ui.button(&issue.message).clicked() {
                        self.map_pan =
                            egui::vec2(-issue.at.x * self.map_zoom, issue.at.y * self.map_zoom);
                        self.fit_scene_once = false;
                    }
                }
            },
        );
        ui.add_space(10.0);
        if let Some(intersection) = map::intersections(&self.program, &projection)
            .into_iter()
            .find(|intersection| {
                self.selected_intersection.as_deref() == Some(&intersection.point_ref)
            })
        {
            ui.label(RichText::new("INTERSECTION").small().strong());
            ui.heading(match intersection.kind {
                map::IntersectionKind::RightTurn => "90° turn",
                map::IntersectionKind::ThreeWay => "3-way intersection",
                map::IntersectionKind::FourWay => "4-way intersection",
            });
            ui.label(format!(
                "Edge cross section: {}",
                preset_label(&intersection.road_name)
            ));
            let connected = intersection
                .ports
                .iter()
                .filter(|port| {
                    projection.segments.iter().any(|segment| {
                        segment.start_ref.as_deref() == Some(&port.point_ref)
                            || segment.end_ref.as_deref() == Some(&port.point_ref)
                    })
                })
                .count();
            ui.label(format!(
                "{connected} of {} ports connected",
                intersection.ports.len()
            ));
            ui.label("Drag the intersection to move it. Click a port to draw a road.");
            if !intersection.ports.is_empty()
                && intersection.kind != map::IntersectionKind::FourWay
                && ui.button("Rotate 90°").clicked()
                && map::rotate_intersection(&mut self.program, &intersection.point_ref)
            {
                self.dirty = true;
            }
            if !intersection.ports.is_empty()
                && ui.button("Delete intersection").clicked()
                && map::remove_placed_intersection(&mut self.program, &intersection.point_ref)
            {
                self.selected_intersection = None;
                self.dirty = true;
            }
            return;
        }
        let selected = self.selected_segment.and_then(|source| {
            map::project(&self.program)
                .segments
                .into_iter()
                .find(|segment| segment.source == source)
        });
        let Some(segment) = selected else {
            ui.label("Select a road on the map, or choose a type from the library to draw.");
            ui.add_space(14.0);
            if let Some(road) = self.road() {
                ui.label(
                    RichText::new("READY TO DRAW")
                        .small()
                        .strong()
                        .color(ui.visuals().weak_text_color()),
                );
                ui.heading(preset_label(&road.name));
                ui.label(format!("{} cross-section elements", road.elements.len()));
                if ui.button("Edit this cross section").clicked() {
                    self.view = View::Section;
                }
            }
            return;
        };
        ui.label(
            RichText::new("PLACED ROAD")
                .small()
                .strong()
                .color(ui.visuals().weak_text_color()),
        );
        ui.label("Road name");
        let changed = ui
            .add_enabled(
                segment.source.editable(),
                egui::TextEdit::singleline(&mut self.segment_name_draft),
            )
            .changed();
        let valid = Self::valid_name(&self.segment_name_draft);
        let duplicate = map::project(&self.program).segments.iter().any(|other| {
            other.source != segment.source
                && other.name.as_deref() == Some(self.segment_name_draft.as_str())
        });
        if !valid && !self.segment_name_draft.is_empty() {
            ui.colored_label(
                editor_palette(ui.ctx().theme()).warning,
                "Use letters, digits, and underscores.",
            );
        } else if duplicate {
            ui.colored_label(
                editor_palette(ui.ctx().theme()).warning,
                "A road already has this name.",
            );
        }
        if changed
            && valid
            && !duplicate
            && segment.source.editable()
            && map::set_name(
                &mut self.program,
                segment.source,
                self.segment_name_draft.clone(),
            )
        {
            self.dirty = true;
        }
        ui.add_space(8.0);
        ui.label("Cross section");
        let mut chosen_section = segment.road_name.clone();
        let sections: Vec<_> = self
            .program
            .statements
            .iter()
            .filter_map(|statement| match statement {
                Statement::Road(road) => Some(road.name.clone()),
                _ => None,
            })
            .collect();
        egui::ComboBox::from_id_salt("placed-road-section")
            .selected_text(preset_label(&chosen_section))
            .show_ui(ui, |ui| {
                for name in &sections {
                    ui.selectable_value(&mut chosen_section, name.clone(), preset_label(name));
                }
            });
        if chosen_section != segment.road_name
            && map::set_road_type(&mut self.program, segment.source, &chosen_section)
        {
            self.dirty = true;
        }
        if let Some((source, is_end)) = self
            .selected_endpoint
            .filter(|(source, _)| *source == segment.source)
        {
            ui.label(if is_end {
                "Far end selected"
            } else {
                "Start end selected"
            });
            ui.horizontal(|ui| {
                if ui.button("Extend / join (E)").clicked() {
                    self.start_extension(source, is_end);
                }
                if ui.button("Rotate (R)").clicked()
                    && map::rotate_endpoint(&mut self.program, source, is_end, -15.0)
                {
                    self.dirty = true;
                }
            });
            ui.label(RichText::new("Shift+R rotates the other way. Click another end while extending to join the roads.")
                .small().color(ui.visuals().weak_text_color()));
            let reference = if is_end {
                segment.end_ref.as_deref()
            } else {
                segment.start_ref.as_deref()
            };
            if let Some(reference) = reference {
                let arms = map::intersection_arms(&projection, reference).len();
                let kind = match arms {
                    2 if map::right_angle(&projection, reference) => {
                        Some(map::IntersectionKind::RightTurn)
                    }
                    3 => Some(map::IntersectionKind::ThreeWay),
                    4 => Some(map::IntersectionKind::FourWay),
                    _ => None,
                };
                if let Some(kind) = kind {
                    let defined = map::intersections(&self.program, &projection)
                        .into_iter()
                        .find(|intersection| intersection.point_ref == reference);
                    ui.label(format!("{arms}-way junction: {reference}"));
                    if let Some(intersection) = &defined {
                        ui.label(format!("Edge: {}", preset_label(&intersection.road_name)));
                    }
                    let edge_name = self.road().map(|road| road.name.clone());
                    if let (Some(intersection), Some(edge_name)) = (&defined, &edge_name) {
                        if intersection.road_name != *edge_name
                            && ui
                                .button("Use selected road type for intersection edge")
                                .clicked()
                            && map::set_intersection(
                                &mut self.program,
                                reference,
                                Some(kind),
                                Some(edge_name),
                            )
                            .is_ok()
                        {
                            self.dirty = true;
                        }
                    }
                    if ui
                        .button(if defined.is_some() {
                            "Remove intersection"
                        } else {
                            "Define intersection"
                        })
                        .clicked()
                        && map::set_intersection(
                            &mut self.program,
                            reference,
                            if defined.is_some() { None } else { Some(kind) },
                            edge_name.as_deref(),
                        )
                        .is_ok()
                    {
                        self.dirty = true;
                    }
                }
            }
        }
        if let Some(split) = map::lane_splits(&self.program).into_iter().find(|split| {
            segment.name.as_deref() == Some(&split.ramp)
                || segment.name.as_deref() == split.full_ramp.as_deref()
        }) {
            ui.label(format!(
                "Exit from {} | element {}",
                split.parent,
                split.element_index + 1
            ));
            ui.label(if split.mode == map::SplitMode::KeepLane {
                "Optional branch: the through road retains its cross section."
            } else {
                "Dedicated branch: the outer elements belong to this road."
            });
        }
        ui.add_space(8.0);
        if segment.source.editable() {
            for (label, is_end, point) in
                [("Start", false, segment.start), ("End", true, segment.end)]
            {
                ui.label(label);
                let reference = if is_end {
                    segment.end_ref.as_deref()
                } else {
                    segment.start_ref.as_deref()
                };
                if reference.is_some_and(|name| map::anchored_reference(&self.program, name)) {
                    ui.label("Attached to a parent lane");
                    ui.add_space(8.0);
                    continue;
                }
                let (mut x, mut y) = (point.x, point.y);
                let changed = ui
                    .horizontal(|ui| {
                        ui.label("X");
                        let changed_x = ui.add(egui::DragValue::new(&mut x).speed(0.05)).changed();
                        ui.label("Y");
                        let changed_y = ui.add(egui::DragValue::new(&mut y).speed(0.05)).changed();
                        changed_x || changed_y
                    })
                    .inner;
                if changed
                    && map::set_connected_endpoint(
                        &mut self.program,
                        segment.source,
                        is_end,
                        WorldPoint { x, y },
                    )
                {
                    self.dirty = true;
                }
                ui.add_space(8.0);
            }
            for reference in [&segment.start_ref, &segment.end_ref].into_iter().flatten() {
                let Some((parent_name, mut lane_index, mut t)) =
                    map::lane_attachment(&self.program, reference)
                else {
                    continue;
                };
                let parent = map::project(&self.program)
                    .segments
                    .into_iter()
                    .find(|parent| parent.name.as_deref() == Some(&parent_name));
                let elements: Vec<_> = parent
                    .and_then(|parent| {
                        self.program
                            .statements
                            .iter()
                            .find_map(|statement| match statement {
                                Statement::Road(road) if road.name == parent.road_name => Some(
                                    road.elements
                                        .iter()
                                        .enumerate()
                                        .map(|(index, element)| (index, element.kind))
                                        .collect(),
                                ),
                                _ => None,
                            })
                    })
                    .unwrap_or_default();
                ui.label(format!("Attached to {}", parent_name));
                let before_lane = lane_index;
                egui::ComboBox::from_id_salt(("attached-lane", reference))
                    .selected_text(
                        elements
                            .iter()
                            .enumerate()
                            .find(|(_, (index, _))| *index == lane_index)
                            .map(|(ordinal, (_, kind))| {
                                format!("{} {}", kind_label(*kind), ordinal + 1)
                            })
                            .unwrap_or_else(|| "Element".into()),
                    )
                    .show_ui(ui, |ui| {
                        for (ordinal, (index, kind)) in elements.iter().enumerate() {
                            ui.selectable_value(
                                &mut lane_index,
                                *index,
                                format!("{} {}", kind_label(*kind), ordinal + 1),
                            );
                        }
                    });
                let changed_t = ui
                    .add(egui::Slider::new(&mut t, 0.0..=1.0).text("Along road"))
                    .changed();
                if (before_lane != lane_index || changed_t)
                    && map::set_lane_attachment(&mut self.program, reference, lane_index, t)
                {
                    self.dirty = true;
                }
                ui.add_space(8.0);
            }
            for (label, second, point) in [
                ("Control 1", false, segment.controls[0]),
                ("Control 2", true, segment.controls[1]),
            ] {
                ui.label(label);
                let (mut x, mut y) = (point.x, point.y);
                let changed = ui
                    .horizontal(|ui| {
                        ui.label("X");
                        let changed_x = ui.add(egui::DragValue::new(&mut x).speed(0.05)).changed();
                        ui.label("Y");
                        let changed_y = ui.add(egui::DragValue::new(&mut y).speed(0.05)).changed();
                        changed_x || changed_y
                    })
                    .inner;
                if changed
                    && map::set_control(
                        &mut self.program,
                        segment.source,
                        second,
                        WorldPoint { x, y },
                    )
                {
                    self.dirty = true;
                }
                ui.add_space(8.0);
            }
            ui.label(
                RichText::new("Drag endpoints or curve controls on the map to reshape it. Connected endpoints move together.")
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
            if let Some(parent_road) =
                self.program
                    .statements
                    .iter()
                    .find_map(|statement| match statement {
                        Statement::Road(road) if road.name == segment.road_name => {
                            Some(road.clone())
                        }
                        _ => None,
                    })
            {
                let elements: Vec<_> = parent_road
                    .elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| (index, element.kind))
                    .collect();
                if !elements.is_empty() {
                    if !elements
                        .iter()
                        .any(|(index, _)| *index == self.branch_element)
                    {
                        self.branch_element = elements
                            .iter()
                            .rev()
                            .find(|(_, kind)| {
                                matches!(
                                    kind,
                                    RoadElementKind::CarLane
                                        | RoadElementKind::BusLane
                                        | RoadElementKind::BikeLane
                                )
                            })
                            .or_else(|| elements.last())
                            .map(|(index, _)| *index)
                            .unwrap_or(0);
                    }
                    ui.add_space(16.0);
                    ui.separator();
                    ui.label(RichText::new("BRANCH FROM CROSS SECTION").small().strong());
                    egui::ComboBox::from_id_salt("branch-lane")
                        .selected_text(
                            elements
                                .iter()
                                .enumerate()
                                .find(|(_, (index, _))| *index == self.branch_element)
                                .map(|(ordinal, (_, kind))| {
                                    format!("{} {}", kind_label(*kind), ordinal + 1)
                                })
                                .unwrap_or_default(),
                        )
                        .show_ui(ui, |ui| {
                            for (ordinal, (index, kind)) in elements.iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.branch_element,
                                    *index,
                                    format!("{} {}", kind_label(*kind), ordinal + 1),
                                );
                            }
                        });
                    ui.add(egui::Slider::new(&mut self.branch_t, 0.0..=1.0).text("Along road"));
                    ui.horizontal(|ui| {
                        if ui.button("Start end").clicked() {
                            self.branch_t = 0.0;
                        }
                        if ui.button("Far end").clicked() {
                            self.branch_t = 1.0;
                        }
                    });
                    ui.label(RichText::new("The selected element and every element toward this side continue into the exit. The through road keeps the remaining elements and gains a divider wall.")
                        .small().color(ui.visuals().weak_text_color()));
                    let mut keep_lane = self.branch_mode == map::SplitMode::KeepLane;
                    if ui
                        .checkbox(&mut keep_lane, "Keep lane on through road")
                        .changed()
                    {
                        self.branch_mode = if keep_lane {
                            map::SplitMode::KeepLane
                        } else {
                            map::SplitMode::Dedicated
                        };
                    }
                    ui.label(
                        RichText::new(if keep_lane {
                            "The through road keeps its full cross section; the branch copies the outer elements."
                        } else {
                            "The outer elements move to the branch; the through road uses the remaining elements."
                        })
                        .small()
                        .color(ui.visuals().weak_text_color()),
                    );
                    if ui.button("Start branch here").clicked() {
                        self.start_branch(segment.source, self.branch_element, self.branch_t);
                    }
                }
            }
            ui.add_space(12.0);
            if ui
                .add(egui::Button::new((
                    icon("x", ui.visuals().text_color()),
                    "Remove placement",
                )))
                .clicked()
                && map::remove(&mut self.program, segment.source)
            {
                self.selected_segment = None;
                self.dirty = true;
                self.status = "Road placement removed".into();
            }
        } else {
            ui.label("This segment comes from a block instance. Its placement is shown here, but block geometry is not edited on the map.");
        }
        ui.add_space(16.0);
        if ui.button("Edit cross section").clicked() {
            self.selected_road = self.program.statements.iter().position(|statement| matches!(statement, Statement::Road(road) if road.name == segment.road_name));
            self.road_name_draft = segment.road_name;
            self.selected_element = None;
            self.view = View::Section;
        }
    }

    fn map_canvas(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            let palette = editor_palette(ui.ctx().theme());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("Map");
                ui.label(
                    RichText::new(match self.map_tool {
                        MapTool::Draw => "Click freely or pick a road/lane end | hold Shift for grid snap | drag empty map to pan | Enter or Esc finishes",
                        MapTool::Select => "Drag points freely; hold Shift for grid snap | E extends, R rotates | green marker starts an exit",
                    })
                    .small()
                    .color(ui.visuals().weak_text_color()),
                );
            });
            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            self.map_rect = Some(rect);
            let before = map::project(&self.program);
            if self.fit_scene_once {
                self.fit_scene_once = false;
                if !before.segments.is_empty() {
                    let mut min = WorldPoint { x: f32::INFINITY, y: f32::INFINITY };
                    let mut max = WorldPoint { x: f32::NEG_INFINITY, y: f32::NEG_INFINITY };
                    for segment in &before.segments {
                        for point in [segment.start, segment.controls[0], segment.controls[1], segment.end] {
                            min.x = min.x.min(point.x);
                            min.y = min.y.min(point.y);
                            max.x = max.x.max(point.x);
                            max.y = max.y.max(point.y);
                        }
                    }
                    let width = (max.x - min.x).max(0.1);
                    let height = (max.y - min.y).max(0.1);
                    self.map_zoom = ((rect.width() - 76.0).max(80.0) / width)
                        .min((rect.height() - 76.0).max(80.0) / height)
                        .clamp(18.0, 120.0);
                    self.map_pan = egui::vec2(
                        -(min.x + max.x) * self.map_zoom / 2.0,
                        (min.y + max.y) * self.map_zoom / 2.0,
                    );
                }
            }
            if response.hovered() {
                let scroll = ui.input(|input| input.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    let pointer = ui.ctx().pointer_hover_pos().unwrap_or(rect.center());
                    let anchored = world_of(rect, self.map_pan, self.map_zoom, pointer);
                    self.map_zoom = (self.map_zoom * (1.0 + scroll * 0.002)).clamp(18.0, 420.0);
                    self.map_pan = pointer
                        - rect.center()
                        - egui::vec2(anchored.x * self.map_zoom, -anchored.y * self.map_zoom);
                }
            }
            let junctions = junction_candidates(&self.program, &before);
            let element_ends = element_end_candidates(&before, &self.program);
            if response.drag_started_by(egui::PointerButton::Primary) {
                let origin = ui
                    .input(|input| input.pointer.press_origin())
                    .or_else(|| response.interact_pointer_pos());
                if let Some(origin) = origin.filter(|point| rect.contains(*point)) {
                    self.dragging_intersection = if self.map_tool == MapTool::Select {
                        placed_intersection_at(&self.program, &before, rect,
                            self.map_pan, self.map_zoom, origin)
                    } else { None };
                    self.dragging_endpoint = if self.map_tool == MapTool::Select {
                        self.dragging_intersection.is_none().then(|| endpoint_near(
                            &before.segments,
                            &self.program,
                            rect,
                            self.map_pan,
                            self.map_zoom,
                            origin,
                        )).flatten()
                    } else {
                        None
                    };
                    self.dragging_control = if self.map_tool == MapTool::Select
                        && self.dragging_endpoint.is_none()
                        && self.dragging_intersection.is_none()
                    {
                        control_near(&before.segments, rect, self.map_pan, self.map_zoom, origin)
                    } else {
                        None
                    };
                    if let Some((source, is_end)) = self.dragging_endpoint {
                        self.selected_segment = Some(source);
                        self.selected_endpoint = Some((source, is_end));
                        self.lane_end_choice = None;
                    }
                    if let Some((source, _)) = self.dragging_control {
                        self.selected_segment = Some(source);
                        self.selected_endpoint = None;
                        self.lane_end_choice = None;
                    }
                    if let Some(segment) = before
                        .segments
                        .iter()
                        .find(|segment| Some(segment.source) == self.selected_segment)
                    {
                        self.segment_name_draft = segment.name.clone().unwrap_or_default();
                    }
                    self.dragging_background = self.dragging_endpoint.is_none()
                        && self.dragging_control.is_none()
                        && self.dragging_intersection.is_none()
                        && segment_near(
                            &before.segments,
                            &self.program,
                            rect,
                            self.map_pan,
                            self.map_zoom,
                            origin,
                        )
                        .is_none();
                }
            }
            if response.dragged_by(egui::PointerButton::Secondary)
                || (self.dragging_background && response.dragged_by(egui::PointerButton::Primary))
            {
                self.map_pan += response.drag_delta();
            }
            if ui.input(|input| {
                input.key_pressed(egui::Key::Escape)
                    || (self.map_tool == MapTool::Draw && input.key_pressed(egui::Key::Enter))
            }) {
                if let Some(reference) = self.map_start_ref.as_deref() {
                    if map::anchored_reference(&self.program, reference) {
                        map::remove_unused_point(&mut self.program, reference);
                    }
                }
                self.map_start = None;
                self.map_start_ref = None;
                self.dragging_endpoint = None;
                self.dragging_control = None;
                self.dragging_background = false;
                self.map_tool = MapTool::Select;
                self.lane_end_choice = None;
            }
            if self.map_tool == MapTool::Select
                && self.selected_intersection.is_some()
                && !ui.ctx().egui_wants_keyboard_input()
                && ui.input(|input| input.key_pressed(egui::Key::Backspace)
                    || input.key_pressed(egui::Key::Delete))
            {
                let reference = self.selected_intersection.take().unwrap();
                if map::remove_placed_intersection(&mut self.program, &reference) {
                    self.dirty = true;
                    self.status = "Intersection removed".into();
                }
            }
            if self.map_tool == MapTool::Select
                && self.selected_segment.is_some()
                && !ui.ctx().egui_wants_keyboard_input()
                && ui.input(|input| {
                    input.key_pressed(egui::Key::Backspace) || input.key_pressed(egui::Key::Delete)
                })
            {
                if map::remove(&mut self.program, self.selected_segment.unwrap()) {
                    self.selected_segment = None;
                    self.selected_endpoint = None;
                    self.lane_end_choice = None;
                    self.segment_name_draft.clear();
                    self.dirty = true;
                    self.status = "Road placement removed".into();
                }
            }
            if self.map_tool == MapTool::Select && !ui.ctx().egui_wants_keyboard_input() {
                if let Some((source, is_end)) = self.selected_endpoint {
                    if ui.input(|input| input.key_pressed(egui::Key::E)) {
                        self.start_extension(source, is_end);
                    } else if let Some(reverse) = ui.input(|input| input.key_pressed(egui::Key::R)
                        .then_some(input.modifiers.shift)) {
                        if map::rotate_endpoint(&mut self.program, source, is_end,
                            if reverse { 15.0 } else { -15.0 }) {
                            self.dirty = true;
                            self.status = "Rotated road end tangent 15° | Shift+R reverses direction".into();
                        }
                    }
                }
            }
            if let Some(pointer) = ui
                .ctx()
                .pointer_hover_pos()
                .filter(|point| rect.contains(*point))
            {
                let pointer_world = world_of(rect, self.map_pan, self.map_zoom, pointer);
                let free_point = if ui.input(|input| input.modifiers.shift) {
                    grid_point(pointer_world)
                } else {
                    pointer_world
                };
                let port_hit = intersection_port_near(&self.program, &before, rect,
                    self.map_pan, self.map_zoom, pointer);
                let port_mismatch = self.map_tool == MapTool::Draw
                    && response.clicked_by(egui::PointerButton::Primary)
                    && port_hit.as_ref().is_some_and(|(_, _, road_name)|
                        self.road().is_some_and(|road| road.name != *road_name));
                if port_mismatch {
                    self.status = "This port needs a road with the intersection's cross section".into();
                }
                let port_busy = port_hit.as_ref().is_some_and(|(_, reference, _)|
                    map::port_in_use(&before, reference));
                if self.map_tool == MapTool::Draw && port_busy
                    && response.clicked_by(egui::PointerButton::Primary) {
                    self.status = "This intersection port already has a road".into();
                }
                let selecting_port = self.map_tool == MapTool::Select;
                if self.map_tool == MapTool::Draw
                    && response.clicked_by(egui::PointerButton::Primary)
                    && !port_mismatch
                    && !port_busy
                {
                    let matched_port = port_hit.as_ref().filter(|(_, _, road_name)|
                        self.road().is_some_and(|road| road.name == *road_name));
                    let target_endpoint = endpoint_near(&before.segments, &self.program,
                        rect, self.map_pan, self.map_zoom, pointer)
                        .filter(|(source, is_end)| {
                            let segment = before.segments.iter().find(|segment| segment.source == *source).unwrap();
                            let point = if *is_end { segment.end } else { segment.start };
                            pointer.distance(screen_of(rect, self.map_pan, self.map_zoom, point)) <= 9.0
                        });
                    let element_end = if target_endpoint.is_none() {
                        element_end_near(&element_ends, rect, self.map_pan, self.map_zoom, pointer)
                    } else { None };
                    let picked = matched_port.map(|(point, reference, _)| (*point, Some(reference.clone())))
                        .or_else(|| target_endpoint.and_then(|(source, is_end)|
                        map::link_endpoint(&mut self.program, source, is_end)
                            .map(|(point, reference)| (point, Some(reference)))))
                        .or_else(|| element_end.map(|candidate| (candidate.position, None)))
                        .or_else(|| junction_near(&junctions, rect, self.map_pan, self.map_zoom, pointer));
                    let (point, mut point_ref) = picked.unwrap_or((free_point, None));
                    if self.map_start != Some(point) {
                        if let Some(candidate) = element_end {
                            point_ref = map::create_lane_point(
                                &mut self.program,
                                candidate.source,
                                candidate.element_index,
                                candidate.t,
                            )
                            .map(|(reference, _)| reference);
                        }
                    }
                    if let Some(start) = self.map_start {
                        let faces_ports = self.map_start_ref.as_deref().is_none_or(|reference|
                            map::port_faces_road(&self.program, reference, point))
                            && point_ref.as_deref().is_none_or(|reference|
                                map::port_faces_road(&self.program, reference, start));
                        if !faces_ports {
                            self.status = "Draw away from the intersection face of each port".into();
                        }
                        if start != point && faces_ports {
                            if let Some(road) = self.road().cloned() {
                                let branch_ref = self.map_start_ref.clone();
                                let exit = branch_ref.as_deref()
                                    .filter(|reference| map::anchored_reference(&self.program, reference))
                                    .and_then(|reference| map::append_exit(
                                        &mut self.program, &road, self.branch_mode, start, reference, point,
                                        point_ref.as_deref()));
                                let (index, end_ref, exit_side, through_added) = if let Some(exit) = exit {
                                    map::create_two_segment_split(&mut self.program,
                                        branch_ref.as_deref().unwrap(), exit.entry, exit.full,
                                        self.branch_mode);
                                    let SegmentSource::TopLevel(index) = exit.full else { unreachable!() };
                                    (index, exit.end_ref, Some(exit.side), exit.through.is_some())
                                } else {
                                    let (index, end_ref) = map::append_connected(
                                        &mut self.program, &road, start,
                                        self.map_start_ref.as_deref(), point,
                                        point_ref.as_deref());
                                    if let Some(reference) = branch_ref.as_deref() {
                                        map::create_lane_split(&mut self.program, reference,
                                            SegmentSource::TopLevel(index), self.branch_mode);
                                    }
                                    (index, end_ref, None, false)
                                };
                                self.selected_segment = Some(SegmentSource::TopLevel(index));
                                if exit_side.is_some() {
                                    let segment_road = map::project(&self.program).segments.into_iter()
                                        .find(|segment| segment.source == SegmentSource::TopLevel(index))
                                        .map(|segment| segment.road_name);
                                    if let Some(name) = segment_road {
                                        self.selected_road = self.program.statements.iter().position(
                                            |statement| matches!(statement, Statement::Road(road) if road.name == name));
                                        self.road_name_draft = name;
                                    }
                                }
                                self.segment_name_draft = map::project(&self.program)
                                    .segments
                                    .into_iter()
                                    .find(|segment| {
                                        segment.source == SegmentSource::TopLevel(index)
                                    })
                                    .and_then(|segment| segment.name)
                                    .unwrap_or_default();
                                self.map_start = Some(point);
                                self.map_start_ref = Some(end_ref);
                                self.dirty = true;
                                self.status = if let Some(side) = exit_side {
                                    if through_added {
                                        format!("Placed {} exit and a divided through road | click another endpoint or press Enter/Esc", side.label())
                                    } else {
                                        format!("Placed {} branch with carried cross section | click another endpoint or press Enter/Esc", side.label())
                                    }
                                } else {
                                    format!("Placed {} | click another endpoint or press Enter/Esc",
                                        preset_label(&road.name))
                                };
                                if self.map_start_ref.as_deref().is_some_and(|reference|
                                    map::intersection_port_direction(&self.program, reference).is_some()) {
                                    self.map_start = None;
                                    self.map_start_ref = None;
                                    self.map_tool = MapTool::Select;
                                    self.status = "Road connected to intersection port".into();
                                }
                            }
                        }
                    } else {
                        self.map_start = Some(point);
                        self.map_start_ref = point_ref;
                    }
                }
                let clicked_port = port_hit.is_some()
                    && response.clicked_by(egui::PointerButton::Primary);
                if selecting_port && self.map_tool == MapTool::Select
                    && response.clicked_by(egui::PointerButton::Primary) {
                    if let Some((position, reference, road_name)) = port_hit {
                        if map::port_in_use(&before, &reference) {
                            self.status = "This intersection port already has a road".into();
                        } else {
                        self.selected_road = self.program.statements.iter().position(|statement|
                            matches!(statement, Statement::Road(road) if road.name == road_name));
                        self.road_name_draft = road_name;
                        self.map_tool = MapTool::Draw;
                        self.map_start = Some(position);
                        self.map_start_ref = Some(reference);
                        self.selected_segment = None;
                        self.selected_intersection = None;
                        self.selected_endpoint = None;
                        self.status = "Drawing from intersection port | click to place the road end".into();
                        }
                    }
                }
                if self.map_tool == MapTool::Select && !clicked_port {
                    let body_hit = response.clicked_by(egui::PointerButton::Primary)
                        .then(|| placed_intersection_at(&self.program, &before, rect,
                            self.map_pan, self.map_zoom, pointer)).flatten();
                    if let Some(reference) = body_hit.as_ref() {
                        self.selected_intersection = Some(reference.clone());
                        self.selected_segment = None;
                        self.selected_endpoint = None;
                        self.lane_end_choice = None;
                    } else if response.clicked_by(egui::PointerButton::Primary) {
                        self.selected_intersection = None;
                        let picked_endpoint = endpoint_near(&before.segments, &self.program,
                            rect, self.map_pan, self.map_zoom, pointer)
                            .filter(|(source, is_end)| {
                                let segment = before.segments.iter()
                                    .find(|segment| segment.source == *source).unwrap();
                                let point = if *is_end { segment.end } else { segment.start };
                                pointer.distance(screen_of(rect, self.map_pan, self.map_zoom, point)) <= 9.0
                            });
                        let branch_marker = self.selected_segment.and_then(|source| {
                            let segment = before.segments.iter().find(|segment| segment.source == source)?;
                            let road = self.program.statements.iter().find_map(|statement| match statement {
                                Statement::Road(road) if road.name == segment.road_name => Some(road), _ => None,
                            })?;
                            let point = map::element_position(segment, road, self.branch_element, self.branch_t)?;
                            (pointer.distance(screen_of(rect, self.map_pan, self.map_zoom, point)) <= 10.0)
                                .then_some((source, self.branch_element, self.branch_t))
                        });
                        if let Some((source, is_end)) = picked_endpoint {
                            self.lane_end_choice = None;
                            if self.selected_segment != Some(source) {
                                self.branch_element = usize::MAX;
                            }
                            self.selected_segment = Some(source);
                            self.selected_endpoint = Some((source, is_end));
                            self.segment_name_draft = before.segments.iter()
                                .find(|segment| segment.source == source)
                                .and_then(|segment| segment.name.clone()).unwrap_or_default();
                            self.status = "Road end selected | E extends or joins; R rotates the tangent".into();
                        } else if let Some((source, element, t)) = branch_marker {
                            self.lane_end_choice = None;
                            self.start_branch(source, element, t);
                        } else if let Some(candidate) = element_end_near(&element_ends, rect,
                            self.map_pan, self.map_zoom, pointer) {
                            self.branch_element = candidate.element_index;
                            self.branch_t = candidate.t;
                            self.selected_segment = Some(candidate.source);
                            self.selected_endpoint = None;
                            self.lane_end_choice = Some(candidate);
                            self.segment_name_draft = before.segments.iter()
                                .find(|segment| segment.source == candidate.source)
                                .and_then(|segment| segment.name.clone()).unwrap_or_default();
                        } else {
                        self.lane_end_choice = None;
                        let selected = segment_near(
                            &before.segments,
                            &self.program,
                            rect,
                            self.map_pan,
                            self.map_zoom,
                            pointer,
                        );
                        if selected != self.selected_segment {
                            self.branch_element = usize::MAX;
                        }
                        self.selected_segment = selected;
                        self.selected_endpoint = None;
                        self.segment_name_draft = before
                            .segments
                            .iter()
                            .find(|segment| Some(segment.source) == self.selected_segment)
                            .and_then(|segment| segment.name.clone())
                            .unwrap_or_default();
                        }
                    }
                    if response.dragged_by(egui::PointerButton::Primary) {
                        if let Some(reference) = self.dragging_intersection.as_deref() {
                            let delta = response.drag_delta();
                            if map::translate_intersection(&mut self.program, reference,
                                WorldPoint { x: delta.x / self.map_zoom, y: -delta.y / self.map_zoom }) {
                                self.selected_intersection = Some(reference.to_owned());
                                self.dirty = true;
                            }
                        }
                        if let Some((source, is_end)) = self.dragging_endpoint {
                            if map::set_connected_endpoint(&mut self.program, source, is_end, free_point)
                            {
                                self.dirty = true;
                            }
                        }
                        if let Some((source, second)) = self.dragging_control {
                            if map::set_control(&mut self.program, source, second, free_point) {
                                self.dirty = true;
                            }
                        }
                    }
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                if let (Some((source, is_end)), Some(pointer)) =
                    (self.dragging_endpoint, response.interact_pointer_pos()) {
                    let projection = map::project(&self.program);
                    if let Some((_, reference, road_name)) = intersection_port_near(
                        &self.program, &projection, rect, self.map_pan, self.map_zoom, pointer) {
                        let matching = projection.segments.iter().any(|segment|
                            segment.source == source && segment.road_name == road_name);
                        if matching && map::attach_endpoint_to_port(&mut self.program,
                            source, is_end, &reference) {
                            self.dirty = true;
                            self.status = "Road end connected to intersection port".into();
                        } else if !matching {
                            self.status = "This port needs a road with the intersection's cross section".into();
                        }
                    }
                }
                self.dragging_endpoint = None;
                self.dragging_control = None;
                self.dragging_intersection = None;
                self.dragging_background = false;
            }

            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, palette.map_background);
            paint_grid(&painter, rect, self.map_pan, self.map_zoom, palette);
            let projection = map::project(&self.program);
            let profiles = paint_profiles(&self.program, &projection);
            let issues = design_rule_issues(&self.program, &projection, &profiles);
            let mut draw_order: Vec<_> = (0..projection.segments.len()).collect();
            draw_order.sort_by_key(|index| {
                profiles[*index].start_alignment.is_some()
                    || profiles[*index].merge_alignment.is_some()
                    || !profiles[*index].start_taper.is_empty()
                    || !profiles[*index].end_taper.is_empty()
            });
            for index in draw_order {
                let segment = &projection.segments[index];
                let selected = self.selected_segment == Some(segment.source);
                let road = self
                    .program
                    .statements
                    .iter()
                    .find_map(|statement| match statement {
                        Statement::Road(road) if road.name == segment.road_name => Some(road),
                        _ => None,
                    });
                if let Some(road) = road.filter(|_| {
                    profiles[index].start_alignment.is_some()
                        || profiles[index].merge_alignment.is_some()
                        || !profiles[index].start_taper.is_empty()
                        || !profiles[index].end_taper.is_empty()
                }) {
                    paint_profiled_road(&painter, rect, self.map_pan, self.map_zoom,
                        segment, road, selected, palette, &profiles[index]);
                } else {
                    paint_road_segment(&painter, rect, self.map_pan, self.map_zoom,
                        segment, road, selected, palette);
                }
                let midpoint = screen_of(rect, self.map_pan, self.map_zoom, segment.point_at(0.5));
                let screen_length = screen_of(rect, self.map_pan, self.map_zoom, segment.start)
                    .distance(screen_of(rect, self.map_pan, self.map_zoom, segment.end));
                if selected || screen_length > 100.0 {
                    painter.text(
                        midpoint + egui::vec2(0.0, -22.0),
                        egui::Align2::CENTER_BOTTOM,
                        segment.name.as_deref().unwrap_or(&segment.road_name),
                        egui::FontId::proportional(12.0),
                        palette.map_text,
                    );
                }
            }
            let intersections = map::intersections(&self.program, &projection);
            let intersection_squares: Vec<_> = intersections.iter().filter_map(|intersection|
                intersection_paint(&self.program, &projection, intersection, rect,
                    self.map_pan, self.map_zoom).map(|paint| paint.square)).collect();
            for intersection in &intersections {
                paint_intersection(&painter, rect, self.map_pan, self.map_zoom,
                    &self.program, &projection, intersection, palette);
                if self.selected_intersection.as_deref() == Some(&intersection.point_ref) {
                    if let Some(paint) = intersection_paint(&self.program, &projection,
                        intersection, rect, self.map_pan, self.map_zoom) {
                        painter.rect_stroke(paint.square, 0.0,
                            egui::Stroke::new(2.0, palette.map_selected), egui::StrokeKind::Inside);
                    }
                }
                for port in &intersection.ports {
                    let point = screen_of(rect, self.map_pan, self.map_zoom, port.position);
                    let center = screen_of(rect, self.map_pan, self.map_zoom, intersection.position);
                    let radius = (point.distance(center) * 0.35).clamp(2.5, 6.0);
                    let connected = projection.segments.iter().any(|segment|
                        segment.start_ref.as_deref() == Some(&port.point_ref)
                            || segment.end_ref.as_deref() == Some(&port.point_ref));
                    painter.circle_filled(point, radius, palette.map_background);
                    painter.circle_stroke(point, radius,
                        egui::Stroke::new(2.0, if connected { palette.map_subtle } else { palette.map_preview }));
                }
            }
            if let Some(segment) = projection.segments.iter()
                .find(|segment| Some(segment.source) == self.selected_segment) {
                for point in [segment.start, segment.end]
                    .map(|point| screen_of(rect, self.map_pan, self.map_zoom, point)) {
                    if intersection_squares.iter().any(|square| square.contains(point)) {
                        continue;
                    }
                    painter.circle_filled(point, 7.0, palette.map_text);
                    painter.circle_stroke(point, 7.0,
                        egui::Stroke::new(2.0, palette.map_selected));
                }
                for (anchor, control) in [
                    (segment.start, segment.controls[0]),
                    (segment.end, segment.controls[1]),
                ] {
                    let a = screen_of(rect, self.map_pan, self.map_zoom, anchor);
                    let b = screen_of(rect, self.map_pan, self.map_zoom, control);
                    if intersection_squares.iter().any(|square| square.contains(a)) {
                        continue;
                    }
                    painter.line_segment([a, b], egui::Stroke::new(1.0, palette.map_selected));
                    painter.circle_filled(b, 5.0, palette.map_selected);
                }
            }
            if self.map_tool == MapTool::Select {
                if let Some(segment) = projection
                    .segments
                    .iter()
                    .find(|segment| Some(segment.source) == self.selected_segment)
                {
                    if let Some(road) =
                        self.program
                            .statements
                            .iter()
                            .find_map(|statement| match statement {
                                Statement::Road(road) if road.name == segment.road_name => {
                                    Some(road)
                                }
                                _ => None,
                            })
                    {
                        if let Some(position) =
                            map::element_position(segment, road, self.branch_element, self.branch_t)
                        {
                            let point = screen_of(rect, self.map_pan, self.map_zoom, position);
                            painter.circle_filled(point, 5.0, palette.map_preview);
                            painter.circle_stroke(
                                point,
                                7.0,
                                egui::Stroke::new(2.0, palette.map_text),
                            );
                            if ui.ctx().pointer_hover_pos().is_some_and(|pointer|
                                pointer.distance(point) <= 12.0) {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                painter.text(point + egui::vec2(12.0, -12.0),
                                    egui::Align2::LEFT_BOTTOM, "Click to branch here",
                                    egui::FontId::proportional(11.0), palette.map_text);
                            }
                        }
                    }
                }
            }
            if matches!(self.map_tool, MapTool::Draw | MapTool::Select) {
                if let Some(pointer) = ui
                    .ctx()
                    .pointer_hover_pos()
                    .filter(|point| rect.contains(*point))
                {
                    let element_ends: Vec<_> = element_end_candidates(&projection, &self.program)
                        .into_iter().filter(|candidate| !intersection_squares.iter().any(|square|
                            square.contains(screen_of(rect, self.map_pan, self.map_zoom,
                                candidate.position)))).collect();
                    let hovered_end =
                        element_end_near(&element_ends, rect, self.map_pan, self.map_zoom, pointer);
                    for candidate in element_ends.iter().filter(|candidate| {
                        pointer.distance(screen_of(
                            rect,
                            self.map_pan,
                            self.map_zoom,
                            candidate.position,
                        )) <= 90.0
                    }) {
                        let screen =
                            screen_of(rect, self.map_pan, self.map_zoom, candidate.position);
                        let hovered = hovered_end.is_some_and(|near| {
                            near.source == candidate.source
                                && near.element_index == candidate.element_index
                                && near.t == candidate.t
                        });
                        painter.circle_filled(
                            screen,
                            if hovered { 6.0 } else { 3.5 },
                            palette.map_background,
                        );
                        painter.circle_stroke(
                            screen,
                            if hovered { 6.0 } else { 3.5 },
                            egui::Stroke::new(1.5, palette.map_preview),
                        );
                    }
                    if let Some(candidate) = hovered_end {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        let screen =
                            screen_of(rect, self.map_pan, self.map_zoom, candidate.position);
                        painter.text(
                            screen + egui::vec2(11.0, -12.0),
                            egui::Align2::LEFT_BOTTOM,
                            format!(
                                "{} {} end | click for options",
                                kind_label(candidate.kind),
                                candidate.element_index + 1
                            ),
                            egui::FontId::proportional(11.0),
                            palette.map_text,
                        );
                    }
                    for (point, reference) in junction_candidates(&self.program, &projection) {
                        let screen = screen_of(rect, self.map_pan, self.map_zoom, point);
                        if intersection_squares.iter().any(|square| square.contains(screen)) {
                            continue;
                        }
                        let distance = pointer.distance(screen);
                        if distance <= 90.0 {
                            let hovered = distance <= 16.0;
                            painter.circle_filled(
                                screen,
                                if hovered { 8.0 } else { 5.0 },
                                palette.map_background,
                            );
                            painter.circle_stroke(
                                screen,
                                if hovered { 8.0 } else { 5.0 },
                                egui::Stroke::new(2.0, palette.map_preview),
                            );
                            if hovered {
                                painter.text(
                                    screen + egui::vec2(10.0, -11.0),
                                    egui::Align2::LEFT_BOTTOM,
                                    reference.as_deref().unwrap_or("Connect here"),
                                    egui::FontId::proportional(11.0),
                                    palette.map_text,
                                );
                            }
                        }
                    }
                }
            }
            if let Some(start) = self.map_start {
                let a = screen_of(rect, self.map_pan, self.map_zoom, start);
                painter.circle_filled(a, 5.0, palette.map_preview);
                if let Some(pointer) = ui
                    .ctx()
                    .pointer_hover_pos()
                    .filter(|point| rect.contains(*point))
                {
                    let stroke = egui::Stroke::new(3.0, palette.map_preview);
                    let end = endpoint_near(&projection.segments, &self.program,
                            rect, self.map_pan, self.map_zoom, pointer)
                        .and_then(|(source, is_end)| projection.segments.iter()
                            .find(|segment| segment.source == source)
                            .map(|segment| if is_end { segment.end } else { segment.start }))
                        .or_else(|| element_end_near(&element_ends, rect, self.map_pan, self.map_zoom, pointer)
                            .map(|candidate| candidate.position)
                        )
                            .or_else(|| {
                                junction_near(
                                    &junctions,
                                    rect,
                                    self.map_pan,
                                    self.map_zoom,
                                    pointer,
                                )
                                .map(|(point, _)| point)
                            })
                            .map(|point| screen_of(rect, self.map_pan, self.map_zoom, point))
                            .unwrap_or(pointer);
                    painter.line_segment([a, end], stroke);
                }
            }
            painter.text(
                rect.left_top() + egui::vec2(18.0, 18.0),
                egui::Align2::LEFT_TOP,
                format!(
                    "{} segments  |  {} road types  |  {} design issues",
                    projection.segments.len(),
                    self.program
                        .statements
                        .iter()
                        .filter(|statement| matches!(statement, Statement::Road(_)))
                        .count(),
                    issues.len()
                ),
                egui::FontId::proportional(13.0),
                palette.map_subtle,
            );
            let north = rect.right_top() + egui::vec2(-24.0, 34.0);
            painter.text(
                north + egui::vec2(0.0, 9.0),
                egui::Align2::CENTER_TOP,
                "N",
                egui::FontId::proportional(14.0),
                palette.map_subtle,
            );
            painter.arrow(
                north,
                egui::vec2(0.0, -14.0),
                egui::Stroke::new(1.5, palette.map_subtle),
            );
            painter.text(
                rect.right_bottom() + egui::vec2(-18.0, -16.0),
                egui::Align2::RIGHT_BOTTOM,
                format!("1 unit = {:.0} px", self.map_zoom),
                egui::FontId::proportional(12.0),
                palette.map_subtle,
            );
            if projection.segments.is_empty() {
                painter.text(
                    rect.center() + egui::vec2(0.0, -18.0),
                    egui::Align2::CENTER_CENTER,
                    "Your map starts here",
                    egui::FontId::proportional(24.0),
                    palette.map_text,
                );
                painter.text(
                    rect.center() + egui::vec2(0.0, 18.0),
                    egui::Align2::CENTER_CENTER,
                    "Choose a road type on the left, then click two points.",
                    egui::FontId::proportional(14.0),
                    palette.map_subtle,
                );
            }
            if let Some(choice) = self.lane_end_choice {
                let anchor = screen_of(rect, self.map_pan, self.map_zoom, choice.position);
                painter.circle_stroke(anchor, 8.0, egui::Stroke::new(2.5, palette.map_selected));
                let menu_pos = egui::pos2(
                    (anchor.x + 12.0).min(rect.right() - 190.0),
                    (anchor.y + 12.0).min(rect.bottom() - 150.0),
                );
                let mut action = None;
                egui::Area::new(egui::Id::new("lane_end_choice"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(menu_pos)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.label(format!("{} {} end", kind_label(choice.kind), choice.element_index + 1));
                            if ui.button("Continue full road").clicked() { action = Some(0); }
                            if ui.button("Exit / merge from this lane").clicked() { action = Some(1); }
                            if ui.button("Join full road to another end").clicked() { action = Some(2); }
                        });
                    });
                if let Some(action) = action {
                    self.lane_end_choice = None;
                    match action {
                        1 => { self.start_branch(choice.source, choice.element_index, choice.t); }
                        2 => {
                            if self.start_extension(choice.source, choice.t >= 0.5) {
                                self.status = "Join roads | click another road end with the same cross section".into();
                            }
                        }
                        _ => { self.start_extension(choice.source, choice.t >= 0.5); }
                    }
                }
            }
        });
    }

    fn section_canvas(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            let palette = editor_palette(ui.ctx().theme());
            ui.add_space(18.0);
            let Some(road) = self.road() else {
                ui.vertical_centered(|ui| {
                    ui.add_space(90.0);
                    ui.add(icon("box", ui.visuals().text_color()));
                    ui.heading("No road selected");
                    ui.label("Create a road from the toolbar, or open a .road file.");
                });
                return;
            };
            ui.heading(&road.name);
            ui.label(
                RichText::new("CROSS SECTION | left to right in source order")
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(25.0);
            let elements = road.elements.clone();
            let mut clicked = None;
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, element) in elements.iter().enumerate() {
                        let selected = self.selected_element == Some(index);
                        let width = element_width(element.kind);
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(width, 260.0), egui::Sense::click());
                        let fill = kind_color(element.kind, ui.ctx().theme());
                        ui.painter()
                            .rect_filled(rect.shrink2(egui::vec2(2.0, 8.0)), 5.0, fill);
                        if selected {
                            ui.painter().rect_stroke(
                                rect.shrink(2.0),
                                5.0,
                                egui::Stroke::new(3.0, palette.map_selected),
                                egui::StrokeKind::Outside,
                            );
                        }
                        let center = rect.center();
                        ui.painter().text(
                            egui::pos2(center.x, center.y - 12.0),
                            egui::Align2::CENTER_CENTER,
                            kind_short(element.kind),
                            egui::FontId::proportional(13.0),
                            palette.tile_text,
                        );
                        if let Some(catch) = element.catch {
                            ui.painter().text(
                                egui::pos2(center.x, center.y + 15.0),
                                egui::Align2::CENTER_CENTER,
                                format!("${catch}"),
                                egui::FontId::monospace(12.0),
                                palette.tile_text,
                            );
                        }
                        if response.clicked() {
                            clicked = Some(index);
                        }
                        response
                            .on_hover_text(format!("{} | click to edit", kind_label(element.kind)));
                    }
                });
            });
            if let Some(index) = clicked {
                self.selected_element = Some(index);
            }
            ui.add_space(20.0);
            ui.label(
                RichText::new(format!(
                    "{} elements  |  Select to inspect  |  Add after selection",
                    elements.len()
                ))
                .color(ui.visuals().weak_text_color()),
            );
            if self.show_source {
                ui.add_space(20.0);
                ui.separator();
                ui.label(RichText::new("SERIALIZED SOURCE").small().strong());
                let mut source = self.program.to_road_source();
                egui::ScrollArea::vertical()
                    .max_height(250.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut source)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .interactive(false),
                        );
                    });
            }
        });
    }

    fn pending_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending else {
            return;
        };
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Discard changes to the current document?");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending = None;
                    }
                    if ui.button("Discard changes").clicked() {
                        self.pending = None;
                        match action {
                            PendingAction::New => self.new_document(),
                            PendingAction::Open => self.pick_open(),
                        }
                    }
                });
            });
    }

    fn delete_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_delete_road {
            return;
        }
        egui::Window::new("Delete road?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(
                    "Delete this road definition? Calls that name it will remain in the document.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.confirm_delete_road = false;
                    }
                    if ui.button("Delete road").clicked() {
                        self.confirm_delete_road = false;
                        self.remove_road();
                    }
                });
            });
    }
}

fn screen_of(rect: egui::Rect, pan: egui::Vec2, zoom: f32, point: WorldPoint) -> egui::Pos2 {
    rect.center() + pan + egui::vec2(point.x * zoom, -point.y * zoom)
}

fn world_of(rect: egui::Rect, pan: egui::Vec2, zoom: f32, point: egui::Pos2) -> WorldPoint {
    let relative = point - rect.center() - pan;
    WorldPoint {
        x: relative.x / zoom,
        y: -relative.y / zoom,
    }
}

fn grid_point(point: WorldPoint) -> WorldPoint {
    WorldPoint {
        x: (point.x * 4.0).round() / 4.0,
        y: (point.y * 4.0).round() / 4.0,
    }
}

fn junction_candidates(
    program: &Program,
    projection: &map::Projection,
) -> Vec<(WorldPoint, Option<String>)> {
    let placed_centers: Vec<_> = map::intersections(program, projection)
        .into_iter()
        .filter(|intersection| !intersection.ports.is_empty())
        .map(|intersection| intersection.point_ref)
        .collect();
    let mut candidates: Vec<_> = projection
        .points
        .iter()
        .filter(|point| !placed_centers.contains(&point.name))
        .map(|point| (point.position, Some(point.name.clone())))
        .collect();
    for segment in &projection.segments {
        for (position, reference) in [
            (segment.start, &segment.start_ref),
            (segment.end, &segment.end_ref),
        ] {
            if !candidates
                .iter()
                .any(|(candidate, _)| *candidate == position)
            {
                candidates.push((
                    position,
                    if segment.source.editable() {
                        reference.clone()
                    } else {
                        None
                    },
                ));
            }
        }
    }
    candidates
}

fn junction_near(
    candidates: &[(WorldPoint, Option<String>)],
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<(WorldPoint, Option<String>)> {
    candidates
        .iter()
        .filter_map(|(position, reference)| {
            let distance = pointer.distance_sq(screen_of(rect, pan, zoom, *position));
            (distance <= 16.0 * 16.0).then(|| (distance, *position, reference.clone()))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, position, reference)| (position, reference))
}

fn intersection_port_near(
    program: &Program,
    projection: &map::Projection,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<(WorldPoint, String, String)> {
    map::intersections(program, projection)
        .into_iter()
        .flat_map(|intersection| {
            intersection.ports.into_iter().map(move |port| {
                (
                    port.position,
                    port.point_ref,
                    intersection.road_name.clone(),
                    intersection.position,
                )
            })
        })
        .filter_map(|(position, reference, road_name, center)| {
            let distance = pointer.distance_sq(screen_of(rect, pan, zoom, position));
            let radius = (screen_of(rect, pan, zoom, position)
                .distance(screen_of(rect, pan, zoom, center))
                * 0.42)
                .clamp(3.0, 12.0);
            (distance <= radius * radius).then_some((distance, position, reference, road_name))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, position, reference, road_name)| (position, reference, road_name))
}

fn placed_intersection_at(
    program: &Program,
    projection: &map::Projection,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<String> {
    if intersection_port_near(program, projection, rect, pan, zoom, pointer).is_some() {
        return None;
    }
    map::intersections(program, projection)
        .into_iter()
        .rev()
        .find_map(|intersection| {
            (!intersection.ports.is_empty()
                && intersection_paint(program, projection, &intersection, rect, pan, zoom)
                    .is_some_and(|paint| paint.square.contains(pointer)))
            .then_some(intersection.point_ref)
        })
}

#[derive(Clone, Copy)]
struct ElementEndCandidate {
    position: WorldPoint,
    source: SegmentSource,
    element_index: usize,
    t: f32,
    kind: RoadElementKind,
}

fn element_end_candidates(
    projection: &map::Projection,
    program: &Program,
) -> Vec<ElementEndCandidate> {
    let mut candidates = Vec::new();
    for segment in projection
        .segments
        .iter()
        .filter(|segment| segment.source.editable())
    {
        let Some(road) = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == segment.road_name => Some(road),
                _ => None,
            })
        else {
            continue;
        };
        for (element_index, element) in road.elements.iter().enumerate() {
            for t in [0.0, 1.0] {
                if let Some(position) = map::element_position(segment, road, element_index, t) {
                    candidates.push(ElementEndCandidate {
                        position,
                        source: segment.source,
                        element_index,
                        t,
                        kind: element.kind,
                    });
                }
            }
        }
    }
    candidates
}

fn element_end_near(
    candidates: &[ElementEndCandidate],
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<ElementEndCandidate> {
    candidates
        .iter()
        .copied()
        .filter_map(|candidate| {
            let distance = pointer.distance_sq(screen_of(rect, pan, zoom, candidate.position));
            (distance <= 12.0 * 12.0).then_some((distance, candidate))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, candidate)| candidate)
}

fn segment_near(
    segments: &[map::Segment],
    program: &Program,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<SegmentSource> {
    segments
        .iter()
        .rev()
        .find(|segment| {
            let half_width = program
                .statements
                .iter()
                .find_map(|statement| match statement {
                    Statement::Road(road) if road.name == segment.road_name => Some(
                        road.elements
                            .iter()
                            .map(|element| map_element_width(element.kind, zoom))
                            .sum::<f32>()
                            / 2.0,
                    ),
                    _ => None,
                })
                .unwrap_or(0.0);
            let radius = (half_width + 3.0).max(16.0);
            (0..24).any(|step| {
                let a = screen_of(rect, pan, zoom, segment.point_at(step as f32 / 24.0));
                let b = screen_of(rect, pan, zoom, segment.point_at((step + 1) as f32 / 24.0));
                let line = b - a;
                let length_squared = line.length_sq();
                if length_squared < 1.0 {
                    return pointer.distance_sq(a) < radius * radius;
                }
                let along = ((pointer - a).dot(line) / length_squared).clamp(0.0, 1.0);
                pointer.distance_sq(a + line * along) < radius * radius
            })
        })
        .map(|segment| segment.source)
}

fn control_near(
    segments: &[map::Segment],
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<(SegmentSource, bool)> {
    segments
        .iter()
        .rev()
        .filter(|segment| segment.source.editable())
        .find_map(|segment| {
            let [first, second] = segment.controls;
            if pointer.distance_sq(screen_of(rect, pan, zoom, first)) <= 12.0 * 12.0 {
                Some((segment.source, false))
            } else if pointer.distance_sq(screen_of(rect, pan, zoom, second)) <= 12.0 * 12.0 {
                Some((segment.source, true))
            } else {
                None
            }
        })
}

fn map_element_width(kind: RoadElementKind, zoom: f32) -> f32 {
    map::element_world_width(kind) * zoom
}

#[derive(Default)]
struct PaintProfile {
    start_alignment: Option<usize>,
    merge_alignment: Option<usize>,
    start_taper: Vec<usize>,
    end_taper: Vec<usize>,
}

#[derive(Clone)]
struct DesignIssue {
    code: &'static str,
    message: String,
    at: WorldPoint,
    roads: [String; 2],
}

fn road_joins(projection: &map::Projection) -> Vec<(usize, usize)> {
    let mut ends: HashMap<&str, Vec<(usize, bool)>> = HashMap::new();
    for (index, segment) in projection.segments.iter().enumerate() {
        if let Some(reference) = segment.start_ref.as_deref() {
            ends.entry(reference).or_default().push((index, false));
        }
        if let Some(reference) = segment.end_ref.as_deref() {
            ends.entry(reference).or_default().push((index, true));
        }
    }
    ends.into_values()
        .filter_map(|ends| {
            if ends.len() != 2 {
                return None;
            }
            match (ends[0], ends[1]) {
                ((a, true), (b, false)) => Some((a, b)),
                ((a, false), (b, true)) => Some((b, a)),
                _ => None,
            }
        })
        .collect()
}

fn straight_join(incoming: &map::Segment, outgoing: &map::Segment) -> bool {
    let a = incoming.tangent_at(1.0);
    let b = outgoing.tangent_at(0.0);
    let lengths = (a.x * a.x + a.y * a.y).sqrt() * (b.x * b.x + b.y * b.y).sqrt();
    lengths > 0.000001 && (a.x * b.x + a.y * b.y) / lengths > 0.95
}

/// Indices of the wider road's elements outside an exact, contiguous match.
fn extra_elements_at_join(wide: &RoadDefinition, narrow: &RoadDefinition) -> Option<Vec<usize>> {
    if wide.elements.len() <= narrow.elements.len() {
        return None;
    }
    let start = wide
        .elements
        .windows(narrow.elements.len())
        .position(|window| window == narrow.elements.as_slice())?;
    Some(
        (0..wide.elements.len())
            .filter(|index| *index < start || *index >= start + narrow.elements.len())
            .collect(),
    )
}

fn road_for_segment<'a>(
    program: &'a Program,
    segment: &map::Segment,
) -> Option<&'a RoadDefinition> {
    program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Road(road) if road.name == segment.road_name => Some(road),
            _ => None,
        })
}

fn ramp_element_at_attachment(
    parent_road: &RoadDefinition,
    ramp_road: &RoadDefinition,
    element_index: usize,
) -> Option<usize> {
    if element_index >= parent_road.elements.len() || ramp_road.elements.is_empty() {
        return None;
    }
    let outward = &parent_road.elements[element_index..];
    let inward = &parent_road.elements[..=element_index];
    let elements = &ramp_road.elements;
    if elements == outward || elements.iter().rev().eq(inward.iter()) {
        Some(0)
    } else if elements == inward || elements.iter().rev().eq(outward.iter()) {
        Some(elements.len() - 1)
    } else {
        None
    }
}

fn paint_profiles(program: &Program, projection: &map::Projection) -> Vec<PaintProfile> {
    let mut profiles: Vec<_> = projection
        .segments
        .iter()
        .map(|_| PaintProfile::default())
        .collect();
    for split in map::lane_splits(program) {
        let Some(parent) = projection
            .segments
            .iter()
            .find(|segment| segment.name.as_deref() == Some(&split.parent))
        else {
            continue;
        };
        let Some((ramp_index, ramp)) = projection
            .segments
            .iter()
            .enumerate()
            .find(|(_, segment)| segment.name.as_deref() == Some(&split.ramp))
        else {
            continue;
        };
        let Some(parent_road) = road_for_segment(program, parent) else {
            continue;
        };
        let Some(ramp_road) = road_for_segment(program, ramp) else {
            continue;
        };
        profiles[ramp_index].start_alignment =
            ramp_element_at_attachment(parent_road, ramp_road, split.element_index);
    }
    for (index, segment) in projection.segments.iter().enumerate() {
        let Some((parent_name, element_index, _)) = segment
            .end_ref
            .as_deref()
            .and_then(|reference| map::lane_attachment(program, reference))
        else {
            continue;
        };
        let Some(parent) = projection
            .segments
            .iter()
            .find(|candidate| candidate.name.as_deref() == Some(&parent_name))
        else {
            continue;
        };
        let (Some(parent_road), Some(ramp_road)) = (
            road_for_segment(program, parent),
            road_for_segment(program, segment),
        ) else {
            continue;
        };
        profiles[index].merge_alignment =
            ramp_element_at_attachment(parent_road, ramp_road, element_index);
    }
    for (incoming, outgoing) in road_joins(projection) {
        let a = &projection.segments[incoming];
        let b = &projection.segments[outgoing];
        if !straight_join(a, b) {
            continue;
        }
        let (Some(a_road), Some(b_road)) =
            (road_for_segment(program, a), road_for_segment(program, b))
        else {
            continue;
        };
        if let Some(extra) = extra_elements_at_join(a_road, b_road) {
            profiles[incoming].end_taper = extra;
        } else if let Some(extra) = extra_elements_at_join(b_road, a_road) {
            profiles[outgoing].start_taper = extra;
        }
    }
    profiles
}

fn screen_normal_at(segment: &map::Segment, t: f32) -> egui::Vec2 {
    let tangent = segment.tangent_at(t);
    let dx = tangent.x;
    let dy = -tangent.y;
    let length = (dx * dx + dy * dy).sqrt();
    if length > 0.0001 {
        egui::vec2(-dy / length, dx / length)
    } else {
        egui::vec2(0.0, 1.0)
    }
}

fn section_widths(
    road: &RoadDefinition,
    zoom: f32,
    profile: &PaintProfile,
    t: f32,
) -> (Vec<f32>, f32) {
    let mut widths: Vec<_> = road
        .elements
        .iter()
        .map(|element| map_element_width(element.kind, zoom))
        .collect();
    let smooth = |value: f32| {
        let x = value.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    for index in &profile.start_taper {
        widths[*index] *= smooth(t / 0.25);
    }
    for index in &profile.end_taper {
        widths[*index] *= smooth((1.0 - t) / 0.25);
    }
    let total: f32 = widths.iter().sum();
    let start_offset = profile
        .start_alignment
        .and_then(|index| {
            widths
                .get(index)
                .map(|selected| total / 2.0 - widths[..index].iter().sum::<f32>() - selected / 2.0)
        })
        .unwrap_or(0.0);
    let end_offset = profile
        .merge_alignment
        .and_then(|index| {
            widths
                .get(index)
                .map(|selected| total / 2.0 - widths[..index].iter().sum::<f32>() - selected / 2.0)
        })
        .unwrap_or(0.0);
    (
        widths,
        -total / 2.0 + start_offset * (1.0 - t).powi(2) + end_offset * t.powi(2),
    )
}

fn section_edges(road: &RoadDefinition, profile: &PaintProfile, t: f32) -> (f32, f32) {
    let (widths, left) = section_widths(road, 1.0, profile, t);
    (left, left + widths.iter().sum::<f32>())
}

fn design_rule_issues(
    program: &Program,
    projection: &map::Projection,
    profiles: &[PaintProfile],
) -> Vec<DesignIssue> {
    let mut issues = Vec::new();
    for (incoming, outgoing) in road_joins(projection) {
        let a = &projection.segments[incoming];
        let b = &projection.segments[outgoing];
        if !straight_join(a, b) {
            continue;
        }
        let (Some(a_road), Some(b_road)) =
            (road_for_segment(program, a), road_for_segment(program, b))
        else {
            continue;
        };
        let (a_left, a_right) = section_edges(a_road, &profiles[incoming], 1.0);
        let (b_left, b_right) = section_edges(b_road, &profiles[outgoing], 0.0);
        if (a_left - b_left).abs() > 0.005 || (a_right - b_right).abs() > 0.005 {
            issues.push(DesignIssue {
                code: "road_edge_mismatch",
                message: format!(
                    "Road edges do not meet: {} → {}",
                    a.name.as_deref().unwrap_or(&a.road_name),
                    b.name.as_deref().unwrap_or(&b.road_name)
                ),
                at: a.end,
                roads: [
                    a.name.clone().unwrap_or_else(|| a.road_name.clone()),
                    b.name.clone().unwrap_or_else(|| b.road_name.clone()),
                ],
            });
        }
    }
    for intersection in map::intersections(program, projection) {
        let Some(edge) = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == intersection.road_name => Some(road),
                _ => None,
            })
        else {
            continue;
        };
        let attached: Vec<_> = if intersection.ports.is_empty() {
            map::intersection_arms(projection, &intersection.point_ref)
                .into_iter()
                .map(|(index, _)| index)
                .collect()
        } else {
            projection
                .segments
                .iter()
                .enumerate()
                .filter_map(|(index, segment)| {
                    intersection
                        .ports
                        .iter()
                        .any(|port| {
                            segment.start_ref.as_deref() == Some(&port.point_ref)
                                || segment.end_ref.as_deref() == Some(&port.point_ref)
                        })
                        .then_some(index)
                })
                .collect()
        };
        for index in attached {
            let segment = &projection.segments[index];
            let Some(approach) = road_for_segment(program, segment) else {
                continue;
            };
            if !approach
                .elements
                .iter()
                .map(|element| element.kind)
                .eq(edge.elements.iter().map(|element| element.kind))
            {
                let name = segment.name.as_deref().unwrap_or(&segment.road_name);
                issues.push(DesignIssue {
                    code: "intersection_edge_mismatch",
                    message: format!(
                        "{name} must match {} before {}",
                        edge.name, intersection.point_ref
                    ),
                    at: intersection.position,
                    roads: [name.to_owned(), edge.name.clone()],
                });
            }
        }
    }
    issues
}

fn paint_profiled_road(
    painter: &egui::Painter,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    segment: &map::Segment,
    road: &RoadDefinition,
    selected: bool,
    palette: EditorPalette,
    profile: &PaintProfile,
) {
    let samples = 64;
    let mut boundaries = Vec::with_capacity(samples + 1);
    for step in 0..=samples {
        let t = step as f32 / samples as f32;
        let center = screen_of(rect, pan, zoom, segment.point_at(t));
        let normal = screen_normal_at(segment, t);
        let (widths, mut offset) = section_widths(road, zoom, profile, t);
        let mut row = Vec::with_capacity(widths.len() + 1);
        row.push(center + normal * offset);
        for width in widths {
            offset += width;
            row.push(center + normal * offset);
        }
        boundaries.push(row);
    }
    for (element_index, element) in road.elements.iter().enumerate() {
        let color = kind_color(element.kind, palette.theme);
        for pair in boundaries.windows(2) {
            let polygon = [
                pair[0][element_index],
                pair[1][element_index],
                pair[1][element_index + 1],
                pair[0][element_index + 1],
            ];
            painter.add(egui::Shape::convex_polygon(
                polygon.to_vec(),
                color,
                egui::Stroke::NONE,
            ));
        }
    }
    let outline = if selected {
        palette.map_selected
    } else {
        palette.map_outline
    };
    for edge in [0, road.elements.len()] {
        let path: Vec<_> = boundaries.iter().map(|row| row[edge]).collect();
        painter.add(egui::Shape::line(
            path,
            egui::Stroke::new(if selected { 2.5 } else { 1.5 }, outline),
        ));
    }
    let spine: Vec<_> = (0..=samples)
        .map(|step| {
            screen_of(
                rect,
                pan,
                zoom,
                segment.point_at(step as f32 / samples as f32),
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        spine,
        egui::Stroke::new((zoom * 0.004).clamp(0.8, 12.0), palette.map_centerline),
    ));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PortSide {
    East,
    South,
    West,
    North,
}

impl PortSide {
    const ALL: [Self; 4] = [Self::East, Self::South, Self::West, Self::North];

    fn from_world_direction(direction: WorldPoint) -> Option<Self> {
        match (direction.x as i32, direction.y as i32) {
            (1, 0) => Some(Self::East),
            (0, -1) => Some(Self::South),
            (-1, 0) => Some(Self::West),
            (0, 1) => Some(Self::North),
            _ => None,
        }
    }

    fn direction(self) -> egui::Vec2 {
        match self {
            Self::East => egui::vec2(1.0, 0.0),
            Self::South => egui::vec2(0.0, 1.0),
            Self::West => egui::vec2(-1.0, 0.0),
            Self::North => egui::vec2(0.0, -1.0),
        }
    }

    fn edge_point(self, center: egui::Pos2, half: f32, lateral: f32) -> egui::Pos2 {
        match self {
            Self::East => center + egui::vec2(half, lateral),
            Self::South => center + egui::vec2(lateral, half),
            Self::West => center + egui::vec2(-half, lateral),
            Self::North => center + egui::vec2(lateral, -half),
        }
    }

    fn lateral(self) -> egui::Vec2 {
        match self {
            Self::East | Self::West => egui::vec2(0.0, 1.0),
            Self::South | Self::North => egui::vec2(1.0, 0.0),
        }
    }

    fn wedge_planes(self) -> [(f32, f32); 2] {
        match self {
            Self::East => [(1.0, -1.0), (1.0, 1.0)],
            Self::South => [(-1.0, 1.0), (1.0, 1.0)],
            Self::West => [(-1.0, -1.0), (-1.0, 1.0)],
            Self::North => [(-1.0, -1.0), (1.0, -1.0)],
        }
    }
}

fn clipped_port_band(band: egui::Rect, side: PortSide, center: egui::Pos2) -> Vec<egui::Pos2> {
    let mut polygon = vec![
        band.left_top(),
        band.right_top(),
        band.right_bottom(),
        band.left_bottom(),
    ];
    for (x, y) in side.wedge_planes() {
        let signed = |point: egui::Pos2| x * (point.x - center.x) + y * (point.y - center.y);
        let mut clipped = Vec::new();
        if let Some(mut previous) = polygon.last().copied() {
            for current in polygon {
                let before = signed(previous);
                let after = signed(current);
                if (before < 0.0 && after >= 0.0) || (before >= 0.0 && after < 0.0) {
                    let fraction = before / (before - after);
                    clipped.push(previous + (current - previous) * fraction);
                }
                if after >= 0.0 {
                    clipped.push(current);
                }
                previous = current;
            }
        }
        polygon = clipped;
    }
    polygon
}

fn paint_port_band(
    painter: &egui::Painter,
    band: egui::Rect,
    side: PortSide,
    center: egui::Pos2,
    color: Color32,
) {
    let polygon = clipped_port_band(band, side, center);
    if polygon.len() >= 3 {
        painter.add(egui::Shape::convex_polygon(
            polygon,
            color,
            egui::Stroke::NONE,
        ));
    }
}

struct IntersectionArm<'a> {
    segment: &'a map::Segment,
    is_end: bool,
    direction: egui::Vec2,
}

struct IntersectionPort {
    side: PortSide,
    bands: Vec<(RoadElementKind, egui::Rect)>,
}

struct IntersectionPaint {
    square: egui::Rect,
    closed: RoadElementKind,
    ports: Vec<IntersectionPort>,
}

fn port_sides(arms: &[IntersectionArm<'_>]) -> Vec<PortSide> {
    fn assign(
        arms: &[IntersectionArm<'_>],
        current: &mut Vec<PortSide>,
        best: &mut (f32, Vec<PortSide>),
    ) {
        if current.len() == arms.len() {
            let score = current
                .iter()
                .zip(arms)
                .map(|(side, arm)| side.direction().dot(arm.direction))
                .sum();
            if score > best.0 {
                *best = (score, current.clone());
            }
            return;
        }
        for side in PortSide::ALL {
            if !current.contains(&side) {
                current.push(side);
                assign(arms, current, best);
                current.pop();
            }
        }
    }
    let mut best = (f32::NEG_INFINITY, Vec::new());
    assign(arms, &mut Vec::new(), &mut best);
    best.1
}

fn intersection_port(
    arm: &IntersectionArm<'_>,
    side: PortSide,
    junction: egui::Pos2,
    half: f32,
    road: &RoadDefinition,
    zoom: f32,
) -> IntersectionPort {
    let normal = screen_normal_at(arm.segment, if arm.is_end { 1.0 } else { 0.0 });
    let sign = if normal.dot(side.lateral()) >= 0.0 {
        1.0
    } else {
        -1.0
    };
    intersection_port_with_sign(side, junction, half, road, zoom, sign)
}

fn intersection_port_with_sign(
    side: PortSide,
    junction: egui::Pos2,
    half: f32,
    road: &RoadDefinition,
    zoom: f32,
    sign: f32,
) -> IntersectionPort {
    let depth = half * 0.44;
    let mut offset = -half;
    let bands = road
        .elements
        .iter()
        .map(|element| {
            let next = offset + map::element_world_width(element.kind) * zoom;
            let a = side.edge_point(junction, half, offset * sign);
            let b = side.edge_point(junction, half, next * sign);
            let inside = -side.direction() * depth;
            offset = next;
            (element.kind, egui::Rect::from_two_pos(a, b + inside))
        })
        .collect();
    IntersectionPort { side, bands }
}

fn intersection_paint(
    program: &Program,
    projection: &map::Projection,
    intersection: &map::Intersection,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
) -> Option<IntersectionPaint> {
    let junction = screen_of(rect, pan, zoom, intersection.position);
    let road = program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Road(road) if road.name == intersection.road_name => Some(road),
            _ => None,
        })?;
    let mut arms = Vec::new();
    for (index, is_end) in map::intersection_arms(projection, &intersection.point_ref) {
        let segment = &projection.segments[index];
        let tangent = segment.tangent_at(if is_end { 1.0 } else { 0.0 });
        let direction = egui::vec2(
            if is_end { -tangent.x } else { tangent.x },
            if is_end { tangent.y } else { -tangent.y },
        );
        arms.push(IntersectionArm {
            segment,
            is_end,
            direction,
        });
    }
    let half = road
        .elements
        .iter()
        .map(|element| map::element_world_width(element.kind))
        .sum::<f32>()
        * zoom
        * 0.5;
    let closed = if road
        .elements
        .iter()
        .any(|element| element.kind == RoadElementKind::Sidewalk)
    {
        RoadElementKind::Sidewalk
    } else if road
        .elements
        .iter()
        .any(|element| element.kind == RoadElementKind::Verge)
    {
        RoadElementKind::Verge
    } else {
        RoadElementKind::CarLane
    };
    let ports = if intersection.ports.is_empty() {
        port_sides(&arms)
            .into_iter()
            .zip(&arms)
            .map(|(side, arm)| intersection_port(arm, side, junction, half, road, zoom))
            .collect()
    } else {
        intersection
            .ports
            .iter()
            .filter_map(|port| {
                let side = PortSide::from_world_direction(port.direction)?;
                Some(intersection_port_with_sign(
                    side, junction, half, road, zoom, 1.0,
                ))
            })
            .collect()
    };
    Some(IntersectionPaint {
        square: egui::Rect::from_center_size(junction, egui::vec2(half * 2.0, half * 2.0)),
        closed,
        ports,
    })
}

fn paint_intersection(
    painter: &egui::Painter,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    program: &Program,
    projection: &map::Projection,
    intersection: &map::Intersection,
    palette: EditorPalette,
) {
    let Some(paint) = intersection_paint(program, projection, intersection, rect, pan, zoom) else {
        return;
    };
    painter.rect_filled(
        paint.square,
        0.0,
        kind_color(RoadElementKind::CarLane, palette.theme),
    );
    let center = paint.square.center();
    let half = paint.square.width() * 0.5;
    if paint.ports.len() == 4 {
        // Each corner follows the port's outer bands. The carriageway stays
        // clear across the junction instead of forming four pointed wedges.
        if let Some(north) = paint.ports.iter().find(|port| port.side == PortSide::North) {
            for right in [false, true] {
                let corner = if right {
                    paint.square.right_top()
                } else {
                    paint.square.left_top()
                };
                paint_rounded_corner(
                    painter,
                    north,
                    corner,
                    center.x,
                    right,
                    false,
                    palette.theme,
                );
            }
        }
        if let Some(south) = paint.ports.iter().find(|port| port.side == PortSide::South) {
            for right in [false, true] {
                let corner = if right {
                    paint.square.right_bottom()
                } else {
                    paint.square.left_bottom()
                };
                paint_rounded_corner(painter, south, corner, center.x, right, true, palette.theme);
            }
        }
        return;
    }
    for side in PortSide::ALL {
        if !paint.ports.iter().any(|port| port.side == side) {
            let a = side.edge_point(center, half, -half);
            let b = side.edge_point(center, half, half);
            paint_port_band(
                painter,
                egui::Rect::from_two_pos(a, b - side.direction() * half * 0.44),
                side,
                center,
                kind_color(paint.closed, palette.theme),
            );
        }
    }
    for port in &paint.ports {
        for (kind, band) in &port.bands {
            paint_port_band(
                painter,
                *band,
                port.side,
                center,
                kind_color(*kind, palette.theme),
            );
        }
    }
    for side in PortSide::ALL {
        if !paint.ports.iter().any(|port| port.side == side) {
            painter.line_segment(
                [
                    side.edge_point(center, half, -half),
                    side.edge_point(center, half, half),
                ],
                egui::Stroke::new(1.5, palette.map_outline),
            );
        }
    }
}

fn paint_rounded_corner(
    painter: &egui::Painter,
    port: &IntersectionPort,
    corner: egui::Pos2,
    middle_x: f32,
    right: bool,
    bottom: bool,
    theme: egui::Theme,
) {
    for (kind, radius) in corner_disks(port, corner.x, middle_x, right)
        .into_iter()
        .rev()
    {
        let mut points = Vec::with_capacity(18);
        points.push(corner);
        for step in 0..=16 {
            let angle = std::f32::consts::FRAC_PI_2 * step as f32 / 16.0;
            points.push(
                corner
                    + egui::vec2(
                        if right { -1.0 } else { 1.0 } * radius * angle.cos(),
                        if bottom { -1.0 } else { 1.0 } * radius * angle.sin(),
                    ),
            );
        }
        painter.add(egui::Shape::convex_polygon(
            points,
            kind_color(kind, theme),
            egui::Stroke::NONE,
        ));
    }
}

fn corner_disks(
    port: &IntersectionPort,
    corner_x: f32,
    middle_x: f32,
    right: bool,
) -> Vec<(RoadElementKind, f32)> {
    let mut bands: Vec<_> = port
        .bands
        .iter()
        .filter_map(|(kind, band)| {
            if (band.center().x >= middle_x) != right {
                return None;
            }
            let distance = (corner_x - band.center().x).abs();
            Some((distance, *kind, band.width()))
        })
        .collect();
    bands.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut radius = 0.0;
    bands
        .into_iter()
        .take_while(|(_, kind, _)| {
            !matches!(
                kind,
                RoadElementKind::CarLane | RoadElementKind::BusLane | RoadElementKind::Middle
            )
        })
        .map(|(_, kind, width)| {
            radius += width;
            (kind, radius)
        })
        .collect()
}

fn paint_road_segment(
    painter: &egui::Painter,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    segment: &map::Segment,
    road: Option<&RoadDefinition>,
    selected: bool,
    palette: EditorPalette,
) {
    let samples = 32;
    let spine: Vec<_> = (0..=samples)
        .map(|step| {
            screen_of(
                rect,
                pan,
                zoom,
                segment.point_at(step as f32 / samples as f32),
            )
        })
        .collect();
    let normals: Vec<_> = (0..spine.len())
        .map(|index| screen_normal_at(segment, index as f32 / samples as f32))
        .collect();
    let kinds: Vec<_> = road
        .map(|road| road.elements.iter().map(|element| element.kind).collect())
        .unwrap_or_else(|| vec![RoadElementKind::CarLane]);
    let total: f32 = kinds
        .iter()
        .map(|kind| map_element_width(*kind, zoom))
        .sum();
    painter.add(egui::Shape::line(
        spine.clone(),
        egui::Stroke::new(
            total + if selected { 6.0 } else { 3.0 },
            if selected {
                palette.map_selected
            } else {
                palette.map_outline
            },
        ),
    ));
    let mut offset = -total / 2.0;
    for kind in kinds {
        let width = map_element_width(kind, zoom);
        let center = offset + width / 2.0;
        let path: Vec<_> = spine
            .iter()
            .zip(&normals)
            .map(|(point, normal)| *point + *normal * center)
            .collect();
        painter.add(egui::Shape::line(
            path,
            egui::Stroke::new(width, kind_color(kind, palette.theme)),
        ));
        offset += width;
    }
    painter.add(egui::Shape::line(
        spine,
        egui::Stroke::new((zoom * 0.004).clamp(0.8, 12.0), palette.map_centerline),
    ));
}

fn endpoint_near(
    segments: &[map::Segment],
    program: &Program,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<(SegmentSource, bool)> {
    segments
        .iter()
        .rev()
        .filter(|segment| segment.source.editable())
        .find_map(|segment| {
            let start = screen_of(rect, pan, zoom, segment.start);
            let end = screen_of(rect, pan, zoom, segment.end);
            if pointer.distance_sq(start) <= 14.0 * 14.0
                && !segment
                    .start_ref
                    .as_deref()
                    .is_some_and(|name| map::anchored_reference(program, name))
            {
                Some((segment.source, false))
            } else if pointer.distance_sq(end) <= 14.0 * 14.0
                && !segment
                    .end_ref
                    .as_deref()
                    .is_some_and(|name| map::anchored_reference(program, name))
            {
                Some((segment.source, true))
            } else {
                None
            }
        })
}

fn paint_grid(
    painter: &egui::Painter,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    palette: EditorPalette,
) {
    let left = world_of(rect, pan, zoom, rect.left_top()).x.floor() as i32;
    let right = world_of(rect, pan, zoom, rect.right_top()).x.ceil() as i32;
    let bottom = world_of(rect, pan, zoom, rect.left_bottom()).y.floor() as i32;
    let top = world_of(rect, pan, zoom, rect.left_top()).y.ceil() as i32;
    for x in left..=right {
        let screen = screen_of(
            rect,
            pan,
            zoom,
            WorldPoint {
                x: x as f32,
                y: 0.0,
            },
        )
        .x;
        let color = if x == 0 {
            palette.map_axis
        } else if x % 5 == 0 {
            palette.map_major
        } else {
            palette.map_minor
        };
        painter.line_segment(
            [
                egui::pos2(screen, rect.top()),
                egui::pos2(screen, rect.bottom()),
            ],
            egui::Stroke::new(1.0, color),
        );
    }
    for y in bottom..=top {
        let screen = screen_of(
            rect,
            pan,
            zoom,
            WorldPoint {
                x: 0.0,
                y: y as f32,
            },
        )
        .y;
        let color = if y == 0 {
            palette.map_axis
        } else if y % 5 == 0 {
            palette.map_major
        } else {
            palette.map_minor
        };
        painter.line_segment(
            [
                egui::pos2(rect.left(), screen),
                egui::pos2(rect.right(), screen),
            ],
            egui::Stroke::new(1.0, color),
        );
    }
}

impl eframe::App for RoadEditor {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        if self.mcp.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        self.handle_mcp(&ctx);
        let before = self.program.clone();
        // Native menu events arrive outside egui's input stream.
        #[cfg(target_os = "macos")]
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        #[cfg(target_os = "macos")]
        if let Some(menu) = &self.native_menu {
            let actions = menu.drain();
            for action in actions {
                self.file_action(action);
            }
        }
        let shortcut = ctx.input_mut(|input| {
            let command = egui::Modifiers::COMMAND;
            if input.consume_key(command | egui::Modifiers::SHIFT, egui::Key::S) {
                Some(FileAction::SaveAs)
            } else if input.consume_key(command, egui::Key::S) {
                Some(FileAction::Save)
            } else if input.consume_key(command, egui::Key::O) {
                Some(FileAction::Open)
            } else if input.consume_key(command, egui::Key::N) {
                Some(FileAction::New)
            } else {
                None
            }
        });
        if let Some(action) = shortcut {
            self.file_action(action);
        }
        let history_shortcut = ctx.input_mut(|input| {
            let command = egui::Modifiers::COMMAND;
            if input.consume_key(command | egui::Modifiers::SHIFT, egui::Key::Z)
                || (cfg!(not(target_os = "macos")) && input.consume_key(command, egui::Key::Y))
            {
                Some(false)
            } else if input.consume_key(command, egui::Key::Z) {
                Some(true)
            } else {
                None
            }
        });
        if let Some(undo) = history_shortcut {
            self.restore_history(undo);
        }
        self.top_bar(root);
        self.sidebar(root);
        self.inspector(root);
        egui::Panel::bottom("status").show(root, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&self.status)
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            });
        });
        match self.view {
            View::Map => self.map_canvas(root),
            View::Section => self.section_canvas(root),
        }
        self.pending_dialog(&ctx);
        self.delete_dialog(&ctx);
        let dragging = ctx.input(|input| input.pointer.primary_down())
            && (self.dragging_endpoint.is_some() || self.dragging_control.is_some());
        self.record_history(before, dragging);
    }
}

fn icon(name: &str, tint: Color32) -> egui::Image<'static> {
    let bytes: &'static [u8] = match name {
        "plus" => include_bytes!("../../assets/lucide/plus.svg"),
        "minus" => include_bytes!("../../assets/lucide/minus.svg"),
        "box" => include_bytes!("../../assets/lucide/box.svg"),
        _ => include_bytes!("../../assets/lucide/x.svg"),
    };
    egui::Image::from_bytes(format!("bytes://lucide/{name}.svg"), bytes)
        .fit_to_exact_size(egui::vec2(16.0, 16.0))
        .tint(tint)
}

fn kind_label(kind: RoadElementKind) -> &'static str {
    match kind {
        RoadElementKind::BuildingEdge => "Building edge",
        RoadElementKind::Sidewalk => "Sidewalk",
        RoadElementKind::BikeLane => "Bike lane",
        RoadElementKind::ParkingLane => "Parking lane",
        RoadElementKind::BusLane => "Bus lane",
        RoadElementKind::CarLane => "Car lane",
        RoadElementKind::BusStop => "Bus stop",
        RoadElementKind::Middle => "Median",
        RoadElementKind::Verge => "Verge",
        RoadElementKind::Barrier => "Divider wall",
    }
}

fn kind_short(kind: RoadElementKind) -> &'static str {
    match kind {
        RoadElementKind::BuildingEdge => "EDGE",
        RoadElementKind::Sidewalk => "WALK",
        RoadElementKind::BikeLane => "BIKE",
        RoadElementKind::ParkingLane => "PARK",
        RoadElementKind::BusLane => "BUS",
        RoadElementKind::CarLane => "CAR",
        RoadElementKind::BusStop => "STOP",
        RoadElementKind::Middle => "MID",
        RoadElementKind::Verge => "VERGE",
        RoadElementKind::Barrier => "WALL",
    }
}

fn kind_color(kind: RoadElementKind, theme: egui::Theme) -> Color32 {
    match theme {
        egui::Theme::Dark => match kind {
            RoadElementKind::BuildingEdge => Color32::from_rgb(9, 25, 32),
            RoadElementKind::Sidewalk => Color32::from_rgb(163, 119, 86),
            RoadElementKind::BikeLane => Color32::from_rgb(58, 131, 105),
            RoadElementKind::ParkingLane => Color32::from_rgb(87, 122, 149),
            RoadElementKind::BusLane => Color32::from_rgb(164, 76, 71),
            RoadElementKind::CarLane => Color32::from_rgb(59, 95, 127),
            RoadElementKind::BusStop => Color32::from_rgb(162, 121, 53),
            RoadElementKind::Middle => Color32::from_rgb(59, 95, 127),
            RoadElementKind::Verge => Color32::from_rgb(71, 115, 77),
            RoadElementKind::Barrier => Color32::from_rgb(165, 170, 162),
        },
        egui::Theme::Light => match kind {
            RoadElementKind::BuildingEdge => Color32::from_rgb(190, 198, 205),
            RoadElementKind::Sidewalk => Color32::from_rgb(220, 197, 164),
            RoadElementKind::BikeLane => Color32::from_rgb(133, 198, 166),
            RoadElementKind::ParkingLane => Color32::from_rgb(169, 184, 205),
            RoadElementKind::BusLane => Color32::from_rgb(225, 154, 146),
            RoadElementKind::CarLane => Color32::from_rgb(151, 176, 205),
            RoadElementKind::BusStop => Color32::from_rgb(230, 188, 110),
            RoadElementKind::Middle => Color32::from_rgb(204, 191, 148),
            RoadElementKind::Verge => Color32::from_rgb(158, 200, 160),
            RoadElementKind::Barrier => Color32::from_rgb(107, 115, 112),
        },
    }
}

fn element_width(kind: RoadElementKind) -> f32 {
    match kind {
        RoadElementKind::BuildingEdge => 72.0,
        RoadElementKind::Sidewalk => 112.0,
        RoadElementKind::BikeLane => 94.0,
        RoadElementKind::ParkingLane => 103.0,
        RoadElementKind::BusLane => 105.0,
        RoadElementKind::CarLane => 108.0,
        RoadElementKind::BusStop => 96.0,
        RoadElementKind::Middle => 78.0,
        RoadElementKind::Verge => 86.0,
        RoadElementKind::Barrier => 30.0,
    }
}

#[cfg(target_os = "macos")]
struct NativeFileMenu {
    _menu: muda::Menu,
    items: [(muda::MenuId, FileAction); 4],
}

#[cfg(target_os = "macos")]
impl NativeFileMenu {
    fn install() -> Option<Self> {
        use muda::{
            accelerator::{Accelerator, Code, Modifiers},
            Menu, MenuItem, PredefinedMenuItem, Submenu,
        };
        let menu = Menu::new();
        let app = Submenu::new("OpenCity", true);
        app.append_items(&[
            &PredefinedMenuItem::about(None, None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ])
        .ok()?;
        let file = Submenu::new("File", true);
        let new = MenuItem::new(
            "New",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyN)),
        );
        let open = MenuItem::new(
            "Open…",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyO)),
        );
        let save = MenuItem::new(
            "Save",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyS)),
        );
        let save_as = MenuItem::new(
            "Save As…",
            true,
            Some(Accelerator::new(
                Some(Modifiers::META | Modifiers::SHIFT),
                Code::KeyS,
            )),
        );
        file.append_items(&[
            &new,
            &open,
            &PredefinedMenuItem::separator(),
            &save,
            &save_as,
        ])
        .ok()?;
        menu.append_items(&[&app, &file]).ok()?;
        menu.init_for_nsapp();
        Some(Self {
            _menu: menu,
            items: [
                (new.id().clone(), FileAction::New),
                (open.id().clone(), FileAction::Open),
                (save.id().clone(), FileAction::Save),
                (save_as.id().clone(), FileAction::SaveAs),
            ],
        })
    }

    fn drain(&self) -> Vec<FileAction> {
        let mut actions = Vec::new();
        while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
            if let Some((_, action)) = self.items.iter().find(|(id, _)| *id == event.id) {
                actions.push(*action);
            }
        }
        actions
    }
}

fn main() -> eframe::Result {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_editor() -> RoadEditor {
        RoadEditor {
            program: parse(include_str!("../../examples/city.road")).unwrap(),
            saved_program: parse(include_str!("../../examples/city.road")).unwrap(),
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
            lane_end_choice: None,
            segment_name_draft: String::new(),
            road_name_draft: "normal_road".into(),
            new_kind: RoadElementKind::BikeLane,
            view: View::Map,
            map_tool: MapTool::Select,
            map_start: None,
            map_start_ref: None,
            branch_element: usize::MAX,
            branch_t: 0.75,
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

    #[test]
    fn history_groups_drag_and_tracks_saved_state() {
        let mut editor = sample_editor();
        let original = editor.program.clone();
        editor.add_road();
        editor.record_history(original.clone(), true);
        let first_edit = editor.program.clone();
        editor.add_road();
        editor.record_history(first_edit, true);
        assert_eq!(editor.undo_stack.len(), 1);
        editor.record_history(editor.program.clone(), false);
        editor.restore_history(true);
        assert_eq!(editor.program, original);
        assert!(!editor.dirty);
        editor.restore_history(false);
        assert!(editor.dirty);
    }

    #[test]
    fn mcp_edits_share_history_and_reject_invalid_move() {
        let mut editor = sample_editor();
        let original = editor.program.clone();
        let result = editor
            .call_mcp(
                "add_road",
                &serde_json::json!({
                    "road_type":"normal_road","start_x":0,"start_y":0,"end_x":3,"end_y":1
                }),
            )
            .unwrap();
        let index = result["segment_index"].as_u64().unwrap();
        editor.record_history(original.clone(), false);
        let before_move = editor.program.clone();
        assert!(editor
            .call_mcp(
                "move_road",
                &serde_json::json!({
                    "segment_index":index,"dx":1,"dy":2
                })
            )
            .is_ok());
        let moved = map::project(&editor.program)
            .segments
            .into_iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(index as usize))
            .unwrap();
        assert_eq!(moved.start, WorldPoint { x: 1.0, y: 2.0 });
        assert_eq!(moved.end, WorldPoint { x: 4.0, y: 3.0 });
        editor.record_history(before_move.clone(), false);
        assert_eq!(editor.undo_stack.len(), 2);
        editor.restore_history(true);
        assert_eq!(editor.program, before_move);
        assert!(editor
            .call_mcp(
                "move_road",
                &serde_json::json!({
                    "segment_index":index,"dx":"bad","dy":2
                })
            )
            .is_err());
        assert_eq!(editor.program, before_move);
        editor.restore_history(true);
        assert_eq!(editor.program, original);
    }

    #[test]
    fn mcp_sets_and_reports_an_intersection() {
        let mut editor = sample_editor();
        editor.program = parse(
            "road street: carlane; end; center = map_point(0, 0); \
            west = map_point(-1, 0); east = map_point(1, 0); north = map_point(0, 1); \
            a = street(west, -0.7, 0, -0.3, 0, center); \
            b = street(center, 0.3, 0, 0.7, 0, east); \
            c = street(center, 0, 0.3, 0, 0.7, north);",
        )
        .unwrap();
        assert!(editor
            .call_mcp(
                "set_intersection",
                &serde_json::json!({
                    "point_ref":"center", "kind":"four_way"
                })
            )
            .is_err());
        editor
            .call_mcp(
                "set_intersection",
                &serde_json::json!({
                    "point_ref":"center", "kind":"three_way", "road_type":"street"
                }),
            )
            .unwrap();
        let state = editor
            .call_mcp("get_road_state", &serde_json::json!({}))
            .unwrap();
        assert_eq!(state["intersections"][0]["point_ref"], "center");
        assert_eq!(state["intersections"][0]["kind"], "three_way");
        assert_eq!(state["intersections"][0]["road_type"], "street");
        editor
            .call_mcp(
                "set_intersection",
                &serde_json::json!({
                    "point_ref":"center", "kind":"none"
                }),
            )
            .unwrap();
        assert_eq!(
            editor
                .call_mcp("get_road_state", &serde_json::json!({}))
                .unwrap()["intersections"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn mcp_document_replacement_is_validated_and_undoable() {
        let mut editor = sample_editor();
        let original = editor.program.clone();
        let source = format!(
            "{}\nneighborhood_street(0, 0, 1, 1);\n",
            default_program().to_road_source()
        );
        let result = editor
            .call_mcp("replace_document", &serde_json::json!({"source": source}))
            .unwrap();
        assert_eq!(result["segments"], 1);
        editor.record_history(original.clone(), false);
        assert!(editor.dirty);
        let replacement = editor.program.clone();
        assert!(editor
            .call_mcp(
                "replace_document",
                &serde_json::json!({"source": "road broken:"})
            )
            .is_err());
        assert_eq!(editor.program, replacement);
        editor.restore_history(true);
        assert_eq!(editor.program, original);
    }

    #[test]
    fn mcp_add_road_reuses_a_named_junction() {
        let mut editor = sample_editor();
        editor.program =
            parse("road lane: sidewalk-carlane-sidewalk; end; a = map_point(0, 0);").unwrap();
        let result = editor
            .call_mcp(
                "add_road",
                &serde_json::json!({
                    "road_type":"lane", "name":"curved_lane", "start_ref":"a",
                    "start_x":0, "start_y":0, "end_x":2, "end_y":1,
                    "control1_x":0.5, "control1_y":0, "control2_x":1.5, "control2_y":1
                }),
            )
            .unwrap();
        let segment = map::project(&editor.program)
            .segments
            .into_iter()
            .find(|segment| {
                segment.source
                    == SegmentSource::TopLevel(result["segment_index"].as_u64().unwrap() as usize)
            })
            .unwrap();
        assert_eq!(segment.name.as_deref(), Some("curved_lane"));
        assert_eq!(segment.start_ref.as_deref(), Some("a"));
        assert_eq!(segment.controls[0], WorldPoint { x: 0.5, y: 0.0 });
        let before = editor.program.clone();
        assert!(editor
            .call_mcp(
                "add_road",
                &serde_json::json!({
                    "road_type":"lane", "start_ref":"a", "start_x":1, "start_y":0,
                    "end_x":3, "end_y":1
                })
            )
            .is_err());
        assert_eq!(editor.program, before);
    }

    #[test]
    fn names_follow_language_rules() {
        assert!(RoadEditor::valid_name("road_2"));
        assert!(!RoadEditor::valid_name("2_road"));
        assert!(!RoadEditor::valid_name("road"));
        assert!(!RoadEditor::valid_name("road-name"));
    }

    #[test]
    fn default_library_has_city_rural_and_interstate_profiles() {
        let program = default_program();
        let mut categories = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for statement in &program.statements {
            let Statement::Road(road) = statement else {
                panic!("default library must contain only roads")
            };
            assert!(!road.elements.is_empty());
            assert!(
                names.insert(&road.name),
                "duplicate preset name: {}",
                road.name
            );
            categories.insert(preset_category(&road.name));
        }
        assert!(program.statements.len() >= 12);
        assert_eq!(categories.len(), 3);
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    }

    #[test]
    fn all_cloverleaf_ramp_mouths_align_with_outer_highway_lane() {
        let program = parse(include_str!("../../examples/cloverleaf_town.road")).unwrap();
        let projection = map::project(&program);
        let profiles = paint_profiles(&program, &projection);
        let mut exits = 0;
        let mut merges = 0;
        for (index, segment) in projection.segments.iter().enumerate() {
            if segment.road_name != "ramp_mouth" {
                continue;
            }
            let road = road_for_segment(&program, segment).unwrap();
            for (reference, selected, t) in [
                (
                    segment.start_ref.as_deref(),
                    profiles[index].start_alignment,
                    0.0,
                ),
                (
                    segment.end_ref.as_deref(),
                    profiles[index].merge_alignment,
                    1.0,
                ),
            ] {
                if reference
                    .and_then(|name| map::lane_attachment(&program, name))
                    .is_none()
                {
                    continue;
                }
                assert_eq!(
                    selected,
                    Some(0),
                    "{} at t={t}",
                    segment.name.as_deref().unwrap()
                );
                let (_, left_edge) = section_widths(road, 100.0, &profiles[index], t);
                assert!(
                    (left_edge + map_element_width(RoadElementKind::CarLane, 100.0) / 2.0).abs()
                        < 0.0001,
                    "{} at t={t}: ramp lane misses the receiving lane",
                    segment.name.as_deref().unwrap()
                );
                if t == 0.0 {
                    exits += 1;
                } else {
                    merges += 1;
                }
            }
        }
        assert_eq!((exits, merges), (8, 9));
    }

    #[test]
    fn cloverleaf_road_joins_taper_without_edge_mismatches() {
        let program = parse(include_str!("../../examples/cloverleaf_town.road")).unwrap();
        let projection = map::project(&program);
        let profiles = paint_profiles(&program, &projection);
        let tapered = profiles
            .iter()
            .map(|profile| {
                usize::from(!profile.start_taper.is_empty())
                    + usize::from(!profile.end_taper.is_empty())
            })
            .sum::<usize>();
        assert!(
            tapered >= 17,
            "expected every cloverleaf ramp seam to taper"
        );
        let issues = design_rule_issues(&program, &projection, &profiles);
        assert!(
            issues.is_empty(),
            "{}",
            issues
                .iter()
                .map(|issue| issue.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }

    #[test]
    fn design_check_finds_a_straight_misaligned_join() {
        let program = parse("road narrow: carlane; end; road wide: verge-carlane-verge; end; p0 = map_point(0, 0); seam = map_point(1, 0); p1 = map_point(2, 0); a = narrow(p0, 0.3, 0, 0.7, 0, seam); b = wide(seam, 1.3, 0, 1.7, 0, p1);").unwrap();
        let projection = map::project(&program);
        let mut profiles = paint_profiles(&program, &projection);
        assert!(design_rule_issues(&program, &projection, &profiles).is_empty());
        profiles[1].start_taper.clear();
        let issues = design_rule_issues(&program, &projection, &profiles);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "road_edge_mismatch");
    }

    #[test]
    fn starter_city_defines_street_intersections_without_filling_the_cloverleaf() {
        let program = starter_program();
        let projection = map::project(&program);
        assert_eq!(projection.unresolved, 0);
        assert_eq!(projection.segments.len(), 72);
        let intersections = map::intersections(&program, &projection);
        assert_eq!(intersections.len(), 12);
        for intersection in &intersections {
            assert_eq!(intersection.ports.len(), intersection.kind.arm_count());
            assert!(map::intersection_arms(&projection, &intersection.point_ref).is_empty());
            for port in &intersection.ports {
                assert!(projection
                    .segments
                    .iter()
                    .any(
                        |segment| segment.start_ref.as_deref() == Some(&port.point_ref)
                            || segment.end_ref.as_deref() == Some(&port.point_ref)
                    ));
            }
        }
        assert!(design_rule_issues(
            &program,
            &projection,
            &paint_profiles(&program, &projection)
        )
        .is_empty());
        assert_eq!(
            intersections
                .iter()
                .filter(|intersection| intersection.kind == map::IntersectionKind::ThreeWay)
                .count(),
            5
        );
        assert_eq!(
            intersections
                .iter()
                .filter(|intersection| intersection.kind == map::IntersectionKind::RightTurn)
                .count(),
            3
        );
        assert_eq!(
            intersections
                .iter()
                .filter(|intersection| intersection.kind == map::IntersectionKind::FourWay)
                .count(),
            4
        );
        for reference in ["ew_3", "ew_5", "ns_2", "ns_4"] {
            assert_eq!(map::intersection_arms(&projection, reference).len(), 2);
            assert!(!intersections
                .iter()
                .any(|intersection| intersection.point_ref == reference));
        }
        for (reference, highway, lane, t) in [
            ("nw_exit", "interstate_east_west_04", 1, 0.5),
            ("sw_merge", "interstate_east_west_04", 7, 0.5),
            ("ne_merge", "interstate_east_west_05", 1, 0.5),
            ("se_exit", "interstate_east_west_05", 7, 0.5),
            ("nw_merge", "interstate_north_south_04", 1, 0.5),
            ("ne_exit", "interstate_north_south_04", 7, 0.5),
            ("sw_exit", "interstate_north_south_03", 1, 0.5),
            ("se_merge", "interstate_north_south_03", 7, 0.5),
            ("nw_direct_exit", "interstate_north_south_06", 1, 0.756098),
            ("nw_direct_merge", "interstate_east_west_02", 1, 0.208333),
            ("ne_direct_exit", "interstate_east_west_07", 1, 0.696970),
            ("ne_direct_merge", "interstate_north_south_06", 7, 0.756098),
            ("sw_direct_exit", "interstate_east_west_02", 7, 0.208333),
            ("sw_direct_merge", "interstate_north_south_01", 1, 0.243902),
            ("se_direct_exit", "interstate_north_south_01", 7, 0.243902),
            ("se_direct_merge", "interstate_east_west_07", 7, 0.696970),
        ] {
            assert_eq!(
                map::lane_attachment(&program, reference),
                Some((highway.to_owned(), lane, t))
            );
        }
        for (name, highway_at_start, horizontal) in [
            ("nw_loop_a_mouth", true, true),
            ("nw_loop_c_merge", false, false),
            ("sw_loop_a_mouth", true, false),
            ("sw_loop_c_merge", false, true),
            ("ne_loop_a_mouth", true, false),
            ("ne_loop_c_merge", false, true),
            ("se_loop_a_mouth", true, true),
            ("se_loop_c_merge", false, false),
            ("nw_connector_mouth", true, false),
            ("nw_connector_merge", false, true),
            ("ne_connector_mouth", true, true),
            ("ne_connector_merge", false, false),
            ("sw_connector_mouth", true, true),
            ("sw_connector_merge", false, false),
            ("se_connector_mouth", true, false),
            ("se_connector_merge", false, true),
        ] {
            let mouth = projection
                .segments
                .iter()
                .find(|segment| segment.name.as_deref() == Some(name))
                .unwrap();
            let tangent = mouth.tangent_at(if highway_at_start { 0.0 } else { 1.0 });
            if horizontal {
                assert!(tangent.y.abs() < 0.001, "{name} bends at the highway");
                assert!(
                    (mouth.end.x - mouth.start.x).abs() > 0.85,
                    "{name} is too short"
                );
            } else {
                assert!(tangent.x.abs() < 0.001, "{name} bends at the highway");
                assert!(
                    (mouth.end.y - mouth.start.y).abs() > 0.85,
                    "{name} is too short"
                );
            }
        }
        for quadrant in ["nw", "ne", "sw", "se"] {
            let loop_prefix = format!("{quadrant}_loop_");
            let connector_prefix = format!("{quadrant}_connector");
            for loop_segment in projection.segments.iter().filter(|segment| {
                segment
                    .name
                    .as_deref()
                    .is_some_and(|name| name.starts_with(&loop_prefix))
            }) {
                for connector in projection.segments.iter().filter(|segment| {
                    segment
                        .name
                        .as_deref()
                        .is_some_and(|name| name.starts_with(&connector_prefix))
                }) {
                    for loop_step in 0..=100 {
                        let a = loop_segment.point_at(loop_step as f32 / 100.0);
                        for connector_step in 0..=100 {
                            let b = connector.point_at(connector_step as f32 / 100.0);
                            let separation_squared = (a.x - b.x).powi(2) + (a.y - b.y).powi(2);
                            assert!(
                                separation_squared > 0.22_f32.powi(2),
                                "{quadrant} connector crosses its loop"
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(map::intersection_arms(&projection, "ns_3").len(), 2);
        assert_eq!(map::intersection_arms(&projection, "ew_4").len(), 2);
        assert!(!intersections
            .iter()
            .any(|intersection| intersection.point_ref == "ew_2"));
        for reference in ["grid_0_0", "grid_0_3", "grid_2_0"] {
            assert!(intersections
                .iter()
                .any(|intersection| intersection.point_ref == reference
                    && intersection.kind == map::IntersectionKind::RightTurn));
        }
        let country_edge = projection
            .segments
            .iter()
            .find(|segment| segment.name.as_deref() == Some("country_road_1_edge"))
            .unwrap();
        let tangent = country_edge.tangent_at(0.0);
        assert!(tangent.x.abs() < 0.001);
        let profiles = paint_profiles(&program, &projection);
        assert!(design_rule_issues(&program, &projection, &profiles).is_empty());
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    }

    #[test]
    fn starter_grid_intersection_is_square_with_four_cross_section_ports() {
        let program = starter_program();
        let projection = map::project(&program);
        let intersection = map::intersections(&program, &projection)
            .into_iter()
            .find(|intersection| intersection.point_ref == "grid_1_1")
            .unwrap();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1000.0));
        let paint = intersection_paint(
            &program,
            &projection,
            &intersection,
            rect,
            egui::Vec2::ZERO,
            100.0,
        )
        .unwrap();
        let edge = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == intersection.road_name => Some(road),
                _ => None,
            })
            .unwrap();
        let edge_width = edge
            .elements
            .iter()
            .map(|element| map::element_world_width(element.kind))
            .sum::<f32>()
            * 100.0;
        assert!((paint.square.width() - paint.square.height()).abs() < 0.001);
        assert!((paint.square.width() - edge_width).abs() < 0.001);
        assert_eq!(paint.closed, RoadElementKind::Sidewalk);
        assert_eq!(paint.ports.len(), 4);
        for side in PortSide::ALL {
            assert_eq!(
                paint.ports.iter().filter(|port| port.side == side).count(),
                1
            );
        }
        assert!(paint.ports.iter().all(|port| port
            .bands
            .iter()
            .any(|(kind, _)| *kind == RoadElementKind::Sidewalk)));
        for port in &paint.ports {
            assert_eq!(port.bands.len(), edge.elements.len());
            for (index, (kind, band)) in port.bands.iter().enumerate() {
                assert_eq!(*kind, edge.elements[index].kind);
                assert!(paint.square.contains_rect(*band));
            }
        }
        let north = paint
            .ports
            .iter()
            .find(|port| port.side == PortSide::North)
            .unwrap();
        for right in [false, true] {
            let corner_x = if right {
                paint.square.right()
            } else {
                paint.square.left()
            };
            let disks = corner_disks(north, corner_x, paint.square.center().x, right);
            assert_eq!(
                disks.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
                vec![RoadElementKind::Sidewalk, RoadElementKind::ParkingLane]
            );
            let expected = 100.0
                * (map::element_world_width(RoadElementKind::Sidewalk)
                    + map::element_world_width(RoadElementKind::ParkingLane));
            assert!((disks.last().unwrap().1 - expected).abs() < 0.001);
        }
        let zoomed = intersection_paint(
            &program,
            &projection,
            &intersection,
            rect,
            egui::Vec2::ZERO,
            200.0,
        )
        .unwrap();
        assert!((zoomed.square.width() / paint.square.width() - 2.0).abs() < 0.001);
    }

    #[test]
    fn three_way_intersection_has_one_closed_side() {
        let program = starter_program();
        let projection = map::project(&program);
        let intersection = map::intersections(&program, &projection)
            .into_iter()
            .find(|intersection| intersection.point_ref == "grid_0_1")
            .unwrap();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1000.0));
        let paint = intersection_paint(
            &program,
            &projection,
            &intersection,
            rect,
            egui::Vec2::ZERO,
            100.0,
        )
        .unwrap();
        assert_eq!(paint.ports.len(), 3);
        assert_eq!(
            PortSide::ALL
                .iter()
                .filter(|side| !paint.ports.iter().any(|port| port.side == **side))
                .count(),
            1
        );
    }

    #[test]
    fn right_turn_uses_adjacent_square_ports() {
        let program = parse(
            "road lane: sidewalk-carlane-sidewalk; end; \
            center = map_point(0, 0); west = map_point(-1, 0); north = map_point(0, 1); \
            a = lane(west, -0.7, 0, -0.3, 0, center); \
            b = lane(center, 0, 0.3, 0, 0.7, north); \
            intersection2(center, lane);",
        )
        .unwrap();
        let projection = map::project(&program);
        let intersection = &map::intersections(&program, &projection)[0];
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1000.0));
        let paint = intersection_paint(
            &program,
            &projection,
            intersection,
            rect,
            egui::Vec2::ZERO,
            100.0,
        )
        .unwrap();
        assert_eq!(paint.ports.len(), 2);
        assert!(paint.ports.iter().any(|port| port.side == PortSide::West));
        assert!(paint.ports.iter().any(|port| port.side == PortSide::North));
        assert_eq!(paint.closed, RoadElementKind::Sidewalk);
        let expected = 100.0
            * (2.0 * map::element_world_width(RoadElementKind::Sidewalk)
                + map::element_world_width(RoadElementKind::CarLane));
        assert!((paint.square.width() - expected).abs() < 0.001);
    }

    #[test]
    fn placed_intersection_ports_are_pickable_without_any_roads() {
        let mut program = default_program();
        let road = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) => Some(road.clone()),
                _ => None,
            })
            .unwrap();
        let center_ref = map::place_intersection(
            &mut program,
            map::IntersectionKind::FourWay,
            &road,
            WorldPoint { x: 0.0, y: 0.0 },
        );
        let projection = map::project(&program);
        let intersection = map::intersections(&program, &projection)
            .into_iter()
            .find(|intersection| intersection.point_ref == center_ref)
            .unwrap();
        assert_eq!(intersection.ports.len(), 4);
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1000.0));
        for port in &intersection.ports {
            let screen = screen_of(rect, egui::Vec2::ZERO, 420.0, port.position);
            let picked = intersection_port_near(
                &program,
                &projection,
                rect,
                egui::Vec2::ZERO,
                420.0,
                screen,
            )
            .unwrap();
            assert_eq!(picked.1, port.point_ref);
            assert_eq!(picked.2, road.name);
        }
    }

    #[test]
    fn library_click_places_an_intersection_in_the_visible_map() {
        let mut editor = sample_editor();
        editor.program = default_program();
        editor.selected_road = Some(0);
        editor.map_rect = Some(egui::Rect::from_min_size(
            egui::pos2(100.0, 100.0),
            egui::vec2(800.0, 600.0),
        ));
        editor.map_pan = egui::vec2(-200.0, 100.0);
        editor.map_zoom = 100.0;
        let reference = editor
            .add_intersection_from_library(map::IntersectionKind::FourWay)
            .unwrap();
        let projection = map::project(&editor.program);
        let intersection = map::intersections(&editor.program, &projection)
            .into_iter()
            .find(|intersection| intersection.point_ref == reference)
            .unwrap();
        assert_eq!(intersection.position, WorldPoint { x: 2.0, y: 1.0 });
        assert_eq!(intersection.ports.len(), 4);
        assert!(projection.segments.is_empty());
        assert_eq!(
            editor.selected_intersection.as_deref(),
            Some(reference.as_str())
        );
        assert!(editor.map_tool == MapTool::Select);
        let second = editor
            .add_intersection_from_library(map::IntersectionKind::FourWay)
            .unwrap();
        let placed = map::intersections(&editor.program, &map::project(&editor.program));
        assert_ne!(second, reference);
        assert_ne!(placed[0].position, placed[1].position);
    }

    #[test]
    fn port_bands_meet_on_diagonals_without_overlapping_at_corners() {
        let center = egui::pos2(50.0, 50.0);
        let band = egui::Rect::from_min_max(egui::pos2(50.0, 50.0), egui::pos2(100.0, 100.0));
        for side in [PortSide::East, PortSide::South] {
            let polygon = clipped_port_band(band, side, center);
            assert!(polygon.len() >= 3);
            for point in polygon {
                for (x, y) in side.wedge_planes() {
                    assert!(x * (point.x - center.x) + y * (point.y - center.y) >= -0.001);
                }
            }
        }
    }

    #[test]
    fn design_check_reports_an_approach_that_exceeds_the_intersection_edge() {
        let program = parse(
            "road narrow: sidewalk-carlane-sidewalk; end; \
            road wide: sidewalk-carlane-carlane-sidewalk; end; \
            center = map_point(0, 0); west = map_point(-1, 0); \
            east = map_point(1, 0); north = map_point(0, 1); \
            a = wide(west, -0.7, 0, -0.3, 0, center); \
            b = narrow(center, 0.3, 0, 0.7, 0, east); \
            c = narrow(center, 0, 0.3, 0, 0.7, north); \
            intersection3(center, narrow);",
        )
        .unwrap();
        let projection = map::project(&program);
        let profiles = paint_profiles(&program, &projection);
        let issues = design_rule_issues(&program, &projection, &profiles);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "intersection_edge_mismatch");
        assert_eq!(issues[0].roads, ["a", "narrow"]);
    }

    #[test]
    fn exit_carries_sidewalk_without_shrinking_any_band() {
        let mut program =
            parse("road boulevard: middle-carlane-carlane-sidewalk-verge; end;").unwrap();
        let road = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let parent = map::append(
            &mut program,
            &road,
            WorldPoint { x: 0.0, y: 0.0 },
            WorldPoint { x: 4.0, y: 0.0 },
        );
        let (attachment, start) =
            map::create_lane_point(&mut program, SegmentSource::TopLevel(parent), 2, 0.5).unwrap();
        let exit = map::append_exit(
            &mut program,
            &road,
            map::SplitMode::Dedicated,
            start,
            &attachment,
            WorldPoint { x: 5.0, y: -1.5 },
            None,
        )
        .unwrap();
        assert!(map::create_two_segment_split(
            &mut program,
            &attachment,
            exit.entry,
            exit.full,
            map::SplitMode::Dedicated,
        ));
        let projection = map::project(&program);
        let profiles = paint_profiles(&program, &projection);
        let entry_index = projection
            .segments
            .iter()
            .position(|segment| segment.source == exit.entry)
            .unwrap();
        let through = projection
            .segments
            .iter()
            .find(|segment| Some(segment.source) == exit.through)
            .unwrap();
        let entry_road = road_for_segment(&program, &projection.segments[entry_index]).unwrap();
        let through_road = road_for_segment(&program, through).unwrap();
        assert_eq!(entry_road.elements, road.elements[2..]);
        assert_eq!(
            through_road
                .elements
                .iter()
                .map(|element| element.kind)
                .collect::<Vec<_>>(),
            vec![
                RoadElementKind::Middle,
                RoadElementKind::CarLane,
                RoadElementKind::Barrier
            ]
        );
        let (start_widths, _) = section_widths(entry_road, 100.0, &profiles[entry_index], 0.0);
        let (end_widths, _) = section_widths(entry_road, 100.0, &profiles[entry_index], 1.0);
        assert_eq!(start_widths, end_widths);
        assert!(start_widths.iter().all(|width| *width > 0.0));
        let parent_segment = projection
            .segments
            .iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(parent))
            .unwrap();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let zoom = 100.0;
        let parent_normal = screen_normal_at(parent_segment, 1.0);
        let parent_center = screen_of(rect, egui::Vec2::ZERO, zoom, parent_segment.end);
        let parent_widths: Vec<_> = road
            .elements
            .iter()
            .map(|element| map_element_width(element.kind, zoom))
            .collect();
        let parent_left = parent_center - parent_normal * (parent_widths.iter().sum::<f32>() / 2.0);
        let entry_segment = &projection.segments[entry_index];
        let entry_center = screen_of(rect, egui::Vec2::ZERO, zoom, entry_segment.start);
        let (_, entry_offset) = section_widths(entry_road, zoom, &profiles[entry_index], 0.0);
        let entry_left = entry_center + screen_normal_at(entry_segment, 0.0) * entry_offset;
        let expected_entry_left =
            parent_left + parent_normal * parent_widths[..2].iter().sum::<f32>();
        assert!(entry_left.distance(expected_entry_left) < 0.01);
        let through_center = screen_of(rect, egui::Vec2::ZERO, zoom, through.start);
        let through_width: f32 = through_road
            .elements
            .iter()
            .map(|element| map_element_width(element.kind, zoom))
            .sum();
        let through_left = through_center - screen_normal_at(through, 0.0) * (through_width / 2.0);
        assert!(through_left.distance(parent_left) < 0.01);
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    }

    #[test]
    fn file_new_clears_the_demo_and_keeps_the_road_library() {
        let mut editor = sample_editor();
        editor.program = starter_program();
        editor.selected_endpoint = Some((map::project(&editor.program).segments[0].source, true));
        editor.dirty = true;
        editor.new_document();
        assert!(map::project(&editor.program).segments.is_empty());
        assert_eq!(editor.program, default_program());
        assert!(matches!(editor.map_tool, MapTool::Draw));
        assert!(editor.selected_endpoint.is_none());
        assert!(!editor.dirty);
    }

    #[test]
    fn graphical_edits_preserve_other_statements_and_round_trip() {
        let mut editor = sample_editor();
        let rest = editor.program.statements[1..].to_vec();
        editor.selected_element = Some(0);
        editor.add_element();
        assert_eq!(
            editor.road().unwrap().elements[1].kind,
            RoadElementKind::BikeLane
        );
        editor.move_element(1);
        assert_eq!(
            editor.road().unwrap().elements[2].kind,
            RoadElementKind::BikeLane
        );
        editor.remove_element();
        editor.add_road();
        assert_eq!(&editor.program.statements[1..3], &rest);
        assert_eq!(
            parse(&editor.program.to_road_source()).unwrap(),
            editor.program
        );
    }

    #[test]
    fn deleting_road_keeps_block_and_calls() {
        let mut editor = sample_editor();
        let rest = editor.program.statements[1..].to_vec();
        editor.remove_road();
        assert_eq!(editor.program.statements, rest);
        assert!(editor.selected_road.is_none());
        assert_eq!(
            parse(&editor.program.to_road_source()).unwrap(),
            editor.program
        );
    }

    #[test]
    fn renaming_a_road_updates_calls_in_blocks() {
        let mut editor = sample_editor();
        editor
            .program
            .statements
            .push(Statement::Call(opencity::Call {
                assigns: Vec::new(),
                name: "intersection3".into(),
                args: vec![
                    Argument::Identifier("junction".into()),
                    Argument::Identifier("normal_road".into()),
                ],
            }));
        assert!(editor.rename_road("main_road"));
        let Statement::Block(block) = &editor.program.statements[1] else {
            panic!("expected block")
        };
        assert_eq!(block.calls[0].name, "main_road");
        assert_eq!(block.calls[1].name, "main_road");
        let Statement::Call(intersection) = editor.program.statements.last().unwrap() else {
            unreachable!()
        };
        assert_eq!(
            intersection.args[1],
            Argument::Identifier("main_road".into())
        );
        assert_eq!(
            parse(&editor.program.to_road_source()).unwrap(),
            editor.program
        );
    }

    #[test]
    fn map_hit_testing_respects_pan_and_zoom() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let pan = egui::vec2(45.0, -30.0);
        let world = WorldPoint { x: 2.0, y: -1.0 };
        let screen = screen_of(rect, pan, 80.0, world);
        assert_eq!(world_of(rect, pan, 80.0, screen), world);
        let segment = map::Segment {
            source: SegmentSource::TopLevel(1),
            name: Some("road_1".into()),
            road_name: "neighborhood_street".into(),
            start: world,
            end: WorldPoint { x: 4.0, y: -1.0 },
            controls: [
                WorldPoint { x: 2.67, y: -1.0 },
                WorldPoint { x: 3.33, y: -1.0 },
            ],
            start_ref: None,
            end_ref: None,
        };
        assert_eq!(
            segment_near(
                &[segment.clone()],
                &default_program(),
                rect,
                pan,
                80.0,
                screen
            ),
            Some(segment.source)
        );
        assert_eq!(
            endpoint_near(&[segment], &default_program(), rect, pan, 80.0, screen),
            Some((SegmentSource::TopLevel(1), false))
        );
    }

    #[test]
    fn every_cross_section_element_has_a_pickable_end() {
        let mut program = parse("road highway: verge-carlane-middle-carlane-verge; end;").unwrap();
        let road = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        map::append(
            &mut program,
            &road,
            WorldPoint { x: 0.0, y: 0.0 },
            WorldPoint { x: 4.0, y: 0.0 },
        );
        let candidates = element_end_candidates(&map::project(&program), &program);
        assert_eq!(candidates.len(), road.elements.len() * 2);
        let target = candidates
            .iter()
            .find(|candidate| candidate.element_index == 3 && candidate.t == 1.0)
            .unwrap();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let pointer = screen_of(rect, egui::Vec2::ZERO, 420.0, target.position);
        let picked = element_end_near(&candidates, rect, egui::Vec2::ZERO, 420.0, pointer).unwrap();
        assert_eq!((picked.element_index, picked.t), (3, 1.0));
    }

    #[test]
    fn older_lane_branch_can_switch_to_keep_lane_mode() {
        let mut program = parse(
            "road highway: verge-carlane-verge; end; road ramp: verge-carlane-verge; end;",
        )
        .unwrap();
        let highway = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let ramp_road = match &program.statements[1] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let (parent, _) = map::append_connected(
            &mut program,
            &highway,
            WorldPoint { x: 0.0, y: 0.0 },
            None,
            WorldPoint { x: 4.0, y: 0.0 },
            None,
        );
        let (reference, position) =
            map::create_lane_point(&mut program, SegmentSource::TopLevel(parent), 1, 0.5).unwrap();
        let (ramp, _) = map::append_connected(
            &mut program,
            &ramp_road,
            position,
            Some(&reference),
            WorldPoint { x: 6.0, y: -1.0 },
            None,
        );
        let ramp_name = map::project(&program)
            .segments
            .into_iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(ramp))
            .unwrap()
            .name
            .unwrap();
        assert_eq!(
            map::lane_splits(&program)[0].mode,
            map::SplitMode::Dedicated
        );
        assert!(map::set_split_mode(
            &mut program,
            &ramp_name,
            map::SplitMode::KeepLane
        ));
        assert_eq!(map::lane_splits(&program).len(), 1);
        assert_eq!(map::lane_splits(&program)[0].mode, map::SplitMode::KeepLane);
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    }
}
