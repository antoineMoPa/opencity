//! Commands for the city editor.
use super::*;

impl RoadEditor {
    pub(super) fn palette_commands(&self) -> Vec<PaletteCommand> {
        let mut commands: Vec<_> = [
            (
                "road:neighborhood_street",
                "Add road",
                "Neighborhood street · place with two clicks",
                "",
            ),
            (
                "road:divided_highway",
                "Add highway",
                "Divided highway · place with two clicks",
                "",
            ),
            (
                "road:interstate_three_lane",
                "Add interstate",
                "Interstate three lane · place with two clicks",
                "",
            ),
            ("landscape:terrain", "Add terrain", "Place terrain", ""),
            ("landscape:concrete", "Add concrete", "Place concrete", ""),
            (
                "intersection:intersection3",
                "Add three-way intersection",
                "Tee junction · click to place · R rotate · Esc cancel",
                "",
            ),
            (
                "intersection:intersection4",
                "Add four-way intersection",
                "Cross junction · click to place · R rotate · Esc cancel",
                "",
            ),
            (
                "intersection:intersection2",
                "Add right-turn intersection",
                "Corner junction · click to place · R rotate · Esc cancel",
                "",
            ),
            (
                "edit-section",
                "Edit cross section",
                "Edit the selected road profile",
                "",
            ),
            ("map", "Back to map", "Return to the map viewport", ""),
            ("open", "Open…", "Open a road file", "⌘O"),
            ("save", "Save", "Save the current city", "⌘S"),
            ("save-as", "Save As…", "Save to another road file", "⇧⌘S"),
            ("export-glb", "Export GLB", "Export GLB", ""),
            ("export-svg", "Export SVG", "Export SVG", ""),
            ("undo", "Undo", "Undo the last edit", "⌘Z"),
            ("redo", "Redo", "Redo the last edit", "⇧⌘Z"),
            ("copy", "Copy", "Copy selected road placements", "⌘C"),
            ("paste", "Paste", "Paste road placements", "⌘V"),
            ("select", "Select tool", "Select a road or feature", ""),
            (
                "box-select",
                "Box select",
                "Drag to select roads in Top or Isometric view",
                "",
            ),
            (
                "rename",
                "Rename selected road",
                "Edit the selected road name",
                "",
            ),
            (
                "theme",
                "Toggle light and dark theme",
                "Switch the workspace theme",
                "⌘J",
            ),
        ]
        .into_iter()
        .map(|(id, title, description, shortcut)| PaletteCommand {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            shortcut: shortcut.into(),
        })
        .collect();
        for statement in &self.program.statements {
            if let Statement::Road(road) = statement {
                if Self::canonical_road_names().contains(&road.name.as_str()) {
                    continue;
                }
                commands.push(PaletteCommand {
                    id: format!("road:{}", road.name),
                    title: format!("Add road: {}", preset_label(&road.name)),
                    description: "Place a road with two clicks".into(),
                    shortcut: String::new(),
                });
            }
        }
        for kind in Self::placement_landscapes() {
            if matches!(kind, LandscapeKind::Terrain | LandscapeKind::Concrete) {
                continue;
            }
            commands.push(PaletteCommand {
                id: format!("landscape:{}", kind.call_name()),
                title: format!("Add {}", kind.call_name()),
                description: landscape_label(kind).into(),
                shortcut: String::new(),
            });
        }
        for (id, title) in [
            ("annotation-segment", "Annotation segment"),
            ("annotation-text", "Annotation text"),
        ] {
            commands.push(PaletteCommand {
                id: id.into(),
                title: title.into(),
                description: "Place a red map annotation".into(),
                shortcut: String::new(),
            });
        }
        commands.push(PaletteCommand {
            id: "clear".into(),
            title: "Clear".into(),
            description: "Remove all placements; keep road and block definitions".into(),
            shortcut: "⌘N".into(),
        });
        commands
    }

    pub(super) fn canonical_road_names() -> [&'static str; 3] {
        [
            "neighborhood_street",
            "divided_highway",
            "interstate_three_lane",
        ]
    }

    /// Materialize a palette preset only when its first placement commits.
    pub(super) fn ensure_placement_road_type(&mut self) -> Option<String> {
        if let Some(road) = self.minimal.pending_road.take() {
            let index = self
                .program
                .statements
                .iter()
                .position(|s| matches!(s, Statement::Road(r) if r.name == road.name));
            self.selected_road = Some(index.unwrap_or_else(|| {
                let index = self.program.statements.len();
                self.program.statements.push(Statement::Road(road));
                index
            }));
        }
        self.road().map(|road| road.name.clone())
    }

    pub(super) fn placement_landscapes() -> [LandscapeKind; 8] {
        [
            LandscapeKind::Terrain,
            LandscapeKind::Concrete,
            LandscapeKind::Land,
            LandscapeKind::Beach,
            LandscapeKind::Lake,
            LandscapeKind::River,
            LandscapeKind::Vegetation,
            LandscapeKind::Building,
        ]
    }

