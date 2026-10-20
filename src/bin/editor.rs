use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText};
use opencity::{
    parse, parse_road_file, Program, RoadDefinition, RoadElement, RoadElementKind, Statement,
};

#[path = "editor/map.rs"]
mod map;
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
    RoadElementKind::Shoulder,
    RoadElementKind::Verge,
];

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum RoadShape {
    Straight,
    Curve,
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
        "divided_highway" | "interstate_three_lane" | "interstate_four_lane" => {
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
        "interstate_three_lane" => "Interstate | 3 lanes/way",
        "interstate_four_lane" => "Interstate | 4 lanes/way",
        _ => name,
    }
}

fn default_program() -> Program {
    parse(include_str!("../../examples/default_roads.road")).expect("bundled road library parses")
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
    path: Option<PathBuf>,
    dirty: bool,
    selected_road: Option<usize>,
    selected_element: Option<usize>,
    selected_segment: Option<SegmentSource>,
    segment_name_draft: String,
    road_name_draft: String,
    new_kind: RoadElementKind,
    view: View,
    map_tool: MapTool,
    road_shape: RoadShape,
    map_start: Option<WorldPoint>,
    map_pan: egui::Vec2,
    map_zoom: f32,
    dragging_endpoint: Option<(SegmentSource, bool)>,
    dragging_control: Option<(SegmentSource, bool)>,
    dragging_background: bool,
    show_source: bool,
    status: String,
    pending: Option<PendingAction>,
    confirm_delete_road: bool,
    #[cfg(target_os = "macos")]
    native_menu: Option<NativeFileMenu>,
}

