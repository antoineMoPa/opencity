//! Document for the city editor.
use super::*;

impl RoadEditor {
    pub(super) fn open_path(&mut self, path: PathBuf) {
        match parse_road_file(&path) {
            Ok(program) => {
                self.load_program(program, Some(path.clone()));
                self.status = format!("Opened {}", path.display());
            }
            Err(error) => self.status = format!("Could not open {}: {error}", path.display()),
        }
    }

    pub(super) fn load_program(&mut self, program: Program, path: Option<PathBuf>) {
        self.camera_transition = None;
        self.map_placement_before = None;
        self.map_placement_draft = None;
        self.map_start_height = None;
        self.map_tool = MapTool::Select;
        self.landscape_points.clear();
        self.minimal = minimal::MinimalEditor::default();
        self.selected_road = program
            .statements
            .iter()
            .position(|statement| matches!(statement, Statement::Road(_)));
        self.selected_element = None;
        self.selected_segment = None;
        self.selected_segments.clear();
        self.box_start = None;
        self.iso_drag_pan = false;
        self.iso_drag_orbit = false;
        self.selected_intersection = None;
        self.selected_endpoint = None;
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
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.connection_member = None;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.connection_id = None;
        }
        self.program = program;
        self.saved_program = self.program.clone();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.skip_history = true;
        self.fit_scene_once = true;
        self.camera_view = CameraView::Isometric;
        self.map_pan = egui::Vec2::ZERO;
        self.map_zoom = 55.0;
        self.path = path;
        self.scene_cache = None;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.scene_build = None;
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.web_scene_build = None;
        }
        self.dirty = false;
        self.confirm_delete_road = false;
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn save_to(&mut self, path: PathBuf) {
        if let Err(error) = parse(&self.program.to_road_source()) {
            self.status = format!("Fix the document before saving: {error}");
            return;
        }
        match self.program.write_road_file(&path) {
            Ok(()) => {
                self.path = Some(path.clone());
                self.scene_cache = None;
                self.scene_build = None;
                self.saved_program = self.program.clone();
                self.dirty = false;
                self.status = format!("Saved {}", path.display());
            }
            Err(error) => self.status = format!("Could not save {}: {error}", path.display()),
        }
    }

    pub(super) fn file_action(&mut self, action: FileAction) {
        if matches!(self.pending, Some(PendingAction::New)) {
            return;
        }
        match action {
            FileAction::New => {
                if !matches!(self.pending, Some(PendingAction::New)) {
                    self.command_palette.dismiss();
                    self.minimal.clear_just_opened = true;
                    self.pending = Some(PendingAction::New);
                }
            }
            FileAction::Open if self.dirty => self.pending = Some(PendingAction::Open),
            FileAction::Open => self.pick_open(),
            FileAction::Save => {
                #[cfg(target_arch = "wasm32")]
                self.pick_save();
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(path) = self.path.clone() {
                    self.save_to(path);
                } else {
                    self.pick_save();
                }
            }
            FileAction::SaveAs => self.pick_save(),
        }
    }

    pub(super) fn new_document(&mut self) {
        self.camera_transition = None;
        self.map_placement_before = None;
        self.map_placement_draft = None;
        self.map_start_height = None;
        self.landscape_points.clear();
        self.minimal = minimal::MinimalEditor::default();
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.connection_member = None;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.connection_id = None;
        }
        self.program = default_program();
        self.saved_program = self.program.clone();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.skip_history = true;
        self.path = None;
        self.selected_road = Some(0);
        self.selected_element = None;
        self.selected_segment = None;
        self.selected_segments.clear();
        self.box_start = None;
        self.iso_drag_pan = false;
        self.iso_drag_orbit = false;
        self.selected_intersection = None;
        self.selected_endpoint = None;
        self.segment_name_draft.clear();
        self.road_name_draft = "neighborhood_street".into();
        self.view = View::Map;
        self.map_tool = MapTool::Select;
        self.camera_view = CameraView::Isometric;
        self.map_start = None;
        self.map_start_ref = None;
        self.branch_mode = map::SplitMode::Dedicated;
        self.map_pan = egui::Vec2::ZERO;
        self.map_zoom = 55.0;
        self.fit_scene_once = false;
        self.dragging_endpoint = None;
        self.dragging_control = None;
        self.dragging_intersection = None;
        self.dragging_background = false;
        self.dirty = false;
        self.confirm_delete_road = false;
        self.status = "New blank map".into();
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn pick_open(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("OpenCity roads", &["road"])
            .pick_file()
        {
            self.open_path(path);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn pick_save(&mut self) {
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

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn export_glb(&mut self) {
        let bytes = glb::export(&self.program);
        if bytes.is_empty() {
            self.status = "Nothing to export: place a road or landscape feature first".into();
            return;
        }
        let mut dialog = rfd::FileDialog::new()
            .add_filter("glTF binary", &["glb"])
            .set_file_name("opencity.glb");
        if let Some(parent) = self.path.as_ref().and_then(|path| path.parent()) {
            dialog = dialog.set_directory(parent);
        }
        if let Some(path) = dialog.save_file() {
            match std::fs::write(&path, bytes) {
                Ok(()) => self.status = format!("Exported {}", path.display()),
                Err(error) => self.status = format!("GLB export failed: {error}"),
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn export_svg(&mut self) {
        let document = svg::export(&self.program);
        if document.is_empty() {
            self.status = "Nothing to export: place a road or landscape feature first".into();
            return;
        }
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Scalable Vector Graphics", &["svg"])
            .set_file_name("opencity-map.svg");
        if let Some(parent) = self.path.as_ref().and_then(|path| path.parent()) {
            dialog = dialog.set_directory(parent);
        }
        if let Some(path) = dialog.save_file() {
            match std::fs::write(&path, document) {
                Ok(()) => self.status = format!("Exported {}", path.display()),
                Err(error) => self.status = format!("SVG export failed: {error}"),
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn pick_open(&mut self) {
        let sender = self.web_event_sender.clone();
        let ctx = self.web_ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(file) = rfd::AsyncFileDialog::new()
                .add_filter("OpenCity roads", &["road"])
                .pick_file()
                .await
            {
                let name = file.file_name();
                let event = match String::from_utf8(file.read().await) {
                    Ok(source) => WebFileEvent::Opened { name, source },
                    Err(error) => WebFileEvent::Message(format!("Could not read {name}: {error}")),
                };
                let _ = sender.send(event);
                ctx.request_repaint();
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn pick_save(&mut self) {
        if let Err(error) = parse(&self.program.to_road_source()) {
            self.status = format!("Fix the document before saving: {error}");
            return;
        }
        let name = self
            .path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("roads.road")
            .to_owned();
        self.save_web(
            &name,
            self.program.to_road_source().into_bytes(),
            Some(self.program.clone()),
        );
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn export_glb(&mut self) {
        let bytes = glb::export(&self.program);
        if bytes.is_empty() {
            self.status = "Nothing to export: place a road or landscape feature first".into();
            return;
        }
        self.save_web("opencity.glb", bytes, None);
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn export_svg(&mut self) {
        let document = svg::export(&self.program);
        if document.is_empty() {
            self.status = "Nothing to export: place a road or landscape feature first".into();
            return;
        }
        self.save_web("opencity-map.svg", document.into_bytes(), None);
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn save_web(&mut self, name: &str, bytes: Vec<u8>, program: Option<Program>) {
        let sender = self.web_event_sender.clone();
        let ctx = self.web_ctx.clone();
        let name = name.to_owned();
        wasm_bindgen_futures::spawn_local(async move {
            let dialog = rfd::AsyncFileDialog::new().set_file_name(&name);
            if let Some(file) = dialog.save_file().await {
                let file_name = file.file_name();
                let event = match file.write(&bytes).await {
                    Ok(()) => match program {
                        Some(program) => WebFileEvent::Saved {
                            name: file_name,
                            program,
                        },
                        None => WebFileEvent::Message(format!("Exported {file_name}")),
                    },
                    Err(error) => {
                        WebFileEvent::Message(format!("Could not save {file_name}: {error}"))
                    }
                };
                let _ = sender.send(event);
                ctx.request_repaint();
            }
        });
    }
}