    pub(super) fn execute_palette_command(&mut self, id: &str, ctx: &egui::Context) {
        if self.camera_view == CameraView::Vehicular
            && (id.starts_with("intersection:")
                || id.starts_with("road:")
                || id.starts_with("landscape:")
                || id.starts_with("annotation-")
                || id == "box-select")
        {
            self.set_camera_view(self.minimal.last_edit_view.unwrap_or(CameraView::Isometric));
            self.status = "Opened editing view; switch back to Vehicular in the palette".into();
        }
        if id.starts_with("intersection:")
            || id.starts_with("road:")
            || id.starts_with("landscape:")
            || id.starts_with("annotation-")
            || id == "select"
        {
            ctx.memory_mut(|m| {
                if let Some(focused) = m.focused() {
                    m.surrender_focus(focused);
                }
            });
        }

        if id.starts_with("intersection:")
            || id.starts_with("road:")
            || id.starts_with("landscape:")
            || id.starts_with("annotation-")
            || matches!(id, "select" | "box-select" | "rename")
        {
            self.view = View::Map;
        }
        if let Some(kind) = id
            .strip_prefix("intersection:")
            .and_then(map::IntersectionKind::from_call_name)
        {
            self.start_intersection_placement(kind);
            return;
        }
        if let Some(name) = id.strip_prefix("road:") {
            if let Some(index) = self
                .program
                .statements
                .iter()
                .position(|s| matches!(s, Statement::Road(r) if r.name == name))
            {
                self.cancel_placement();
                self.selected_road = Some(index);
                self.map_tool = MapTool::Draw;
            } else if Self::canonical_road_names().contains(&name) {
                let presets = parse(include_str!("../../../examples/default_roads.road"))
                    .expect("bundled road presets");
                if let Some(road) = presets.statements.into_iter().find_map(|s| match s {
                    Statement::Road(r) if r.name == name => Some(r),
                    _ => None,
                }) {
                    self.cancel_placement();
                    self.minimal.pending_road = Some(road);
                    self.map_tool = MapTool::Draw;
                }
            }
            return;
        }
        if let Some(name) = id.strip_prefix("landscape:") {
            if let Some(kind) = Self::placement_landscapes()
                .into_iter()
                .find(|kind| kind.call_name() == name)
            {
                self.cancel_placement();
                self.start_landscape(kind);
            }
            return;
        }
        if matches!(id, "annotation-segment" | "annotation-text") {
            self.cancel_placement();
            self.map_tool = if id == "annotation-segment" {
                MapTool::AnnotationSegment
            } else {
                MapTool::AnnotationText
            };
            return;
        }
        match id {
            "edit-section" => self.enter_section_editor(),
            "map" => self.view = View::Map,
            "new" | "clear" => self.file_action(FileAction::New),
            "open" => self.file_action(FileAction::Open),
            "save" => self.file_action(FileAction::Save),
            "save-as" => self.file_action(FileAction::SaveAs),
            "export-glb" => self.export_glb(),
            "export-svg" => self.export_svg(),
            "undo" => {
                self.restore_history(true);
            }
            "redo" => {
                self.restore_history(false);
            }
            "copy"
                if self.view == View::Map
                    && (self.selected_segment.is_some() || !self.selected_segments.is_empty()) =>
            {
                self.copy_selection()
            }
            "paste" if self.view == View::Map && !self.clipboard.is_empty() => {
                self.paste_selection()
            }
            "select" => {
                self.cancel_placement();
                self.map_start = None;
                self.map_start_ref = None;
            }
            "box-select" => {
                self.cancel_placement();
                self.map_tool = MapTool::BoxSelect;
                self.status = "Drag a box to select roads; Escape cancels".into();
            }
            "draw" if self.road().is_some() => self.map_tool = MapTool::Draw,
            "rename" => {
                self.map_tool = MapTool::Select;
                ctx.memory_mut(|m| m.request_focus(egui::Id::new("placed-road-name")));
            }
            "theme" => toggle_theme(ctx),
            _ => {}
        }
    }
}
pub(super) fn preset_label(name: &str) -> &str {
    match name {
        "neighborhood_street" => "Neighborhood street",
        "main_street" => "Main street",
        "downtown_avenue" => "Downtown avenue",
        "protected_bikeway" => "Protected bikeway",
        "dedicated_bike_path" => "Dedicated bike path",
        "train_line" => "Train line",
        "tram_line" => "Tram line",
        "subway_line" => "Subway line",
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
        "airport_runway" => "Airport runway",
        "airport_taxiway" => "Airport taxiway",
        "airport_apron" => "Airport apron",
        _ => name,
    }
}