impl RoadEditor {
    fn new(cc: &eframe::CreationContext<'_>, open_path: Option<PathBuf>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(24, 29, 37);
        visuals.window_fill = Color32::from_rgb(24, 29, 37);
        visuals.widgets.active.bg_fill = Color32::from_rgb(57, 87, 111);
        cc.egui_ctx.set_visuals(visuals);
        let mut app = Self {
            program: default_program(),
            path: None,
            dirty: false,
            selected_road: Some(0),
            selected_element: None,
            selected_segment: None,
            segment_name_draft: String::new(),
            road_name_draft: "neighborhood_street".into(),
            new_kind: RoadElementKind::CarLane,
            view: View::Map,
            map_tool: MapTool::Draw,
            road_shape: RoadShape::Straight,
            map_start: None,
            map_pan: egui::Vec2::ZERO,
            map_zoom: 100.0,
            dragging_endpoint: None,
            dragging_control: None,
            dragging_background: false,
            show_source: false,
            status: "Click two points to place a neighborhood street, or choose another road type."
                .into(),
            pending: None,
            confirm_delete_road: false,
            #[cfg(target_os = "macos")]
            native_menu: NativeFileMenu::install(),
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
        if let Some(road) = self.road_mut() {
            let at = selected
                .map_or(road.elements.len(), |index| index + 1)
                .min(road.elements.len());
            road.elements.insert(at, RoadElement { kind, catch: None });
            self.selected_element = Some(at);
            self.dirty = true;
        }
    }

    fn move_element(&mut self, offset: isize) {
        let Some(index) = self.selected_element else {
            return;
        };
        if let Some(road) = self.road_mut() {
            let target = index as isize + offset;
            if target >= 0 && (target as usize) < road.elements.len() {
                road.elements.swap(index, target as usize);
                self.selected_element = Some(target as usize);
                self.dirty = true;
            }
        }
    }

    fn remove_element(&mut self) {
        let Some(index) = self.selected_element else {
            return;
        };
        if let Some(road) = self.road_mut() {
            if road.elements.len() > 1 && index < road.elements.len() {
                road.elements.remove(index);
                self.selected_element = Some(index.min(road.elements.len() - 1));
                self.dirty = true;
            } else {
                self.status = "A road needs at least one element.".into();
            }
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
        if self.road().is_none() { return; }
        ui.label("Road name");
        let response = ui.text_edit_singleline(&mut self.road_name_draft);
        let valid = Self::valid_name(&self.road_name_draft);
        let duplicate = self.program.statements.iter().enumerate().any(|(index, statement)| {
            Some(index) != self.selected_road
                && matches!(statement, Statement::Road(road) if road.name == self.road_name_draft)
        });
        if !valid || duplicate {
            ui.colored_label(
                Color32::from_rgb(245, 150, 110),
                if duplicate { "A road already has this name." } else { "Use letters, digits, underscores; start with a letter or _." },
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
                self.segment_name_draft.clear();
                self.map_start = None;
                self.dragging_endpoint = None;
                self.dragging_control = None;
                self.dragging_background = false;
                self.road_name_draft = self
                    .selected_road
                    .and_then(|index| match &program.statements[index] {
                        Statement::Road(road) => Some(road.name.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                self.program = program;
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
        self.path = None;
        self.selected_road = Some(0);
        self.selected_element = None;
        self.selected_segment = None;
        self.segment_name_draft.clear();
        self.road_name_draft = "neighborhood_street".into();
        self.view = View::Map;
        self.map_tool = MapTool::Draw;
        self.road_shape = RoadShape::Straight;
        self.map_start = None;
        self.map_pan = egui::Vec2::ZERO;
        self.map_zoom = 100.0;
        self.dragging_endpoint = None;
        self.dragging_control = None;
        self.dragging_background = false;
        self.dirty = false;
        self.confirm_delete_road = false;
        self.status = "New document".into();
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
                ("New    ⌘N", FileAction::New),
                ("Open…    ⌘O", FileAction::Open),
                ("Save    ⌘S", FileAction::Save),
                ("Save As…    ⇧⌘S", FileAction::SaveAs),
            ] {
                if ui.button(label).clicked() {
                    self.file_action(action);
                    ui.close();
                }
            }
        });
    }

    fn top_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(29, 35, 45))
                    .inner_margin(egui::Margin::symmetric(14, 9)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("OPEN CITY")
                            .strong()
                            .color(Color32::from_rgb(105, 190, 220)),
                    );
                    ui.separator();
                    self.file_menu(ui);
                    ui.separator();
                    if ui
                        .selectable_value(&mut self.view, View::Map, "Map")
                        .clicked()
                    {
                        self.map_start = None;
                    }
                    ui.selectable_value(&mut self.view, View::Section, "Cross section");
                    ui.separator();
                    if ui
                        .add(egui::Button::new((icon("plus"), "New type")))
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
                        }
                        if ui
                            .add_enabled(
                                self.road().is_some(),
                                egui::Button::new((icon("plus"), "Draw road")),
                            )
                            .clicked()
                        {
                            self.map_tool = MapTool::Draw;
                            self.map_start = None;
                        }
                        if self.map_tool == MapTool::Draw {
                            ui.selectable_value(&mut self.road_shape, RoadShape::Straight, "Straight");
                            ui.selectable_value(&mut self.road_shape, RoadShape::Curve, "Curve");
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
                            .add_enabled(enabled, egui::Button::new((icon("plus"), "Add element")))
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
                            .color(Color32::from_gray(150)),
                    );
                    ui.label(
                        RichText::new(if self.view == View::Map {
                            "Choose a type to draw"
                        } else {
                            "Choose a type to edit"
                        })
                        .small()
                        .color(Color32::from_gray(120)),
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
                                .color(Color32::from_gray(150)),
                        );
                        ui.add_space(5.0);
                        for (index, road) in roads {
                            if ui
                                .selectable_label(
                                    self.selected_road == Some(index),
                                    preset_label(&road.name),
                                )
                                .clicked()
                            {
                                chosen = Some(index);
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
                            .color(Color32::from_gray(150)),
                        );
                        if projection.segments.is_empty() {
                            ui.label(
                                RichText::new("The map is empty. Pick a type above.")
                                    .small()
                                    .color(Color32::GRAY),
                            );
                        }
                        for (number, segment) in projection.segments.iter().enumerate() {
                            let label = format!("{}  {} | {}", number + 1,
                                segment.name.as_deref().unwrap_or("Unnamed road"),
                                preset_label(&segment.road_name));
                            if ui
                                .selectable_label(
                                    self.selected_segment == Some(segment.source),
                                    label,
                                )
                                .clicked()
                            {
                                self.selected_segment = Some(segment.source);
                                self.segment_name_draft = segment.name.clone().unwrap_or_default();
                                self.map_tool = MapTool::Select;
                                self.map_start = None;
                            }
                        }
                        if let Some(segment) = projection.segments.iter().find(|segment| Some(segment.source) == self.selected_segment) {
                            ui.add_space(12.0);
                            ui.separator();
                            ui.label("Selected road name");
                            let changed = ui.text_edit_singleline(&mut self.segment_name_draft).changed();
                            let valid = Self::valid_name(&self.segment_name_draft);
                            if !valid && !self.segment_name_draft.is_empty() {
                                ui.colored_label(Color32::from_rgb(245, 150, 110), "Use letters, digits, and underscores.");
                            }
                            if changed && valid && segment.source.editable()
                                && map::set_name(&mut self.program, segment.source, self.segment_name_draft.clone()) {
                                self.dirty = true;
                            }
                            ui.label(format!("Cross section: {}", preset_label(&segment.road_name)));
                        }
                        if projection.unresolved > 0 {
                            ui.colored_label(
                                Color32::from_rgb(226, 170, 95),
                                format!(
                                    "{} road call(s) need numeric endpoints",
                                    projection.unresolved
                                ),
                            );
                        }
                    } else {
                        let other_count = self
                            .program
                            .statements
                            .iter()
                            .filter(|statement| !matches!(statement, Statement::Road(_)))
                            .count();
                        if other_count > 0 {
                            ui.add_space(18.0);
                            ui.separator();
                            ui.label(
                                RichText::new("OTHER STATEMENTS")
                                    .small()
                                    .strong()
                                    .color(Color32::from_gray(150)),
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
                                    Statement::Call(call) => {
                                        ui.label(format!("↳  {}(…)", call.name));
                                    }
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
                        .color(Color32::from_gray(150)),
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
                            .color(Color32::from_gray(150)),
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
                        .add(egui::Button::new((icon("minus"), "Remove element")))
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
                if ui.add(egui::Button::new((icon("x"), "Delete road"))).clicked() {
                    delete_road = true;
                }
                if delete_road { self.confirm_delete_road = true; }
            });
    }

    fn map_inspector(&mut self, ui: &mut egui::Ui) {
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
                        .color(Color32::from_gray(150)),
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
                .color(Color32::from_gray(150)),
        );
        ui.heading(preset_label(&segment.road_name));
        ui.add_space(8.0);
        if segment.source.editable() {
            for (label, is_end, point) in
                [("Start", false, segment.start), ("End", true, segment.end)]
            {
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
                    && map::set_endpoint(
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
            ui.label(
                RichText::new("Drag an endpoint on the map to reshape it.")
                    .small()
                    .color(Color32::from_gray(150)),
            );
            ui.add_space(12.0);
            if ui
                .add(egui::Button::new((icon("x"), "Remove placement")))
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
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("Map");
                ui.label(
                    RichText::new(if self.map_tool == MapTool::Draw {
                        "Click a start and end point | drag empty map to pan | Esc cancels"
                    } else {
                        "Click to select | drag endpoints | drag empty map to pan | scroll zooms"
                    })
                    .small()
                    .color(Color32::from_gray(160)),
                );
            });
            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            if response.hovered() {
                let scroll = ui.input(|input| input.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    let pointer = ui.ctx().pointer_hover_pos().unwrap_or(rect.center());
                    let anchored = world_of(rect, self.map_pan, self.map_zoom, pointer);
                    self.map_zoom = (self.map_zoom * (1.0 + scroll * 0.002)).clamp(32.0, 420.0);
                    self.map_pan = pointer
                        - rect.center()
                        - egui::vec2(anchored.x * self.map_zoom, -anchored.y * self.map_zoom);
                }
            }
            let before = map::project(&self.program);
            if response.drag_started_by(egui::PointerButton::Primary) {
                let origin = ui
                    .input(|input| input.pointer.press_origin())
                    .or_else(|| response.interact_pointer_pos());
                if let Some(origin) = origin.filter(|point| rect.contains(*point)) {
                    self.dragging_endpoint = if self.map_tool == MapTool::Select {
                        endpoint_near(&before.segments, rect, self.map_pan, self.map_zoom, origin)
                    } else {
                        None
                    };
                    if let Some((source, _)) = self.dragging_endpoint {
                        self.selected_segment = Some(source);
                    }
                    self.dragging_background = self.dragging_endpoint.is_none()
                        && segment_near(
                            &before.segments,
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
            if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.map_start = None;
                self.dragging_endpoint = None;
                self.dragging_background = false;
                self.map_tool = MapTool::Select;
            }
            if let Some(pointer) = ui
                .ctx()
                .pointer_hover_pos()
                .filter(|point| rect.contains(*point))
            {
                if self.map_tool == MapTool::Draw
                    && response.clicked_by(egui::PointerButton::Primary)
                {
                    let point = snap_point(world_of(rect, self.map_pan, self.map_zoom, pointer));
                    if let Some(start) = self.map_start {
                        if start != point {
                            if let Some(road) = self.road().cloned() {
                                let index = map::append(&mut self.program, &road, start, point);
                                self.selected_segment = Some(SegmentSource::TopLevel(index));
                                self.map_start = Some(point);
                                self.dirty = true;
                                self.status = format!(
                                    "Placed {} | click another endpoint or press Esc",
                                    preset_label(&road.name)
                                );
                            }
                        }
                    } else {
                        self.map_start = Some(point);
                    }
                }
                if self.map_tool == MapTool::Select {
                    if response.clicked_by(egui::PointerButton::Primary) {
                        self.selected_segment = segment_near(
                            &before.segments,
                            rect,
                            self.map_pan,
                            self.map_zoom,
                            pointer,
                        );
                    }
                    if response.dragged_by(egui::PointerButton::Primary) {
                        if let Some((source, is_end)) = self.dragging_endpoint {
                            let point =
                                snap_point(world_of(rect, self.map_pan, self.map_zoom, pointer));
                            if map::set_endpoint(&mut self.program, source, is_end, point) {
                                self.dirty = true;
                            }
                        }
                    }
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                self.dragging_endpoint = None;
                self.dragging_background = false;
            }

            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, Color32::from_rgb(19, 30, 34));
            paint_grid(&painter, rect, self.map_pan, self.map_zoom);
            let projection = map::project(&self.program);
            for segment in &projection.segments {
                let a = screen_of(rect, self.map_pan, self.map_zoom, segment.start);
                let b = screen_of(rect, self.map_pan, self.map_zoom, segment.end);
                let selected = self.selected_segment == Some(segment.source);
                let category = preset_category(&segment.road_name);
                let color = if selected {
                    Color32::from_rgb(124, 207, 245)
                } else {
                    map_road_color(category)
                };
                let lane_count = self
                    .program
                    .statements
                    .iter()
                    .find_map(|statement| match statement {
                        Statement::Road(road) if road.name == segment.road_name => Some(
                            road.elements
                                .iter()
                                .filter(|element| element.kind == RoadElementKind::CarLane)
                                .count(),
                        ),
                        _ => None,
                    })
                    .unwrap_or(2);
                let width = (7.0 + lane_count as f32 * 2.5).min(26.0);
                painter.line_segment(
                    [a, b],
                    egui::Stroke::new(width + 5.0, Color32::from_rgb(11, 18, 22)),
                );
                painter.line_segment([a, b], egui::Stroke::new(width, color));
                painter.line_segment(
                    [a, b],
                    egui::Stroke::new(1.2, Color32::from_white_alpha(145)),
                );
                let midpoint = a + (b - a) * 0.5;
                painter.text(
                    midpoint + egui::vec2(0.0, -16.0),
                    egui::Align2::CENTER_BOTTOM,
                    preset_label(&segment.road_name),
                    egui::FontId::proportional(12.0),
                    Color32::from_rgb(226, 235, 236),
                );
                if selected {
                    for point in [a, b] {
                        painter.circle_filled(point, 7.0, Color32::WHITE);
                        painter.circle_stroke(point, 7.0, egui::Stroke::new(2.0, color));
                    }
                }
            }
            if let Some(start) = self.map_start {
                let a = screen_of(rect, self.map_pan, self.map_zoom, start);
                painter.circle_filled(a, 5.0, Color32::from_rgb(127, 216, 199));
                if let Some(pointer) = ui
                    .ctx()
                    .pointer_hover_pos()
                    .filter(|point| rect.contains(*point))
                {
                    painter.line_segment(
                        [a, pointer],
                        egui::Stroke::new(3.0, Color32::from_rgb(127, 216, 199)),
                    );
                }
            }
            painter.text(
                rect.left_top() + egui::vec2(18.0, 18.0),
                egui::Align2::LEFT_TOP,
                format!(
                    "{} segments  |  {} road types",
                    projection.segments.len(),
                    self.program
                        .statements
                        .iter()
                        .filter(|statement| matches!(statement, Statement::Road(_)))
                        .count()
                ),
                egui::FontId::proportional(13.0),
                Color32::from_rgb(175, 198, 196),
            );
            painter.text(
                rect.right_top() + egui::vec2(-18.0, 18.0),
                egui::Align2::RIGHT_TOP,
                "N ↑",
                egui::FontId::proportional(14.0),
                Color32::from_rgb(175, 198, 196),
            );
            painter.text(
                rect.right_bottom() + egui::vec2(-18.0, -16.0),
                egui::Align2::RIGHT_BOTTOM,
                format!("1 unit = {:.0} px", self.map_zoom),
                egui::FontId::proportional(12.0),
                Color32::from_rgb(150, 174, 172),
            );
            if projection.segments.is_empty() {
                painter.text(
                    rect.center() + egui::vec2(0.0, -18.0),
                    egui::Align2::CENTER_CENTER,
                    "Your map starts here",
                    egui::FontId::proportional(24.0),
                    Color32::from_rgb(226, 237, 235),
                );
                painter.text(
                    rect.center() + egui::vec2(0.0, 18.0),
                    egui::Align2::CENTER_CENTER,
                    "Choose a road type on the left, then click two points.",
                    egui::FontId::proportional(14.0),
                    Color32::from_rgb(153, 177, 175),
                );
            }
        });
    }

    fn section_canvas(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            ui.add_space(18.0);
            let Some(road) = self.road() else {
                ui.vertical_centered(|ui| {
                    ui.add_space(90.0);
                    ui.add(icon("box"));
                    ui.heading("No road selected");
                    ui.label("Create a road from the toolbar, or open a .road file.");
                });
                return;
            };
            ui.heading(&road.name);
            ui.label(
                RichText::new("CROSS SECTION | left to right in source order")
                    .small()
                    .color(Color32::from_gray(150)),
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
                        let fill = kind_color(element.kind);
                        ui.painter()
                            .rect_filled(rect.shrink2(egui::vec2(2.0, 8.0)), 5.0, fill);
                        if selected {
                            ui.painter().rect_stroke(
                                rect.shrink(2.0),
                                5.0,
                                egui::Stroke::new(3.0, Color32::WHITE),
                                egui::StrokeKind::Outside,
                            );
                        }
                        let center = rect.center();
                        ui.painter().text(
                            egui::pos2(center.x, center.y - 12.0),
                            egui::Align2::CENTER_CENTER,
                            kind_short(element.kind),
                            egui::FontId::proportional(13.0),
                            Color32::WHITE,
                        );
                        if let Some(catch) = element.catch {
                            ui.painter().text(
                                egui::pos2(center.x, center.y + 15.0),
                                egui::Align2::CENTER_CENTER,
                                format!("${catch}"),
                                egui::FontId::monospace(12.0),
                                Color32::WHITE,
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
                .color(Color32::from_gray(160)),
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

fn snap_point(point: WorldPoint) -> WorldPoint {
    WorldPoint {
        x: (point.x * 4.0).round() / 4.0,
        y: (point.y * 4.0).round() / 4.0,
    }
}

fn segment_near(
    segments: &[map::Segment],
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<SegmentSource> {
    segments
        .iter()
        .rev()
        .find(|segment| {
            let a = screen_of(rect, pan, zoom, segment.start);
            let b = screen_of(rect, pan, zoom, segment.end);
            let line = b - a;
            let length_squared = line.length_sq();
            if length_squared < 1.0 {
                return pointer.distance_sq(a) < 16.0 * 16.0;
            }
            let along = ((pointer - a).dot(line) / length_squared).clamp(0.0, 1.0);
            pointer.distance_sq(a + line * along) < 16.0 * 16.0
        })
        .map(|segment| segment.source)
}

fn endpoint_near(
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
            let start = screen_of(rect, pan, zoom, segment.start);
            let end = screen_of(rect, pan, zoom, segment.end);
            if pointer.distance_sq(start) <= 14.0 * 14.0 {
                Some((segment.source, false))
            } else if pointer.distance_sq(end) <= 14.0 * 14.0 {
                Some((segment.source, true))
            } else {
                None
            }
        })
}

fn paint_grid(painter: &egui::Painter, rect: egui::Rect, pan: egui::Vec2, zoom: f32) {
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
            Color32::from_rgb(67, 100, 104)
        } else if x % 5 == 0 {
            Color32::from_rgb(45, 70, 72)
        } else {
            Color32::from_rgb(31, 49, 51)
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
            Color32::from_rgb(67, 100, 104)
        } else if y % 5 == 0 {
            Color32::from_rgb(45, 70, 72)
        } else {
            Color32::from_rgb(31, 49, 51)
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

fn map_road_color(category: PresetCategory) -> Color32 {
    match category {
        PresetCategory::City => Color32::from_rgb(118, 153, 169),
        PresetCategory::Rural => Color32::from_rgb(136, 165, 125),
        PresetCategory::Interstate => Color32::from_rgb(202, 157, 104),
        PresetCategory::Custom => Color32::from_rgb(154, 147, 184),
    }
}

impl eframe::App for RoadEditor {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
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
        self.top_bar(root);
        self.sidebar(root);
        self.inspector(root);
        egui::Panel::bottom("status").show(root, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&self.status)
                        .small()
                        .color(Color32::from_gray(170)),
                );
            });
        });
        match self.view {
            View::Map => self.map_canvas(root),
            View::Section => self.section_canvas(root),
        }
        self.pending_dialog(&ctx);
        self.delete_dialog(&ctx);
    }
}

fn icon(name: &str) -> egui::Image<'static> {
    let bytes: &'static [u8] = match name {
        "plus" => include_bytes!("../../assets/lucide/plus.svg"),
        "minus" => include_bytes!("../../assets/lucide/minus.svg"),
        "box" => include_bytes!("../../assets/lucide/box.svg"),
        _ => include_bytes!("../../assets/lucide/x.svg"),
    };
    egui::Image::from_bytes(format!("bytes://lucide/{name}.svg"), bytes)
        .fit_to_exact_size(egui::vec2(16.0, 16.0))
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
        RoadElementKind::Shoulder => "Shoulder",
        RoadElementKind::Verge => "Verge",
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
        RoadElementKind::Shoulder => "SHLD",
        RoadElementKind::Verge => "VERGE",
    }
}

