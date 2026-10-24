//! The app surface: a 3D canvas and one small road toolbar.
use super::*;

#[derive(Default)]
pub(super) struct MinimalEditor {
    pub(super) placement_error: Option<String>,
    pub(super) intersection_rotation: u8,
    pub(super) pending_road: Option<RoadDefinition>,
    pub(super) clear_just_opened: bool,
    pub(super) vehicle_initialized: bool,
    pub(super) last_edit_view: Option<CameraView>,
    pub(super) input_suspended: bool,
    pub(super) palette_rename_focus: bool,
    pub(super) palette_pointer_gesture: bool,
    pub(super) annotation_start: Option<WorldPoint>,
    pub(super) annotation_text: String,
    pub(super) view_switch_rect: Option<egui::Rect>,
    pub(super) view_switch_gesture: bool,
    pub(super) toolbar_rect: Option<egui::Rect>,
    pub(super) rename_source: Option<SegmentSource>,
    original_name: String,
    draft: String,
    error: Option<String>,
}

impl MinimalEditor {
    pub(super) fn pointer_over_toolbar(&self, ctx: &egui::Context) -> bool {
        ctx.pointer_hover_pos()
            .is_some_and(|p| self.toolbar_rect.is_some_and(|r| r.contains(p)))
    }
}

