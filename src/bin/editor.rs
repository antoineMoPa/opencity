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
            map_background: Color32::from_rgb(19, 30, 34),
            map_axis: Color32::from_rgb(67, 100, 104),
            map_major: Color32::from_rgb(45, 70, 72),
            map_minor: Color32::from_rgb(31, 49, 51),
            map_text: Color32::from_rgb(226, 237, 235),
            map_subtle: Color32::from_rgb(153, 177, 175),
            map_outline: Color32::from_rgb(11, 18, 22),
            map_selected: Color32::from_rgb(124, 207, 245),
            map_preview: Color32::from_rgb(127, 216, 199),
            map_centerline: Color32::from_white_alpha(90),
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
    map_start: Option<WorldPoint>,
    map_start_ref: Option<String>,
    branch_element: usize,
    branch_t: f32,
    branch_road_name: String,
    branch_mode: map::SplitMode,
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
        cc.egui_ctx
            .set_visuals_of(egui::Theme::Dark, editor_visuals(egui::Theme::Dark));
        cc.egui_ctx
            .set_visuals_of(egui::Theme::Light, editor_visuals(egui::Theme::Light));
        cc.egui_ctx.set_theme(egui::ThemePreference::System);
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
            map_start: None,
            map_start_ref: None,
            branch_element: usize::MAX,
            branch_t: 0.75,
            branch_road_name: "exit_ramp".into(),
            branch_mode: map::SplitMode::Dedicated,
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
                self.segment_name_draft.clear();
                self.map_start = None;
                self.map_start_ref = None;
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
        self.map_start = None;
        self.map_start_ref = None;
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
                                        "map_point" | "lane_point" | "lane_split"
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
                                            "map_point" | "lane_point" | "lane_split"
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
        ui.label(format!(
            "Cross section: {}",
            preset_label(&segment.road_name)
        ));
        if let Some(split) = map::lane_splits(&self.program)
            .into_iter()
            .find(|split| segment.name.as_deref() == Some(&split.ramp))
        {
            ui.label(format!(
                "Exit from {} | element {}",
                split.parent,
                split.element_index + 1
            ));
            let mut keep_lane = split.mode == map::SplitMode::KeepLane;
            if ui
                .checkbox(&mut keep_lane, "Keep lane on through road")
                .changed()
            {
                let mode = if keep_lane {
                    map::SplitMode::KeepLane
                } else {
                    map::SplitMode::Dedicated
                };
                if map::set_split_mode(&mut self.program, &split.ramp, mode) {
                    self.dirty = true;
                }
            }
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
                    let road_choices: Vec<_> = self
                        .program
                        .statements
                        .iter()
                        .enumerate()
                        .filter_map(|(index, statement)| match statement {
                            Statement::Road(road) => Some((index, road.name.clone())),
                            _ => None,
                        })
                        .collect();
                    if !road_choices
                        .iter()
                        .any(|(_, name)| *name == self.branch_road_name)
                    {
                        self.branch_road_name = road_choices
                            .first()
                            .map(|(_, name)| name.clone())
                            .unwrap_or_default();
                    }
                    egui::ComboBox::from_id_salt("branch-road-type")
                        .selected_text(preset_label(&self.branch_road_name))
                        .show_ui(ui, |ui| {
                            for (_, name) in &road_choices {
                                ui.selectable_value(
                                    &mut self.branch_road_name,
                                    name.clone(),
                                    preset_label(name),
                                );
                            }
                        });
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
                            "The interstate keeps this lane; the ramp grows beside it."
                        } else {
                            "This lane leaves the interstate and becomes the ramp."
                        })
                        .small()
                        .color(ui.visuals().weak_text_color()),
                    );
                    if ui.button("Start branch here").clicked() {
                        if let Some((reference, position)) = map::create_lane_point(
                            &mut self.program,
                            segment.source,
                            self.branch_element,
                            self.branch_t,
                        ) {
                            if let Some((index, name)) = road_choices
                                .into_iter()
                                .find(|(_, name)| *name == self.branch_road_name)
                            {
                                self.selected_road = Some(index);
                                self.road_name_draft = name;
                            }
                            self.map_tool = MapTool::Draw;
                            self.map_start = Some(position);
                            self.map_start_ref = Some(reference);
                            self.dirty = true;
                            self.status =
                                "Branch started | click an endpoint to place the exit or merge"
                                    .into();
                        }
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
                    RichText::new(if self.map_tool == MapTool::Draw {
                        "Click start and end, including colored element ends | drag empty map to pan | Enter or Esc finishes"
                    } else {
                        "Select a road | branch from its cross section in Inspector | drag points to edit"
                    })
                    .small()
                    .color(ui.visuals().weak_text_color()),
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
            let junctions = junction_candidates(&before);
            let element_ends = element_end_candidates(&before, &self.program);
            if response.drag_started_by(egui::PointerButton::Primary) {
                let origin = ui
                    .input(|input| input.pointer.press_origin())
                    .or_else(|| response.interact_pointer_pos());
                if let Some(origin) = origin.filter(|point| rect.contains(*point)) {
                    self.dragging_endpoint = if self.map_tool == MapTool::Select {
                        endpoint_near(
                            &before.segments,
                            &self.program,
                            rect,
                            self.map_pan,
                            self.map_zoom,
                            origin,
                        )
                    } else {
                        None
                    };
                    self.dragging_control = if self.map_tool == MapTool::Select
                        && self.dragging_endpoint.is_none()
                    {
                        control_near(&before.segments, rect, self.map_pan, self.map_zoom, origin)
                    } else {
                        None
                    };
                    if let Some((source, _)) = self.dragging_endpoint {
                        self.selected_segment = Some(source);
                    }
                    if let Some((source, _)) = self.dragging_control {
                        self.selected_segment = Some(source);
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
                    self.segment_name_draft.clear();
                    self.dirty = true;
                    self.status = "Road placement removed".into();
                }
            }
            if let Some(pointer) = ui
                .ctx()
                .pointer_hover_pos()
                .filter(|point| rect.contains(*point))
            {
                if self.map_tool == MapTool::Draw
                    && response.clicked_by(egui::PointerButton::Primary)
                {
                    let element_end =
                        element_end_near(&element_ends, rect, self.map_pan, self.map_zoom, pointer);
                    let picked = element_end
                        .map(|candidate| (candidate.position, None))
                        .or_else(|| {
                            junction_near(&junctions, rect, self.map_pan, self.map_zoom, pointer)
                        });
                    let (point, mut point_ref) = picked.unwrap_or_else(|| {
                        (
                            snap_point(world_of(rect, self.map_pan, self.map_zoom, pointer)),
                            None,
                        )
                    });
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
                        if start != point {
                            if let Some(road) = self.road().cloned() {
                                let branch_ref = self.map_start_ref.clone();
                                let (index, end_ref) = map::append_connected(
                                    &mut self.program,
                                    &road,
                                    start,
                                    self.map_start_ref.as_deref(),
                                    point,
                                    point_ref.as_deref(),
                                );
                                if let Some(reference) = branch_ref.as_deref() {
                                    map::create_lane_split(&mut self.program, reference,
                                        SegmentSource::TopLevel(index), self.branch_mode);
                                }
                                self.selected_segment = Some(SegmentSource::TopLevel(index));
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
                                self.status = format!(
                                    "Placed {} | click another endpoint or press Enter/Esc",
                                    preset_label(&road.name)
                                );
                            }
                        }
                    } else {
                        self.map_start = Some(point);
                        self.map_start_ref = point_ref;
                    }
                }
                if self.map_tool == MapTool::Select {
                    if response.clicked_by(egui::PointerButton::Primary) {
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
                        self.segment_name_draft = before
                            .segments
                            .iter()
                            .find(|segment| Some(segment.source) == self.selected_segment)
                            .and_then(|segment| segment.name.clone())
                            .unwrap_or_default();
                    }
                    if response.dragged_by(egui::PointerButton::Primary) {
                        if let Some((source, is_end)) = self.dragging_endpoint {
                            let point =
                                snap_point(world_of(rect, self.map_pan, self.map_zoom, pointer));
                            if map::set_connected_endpoint(&mut self.program, source, is_end, point)
                            {
                                self.dirty = true;
                            }
                        }
                        if let Some((source, second)) = self.dragging_control {
                            let point =
                                snap_point(world_of(rect, self.map_pan, self.map_zoom, pointer));
                            if map::set_control(&mut self.program, source, second, point) {
                                self.dirty = true;
                            }
                        }
                    }
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                self.dragging_endpoint = None;
                self.dragging_control = None;
                self.dragging_background = false;
            }

            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, palette.map_background);
            paint_grid(&painter, rect, self.map_pan, self.map_zoom, palette);
            let projection = map::project(&self.program);
            let profiles = paint_profiles(&self.program, &projection);
            let mut draw_order: Vec<_> = (0..projection.segments.len()).collect();
            draw_order.sort_by_key(|index| profiles[*index].ramp.is_some());
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
                if let Some(road) = road.filter(|_| profiles[index].ramp.is_some()
                    || !profiles[index].through.is_empty()) {
                    paint_profiled_road(&painter, rect, self.map_pan, self.map_zoom,
                        segment, road, selected, palette, &profiles[index]);
                } else {
                    paint_road_segment(&painter, rect, self.map_pan, self.map_zoom,
                        segment, road, selected, palette);
                }
                let midpoint = screen_of(rect, self.map_pan, self.map_zoom, segment.point_at(0.5));
                painter.text(
                    midpoint + egui::vec2(0.0, -22.0),
                    egui::Align2::CENTER_BOTTOM,
                    segment.name.as_deref().unwrap_or(&segment.road_name),
                    egui::FontId::proportional(12.0),
                    palette.map_text,
                );
                if selected {
                    for point in [segment.start, segment.end]
                        .map(|point| screen_of(rect, self.map_pan, self.map_zoom, point))
                    {
                        painter.circle_filled(point, 7.0, palette.map_text);
                        painter.circle_stroke(
                            point,
                            7.0,
                            egui::Stroke::new(2.0, palette.map_selected),
                        );
                    }
                    {
                        for (anchor, control) in [
                            (segment.start, segment.controls[0]),
                            (segment.end, segment.controls[1]),
                        ] {
                            let a = screen_of(rect, self.map_pan, self.map_zoom, anchor);
                            let b = screen_of(rect, self.map_pan, self.map_zoom, control);
                            painter
                                .line_segment([a, b], egui::Stroke::new(1.0, palette.map_selected));
                            painter.circle_filled(b, 5.0, palette.map_selected);
                        }
                    }
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
                        }
                    }
                }
            }
            if self.map_tool == MapTool::Draw {
                if let Some(pointer) = ui
                    .ctx()
                    .pointer_hover_pos()
                    .filter(|point| rect.contains(*point))
                {
                    let element_ends = element_end_candidates(&projection, &self.program);
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
                        let screen =
                            screen_of(rect, self.map_pan, self.map_zoom, candidate.position);
                        painter.text(
                            screen + egui::vec2(11.0, -12.0),
                            egui::Align2::LEFT_BOTTOM,
                            format!(
                                "{} {} end",
                                kind_label(candidate.kind),
                                candidate.element_index + 1
                            ),
                            egui::FontId::proportional(11.0),
                            palette.map_text,
                        );
                    }
                    for (point, reference) in junction_candidates(&projection) {
                        let screen = screen_of(rect, self.map_pan, self.map_zoom, point);
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
                    let end =
                        element_end_near(&element_ends, rect, self.map_pan, self.map_zoom, pointer)
                            .map(|candidate| candidate.position)
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
                    "{} segments  |  {} road types",
                    projection.segments.len(),
                    self.program
                        .statements
                        .iter()
                        .filter(|statement| matches!(statement, Statement::Road(_)))
                        .count()
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

fn snap_point(point: WorldPoint) -> WorldPoint {
    WorldPoint {
        x: (point.x * 4.0).round() / 4.0,
        y: (point.y * 4.0).round() / 4.0,
    }
}

fn junction_candidates(projection: &map::Projection) -> Vec<(WorldPoint, Option<String>)> {
    let mut candidates: Vec<_> = projection
        .points
        .iter()
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
    let meters = match kind {
        RoadElementKind::CarLane | RoadElementKind::BusLane => 0.045,
        RoadElementKind::ParkingLane | RoadElementKind::Sidewalk | RoadElementKind::BikeLane => {
            0.032
        }
        RoadElementKind::BusStop | RoadElementKind::Shoulder => 0.028,
        RoadElementKind::Middle => 0.025,
        RoadElementKind::Verge => 0.035,
        RoadElementKind::BuildingEdge => 0.012,
    };
    (meters * zoom).max(1.5)
}

#[derive(Clone, Copy)]
struct ThroughEffect {
    element_index: usize,
    left_side: bool,
    split_t: f32,
    depth: usize,
}

#[derive(Clone, Copy)]
struct RampEffect {
    mode: map::SplitMode,
    element_index: usize,
}

#[derive(Default)]
struct PaintProfile {
    through: Vec<ThroughEffect>,
    ramp: Option<RampEffect>,
}

fn paint_profiles(program: &Program, projection: &map::Projection) -> Vec<PaintProfile> {
    let mut profiles: Vec<_> = projection
        .segments
        .iter()
        .map(|_| PaintProfile::default())
        .collect();
    for split in map::lane_splits(program) {
        let Some((parent_index, parent)) = projection
            .segments
            .iter()
            .enumerate()
            .find(|(_, segment)| segment.name.as_deref() == Some(&split.parent))
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
        let Some(parent_road) = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == parent.road_name => Some(road),
                _ => None,
            })
        else {
            continue;
        };
        let Some(ramp_road) = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == ramp.road_name => Some(road),
                _ => None,
            })
        else {
            continue;
        };
        if split.element_index >= parent_road.elements.len() {
            continue;
        }
        let selected = ramp_road
            .elements
            .iter()
            .position(|element| {
                matches!(
                    element.kind,
                    RoadElementKind::CarLane | RoadElementKind::BusLane | RoadElementKind::BikeLane
                )
            })
            .unwrap_or(0);
        profiles[ramp_index].ramp = Some(RampEffect {
            mode: split.mode,
            element_index: selected,
        });
        if split.mode == map::SplitMode::KeepLane {
            continue;
        }
        let effect = ThroughEffect {
            element_index: split.element_index,
            left_side: split.element_index < parent_road.elements.len() / 2,
            split_t: split.t,
            depth: 0,
        };
        profiles[parent_index].through.push(effect);
        let mut current = parent_index;
        for depth in 1..projection.segments.len() {
            let previous = &projection.segments[current];
            let next = projection
                .segments
                .iter()
                .enumerate()
                .find(|(index, candidate)| {
                    *index != current
                        && *index != ramp_index
                        && candidate.road_name == parent.road_name
                        && (previous.end_ref.as_ref().is_some_and(|reference| {
                            candidate.start_ref.as_ref() == Some(reference)
                        }) || candidate.start == previous.end)
                })
                .map(|(index, _)| index);
            let Some(next) = next else { break };
            if profiles[next]
                .through
                .iter()
                .any(|existing| existing.element_index == effect.element_index)
            {
                break;
            }
            profiles[next]
                .through
                .push(ThroughEffect { depth, ..effect });
            current = next;
        }
    }
    profiles
}