fn kind_color(kind: RoadElementKind) -> Color32 {
    match kind {
        RoadElementKind::BuildingEdge => Color32::from_rgb(100, 107, 119),
        RoadElementKind::Sidewalk => Color32::from_rgb(139, 112, 89),
        RoadElementKind::BikeLane => Color32::from_rgb(58, 131, 105),
        RoadElementKind::ParkingLane => Color32::from_rgb(88, 105, 122),
        RoadElementKind::BusLane => Color32::from_rgb(164, 76, 71),
        RoadElementKind::CarLane => Color32::from_rgb(72, 95, 120),
        RoadElementKind::BusStop => Color32::from_rgb(162, 121, 53),
        RoadElementKind::Middle => Color32::from_rgb(106, 94, 65),
        RoadElementKind::Shoulder => Color32::from_rgb(93, 103, 102),
        RoadElementKind::Verge => Color32::from_rgb(71, 115, 77),
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
        RoadElementKind::Shoulder => 76.0,
        RoadElementKind::Verge => 86.0,
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
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "OpenCity Roads",
        options,
        Box::new(move |cc| Ok(Box::new(RoadEditor::new(cc, path.clone())))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_editor() -> RoadEditor {
        RoadEditor {
            program: parse(include_str!("../../examples/city.road")).unwrap(),
            path: None,
            dirty: false,
            selected_road: Some(0),
            selected_element: None,
            selected_segment: None,
            segment_name_draft: String::new(),
            road_name_draft: "normal_road".into(),
            new_kind: RoadElementKind::BikeLane,
            view: View::Map,
            map_tool: MapTool::Select,
            road_shape: RoadShape::Straight,
            map_start: None,
            map_pan: egui::Vec2::ZERO,
            map_zoom: 100.0,
            dragging_endpoint: None,
            dragging_control: None,
            dragging_background: false,
            show_source: false,
            status: String::new(),
            pending: None,
            confirm_delete_road: false,
            #[cfg(target_os = "macos")]
            native_menu: None,
        }
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
        assert!(editor.rename_road("main_road"));
        let Statement::Block(block) = &editor.program.statements[1] else {
            panic!("expected block")
        };
        assert_eq!(block.calls[0].name, "main_road");
        assert_eq!(block.calls[1].name, "main_road");
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
            road_name: "neighborhood_street".into(),
            start: world,
            end: WorldPoint { x: 4.0, y: -1.0 },
        };
        assert_eq!(
            segment_near(&[segment.clone()], rect, pan, 80.0, screen),
            Some(segment.source)
        );
        assert_eq!(
            endpoint_near(&[segment], rect, pan, 80.0, screen),
            Some((SegmentSource::TopLevel(1), false))
        );
    }
}