impl RoadEditor {
    /// Only the pill owns pointer input; the rest of the reveal strip is canvas.
    pub(super) fn view_switch_ui(&mut self, root: &mut egui::Ui) -> bool {
        let ctx = root.ctx().clone();
        let bounds = root.max_rect();
        let reveal = ctx
            .pointer_hover_pos()
            .is_some_and(|p| bounds.contains(p) && p.y < bounds.top() + 100.0);
        self.minimal.view_switch_rect = None;
        if !reveal && !self.minimal.view_switch_gesture {
            return false;
        }
        let choices = [
            (CameraView::Top, "Top"),
            (CameraView::Isometric, "Isometric"),
            (CameraView::Vehicular, "Vehicular"),
        ];
        let mut switch_style = moon_toggle_switch::ToggleSwitchStyle::of(root);
        switch_style.padding = egui::vec2(10.0, 5.0);
        let size = moon_toggle_switch::ToggleSwitch::new(&choices, self.camera_view)
            .style(switch_style.clone())
            .size(root);
        let pos = egui::pos2(
            (bounds.right() - size.x - 12.0).max(bounds.left()),
            bounds.top() + 12.0,
        );
        let rect = egui::Rect::from_min_size(pos, size);
        self.minimal.view_switch_rect = Some(rect);
        let starts_here = ctx.input(|i| {
            i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|p| rect.contains(p))
        });
        self.minimal.view_switch_gesture |= starts_here;
        let owns_pointer = self.minimal.view_switch_gesture;
        // A canvas drag passing over the pill keeps its original owner.
        let enabled =
            self.pending.is_none() && (!ctx.input(|i| i.pointer.any_down()) || owns_pointer);
        let mut chosen = None;
        egui::Area::new(egui::Id::new("minimal-view-switch"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(&ctx, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    ui.allocate_exact_size(size, egui::Sense::hover());
                    chosen = moon_toggle_switch::ToggleSwitch::new(&choices, self.camera_view)
                        .style(switch_style.clone())
                        .show_in(ui, rect);
                });
            });
        if let Some(view) = chosen {
            self.set_camera_view(view);
        }
        if !ctx.input(|i| i.pointer.any_down()) {
            self.minimal.view_switch_gesture = false;
        }
        owns_pointer
    }

    pub(super) fn box_select_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        projection: &map::Projection,
        iso: bool,
    ) -> bool {
        if self.map_tool != MapTool::BoxSelect {
            return false;
        }
        if self.minimal.input_suspended {
            return true;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.box_start = None;
            self.map_tool = MapTool::Select;
            return true;
        }
        if response.drag_started_by(egui::PointerButton::Primary) {
            self.box_start = ui
                .input(|i| i.pointer.press_origin())
                .filter(|p| rect.contains(*p));
        }
        if let (Some(start), Some(pointer)) = (self.box_start, response.interact_pointer_pos()) {
            let selection = egui::Rect::from_two_pos(start, pointer);
            ui.painter_at(rect).rect_stroke(
                selection,
                0,
                egui::Stroke::new(1.0, ui.visuals().selection.stroke.color),
                egui::StrokeKind::Inside,
            );
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                self.selected_segments = projection
                    .segments
                    .iter()
                    .filter(|segment| {
                        let path: Vec<_> = (0..=64)
                            .map(|step| {
                                let t = step as f32 / 64.0;
                                self.spatial_screen(
                                    rect,
                                    segment.point_at(t),
                                    segment.height_at(t),
                                    iso,
                                )
                            })
                            .collect();
                        path.windows(2)
                            .any(|pair| line_crosses_rect(pair[0], pair[1], selection))
                    })
                    .map(|segment| segment.source)
                    .collect();
                self.selected_segment = self.selected_segments.last().copied();
                self.box_start = None;
                self.map_tool = MapTool::Select;
                self.status = format!("Selected {} segment(s)", self.selected_segments.len());
            }
        }
        true
    }

    fn commit_placed_road_name(&mut self) {
        let Some(source) = self.minimal.rename_source else {
            return;
        };
        let current_name = map::project(&self.program)
            .segments
            .into_iter()
            .find(|road| road.source == source)
            .and_then(|road| road.name)
            .unwrap_or_default();
        if current_name != self.minimal.original_name {
            self.minimal.error = Some("This road changed. Select it again before renaming.".into());
            return;
        }
        if self.minimal.draft == self.minimal.original_name {
            return;
        }
        if !Self::valid_name(&self.minimal.draft) {
            self.minimal.error = Some(
                "Use letters, digits and underscores; start with a letter or underscore.".into(),
            );
            return;
        }
        if !map::set_name(&mut self.program, source, self.minimal.draft.clone()) {
            self.minimal.error = Some("This name is already in use.".into());
            return;
        }
        self.minimal.original_name = self.minimal.draft.clone();
        self.segment_name_draft = self.minimal.draft.clone();
        self.minimal.error = None;
        self.dirty = true;
    }

    pub(super) fn minimal_editor_ui(&mut self, root: &mut egui::Ui, palette_active: bool) {
        self.minimal.input_suspended = palette_active;
        if palette_active
            || self.pending.is_some()
            || root.input(|i| i.key_pressed(egui::Key::Escape))
        {
            self.point_options = None;
        }
        if self.view == View::Section {
            self.section_editor_ui(root, palette_active);
            return;
        }
        let ctx = root.ctx().clone();
        let rename_id = egui::Id::new("placed-road-name");
        let editing = ctx.memory(|m| m.has_focus(rename_id));
        let escape = self.map_tool == MapTool::Select
            && self.minimal.rename_source.is_some()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let enter =
            editing && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let selected = self
            .projected_map()
            .segments
            .iter()
            .find(|s| Some(s.source) == self.selected_segment)
            .cloned();
        // Retain the explicit edit target until blur commits it. A selection
        // change must never redirect a pending rename to another road.
        if !editing && !palette_active {
            if let Some(segment) = selected.as_ref().filter(|s| s.source.editable()) {
                let name = segment.name.clone().unwrap_or_default();
                if self.minimal.rename_source != Some(segment.source)
                    || self.minimal.original_name != name
                {
                    self.minimal.rename_source = Some(segment.source);
                    self.minimal.original_name = name.clone();
                    self.minimal.draft = name;
                    self.minimal.error = None;
                }
            } else {
                self.minimal.rename_source = None;
                self.minimal.error = None;
            }
        }
        self.minimal.toolbar_rect = None;
        if self.map_tool == MapTool::Select && self.minimal.rename_source.is_some() {
            let area = egui::Area::new(egui::Id::new("minimal-road-toolbar"))
                .order(egui::Order::Middle)
                .fixed_pos(root.max_rect().min + egui::vec2(12.0, 12.0))
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.add_enabled_ui(!palette_active, |ui| {
                            ui.horizontal(|ui| {
                                if self.map_tool == MapTool::Select
                                    && self.minimal.rename_source.is_some()
                                {
                                    let response = ui.add(
                                        egui::TextEdit::singleline(&mut self.minimal.draft)
                                            .id(rename_id)
                                            .desired_width(180.0)
                                            .hint_text("Road name"),
                                    );
                                    if escape {
                                        self.minimal.draft = self.minimal.original_name.clone();
                                        self.minimal.error = None;
                                        response.surrender_focus();
                                    } else if !palette_active && (enter || response.lost_focus()) {
                                        self.commit_placed_road_name();
                                        if enter {
                                            response.surrender_focus();
                                        }
                                    }
                                    if let Some(error) = &self.minimal.error {
                                        response.on_hover_text(error);
                                    }
                                } else if selected.is_some() {
                                    ui.label("Read-only road").on_hover_text(
                                        "Roads inside block instances cannot be renamed here.",
                                    );
                                }
                            });
                        });
                        if let Some(error) = &self.minimal.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                    });
                });
            self.minimal.toolbar_rect = Some(area.response.rect);
        }
        self.map_rect = None;
        self.map_canvas(root);
    }
}

#[cfg(test)]
#[path = "minimal/tests/mod.rs"]
mod tests;