fn smooth_fraction(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
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
    let original: Vec<_> = road
        .elements
        .iter()
        .map(|element| map_element_width(element.kind, zoom))
        .collect();
    let mut widths = original.clone();
    let mut left_shift = 0.0;
    for effect in &profile.through {
        let progress = match effect.depth {
            0 => (t - effect.split_t) / 0.25,
            1 => (1.0 - effect.split_t + t) / 0.25,
            _ => 1.0,
        };
        if let Some(width) = widths.get_mut(effect.element_index) {
            let removed = *width * smooth_fraction(progress);
            *width -= removed;
            if effect.left_side {
                left_shift += removed;
            }
        }
    }
    if let Some(ramp) = profile.ramp {
        let growth = smooth_fraction(t / 0.25);
        for (index, width) in widths.iter_mut().enumerate() {
            if index != ramp.element_index || ramp.mode == map::SplitMode::KeepLane {
                *width *= growth;
            }
        }
        let active: f32 = widths.iter().sum();
        return (widths, -active / 2.0);
    }
    let total: f32 = original.iter().sum();
    (widths, -total / 2.0 + left_shift)
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
        egui::Stroke::new(0.8, palette.map_centerline),
    ));
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
        egui::Stroke::new(0.8, palette.map_centerline),
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

fn kind_color(kind: RoadElementKind, theme: egui::Theme) -> Color32 {
    match theme {
        egui::Theme::Dark => match kind {
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
            RoadElementKind::Shoulder => Color32::from_rgb(182, 196, 195),
            RoadElementKind::Verge => Color32::from_rgb(158, 200, 160),
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
            map_start: None,
            map_start_ref: None,
            branch_element: usize::MAX,
            branch_t: 0.75,
            branch_road_name: "exit_ramp".into(),
            branch_mode: map::SplitMode::Dedicated,
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
    fn dedicated_exit_removes_through_lane_and_keep_lane_preserves_it() {
        for mode in [map::SplitMode::Dedicated, map::SplitMode::KeepLane] {
            let mut program = parse("road highway: verge-carlane-middle-carlane-verge; end; road ramp: shoulder-carlane-shoulder; end;").unwrap();
            let parent_road = match &program.statements[0] {
                Statement::Road(road) => road.clone(),
                _ => unreachable!(),
            };
            let ramp_road = match &program.statements[1] {
                Statement::Road(road) => road.clone(),
                _ => unreachable!(),
            };
            let (parent, through_junction) = map::append_connected(
                &mut program,
                &parent_road,
                WorldPoint { x: 0.0, y: 0.0 },
                None,
                WorldPoint { x: 4.0, y: 0.0 },
                None,
            );
            let (attachment, position) =
                map::create_lane_point(&mut program, SegmentSource::TopLevel(parent), 3, 1.0)
                    .unwrap();
            let (ramp, _) = map::append_connected(
                &mut program,
                &ramp_road,
                position,
                Some(&attachment),
                WorldPoint { x: 6.0, y: -2.0 },
                None,
            );
            assert_eq!(
                map::lane_splits(&program)[0].mode,
                map::SplitMode::Dedicated
            );
            assert!(map::create_lane_split(
                &mut program,
                &attachment,
                SegmentSource::TopLevel(ramp),
                mode
            ));
            let (through, _) = map::append_connected(
                &mut program,
                &parent_road,
                WorldPoint { x: 4.0, y: 0.0 },
                Some(&through_junction),
                WorldPoint { x: 8.0, y: 0.0 },
                None,
            );
            let projection = map::project(&program);
            let profiles = paint_profiles(&program, &projection);
            let through_index = projection
                .segments
                .iter()
                .position(|segment| segment.source == SegmentSource::TopLevel(through))
                .unwrap();
            let ramp_index = projection
                .segments
                .iter()
                .position(|segment| segment.source == SegmentSource::TopLevel(ramp))
                .unwrap();
            let through_widths =
                section_widths(&parent_road, 100.0, &profiles[through_index], 0.5).0;
            let ramp_start = section_widths(&ramp_road, 100.0, &profiles[ramp_index], 0.0).0;
            assert!(ramp_start[0] < 0.001 && ramp_start[2] < 0.001);
            if mode == map::SplitMode::Dedicated {
                assert!(through_widths[3] < 0.001);
                assert!(ramp_start[1] > 4.0);
            } else {
                assert!(through_widths[3] > 4.0);
                assert!(ramp_start[1] < 0.001);
            }
            assert_eq!(map::lane_splits(&program).len(), 1);
            assert_eq!(parse(&program.to_road_source()).unwrap(), program);
            assert!(map::remove(&mut program, SegmentSource::TopLevel(ramp)));
            assert!(map::lane_splits(&program).is_empty());
        }
    }

    #[test]
    fn older_lane_branch_can_switch_to_keep_lane_mode() {
        let mut program = parse(
            "road highway: verge-carlane-verge; end; road ramp: shoulder-carlane-shoulder; end;",
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
