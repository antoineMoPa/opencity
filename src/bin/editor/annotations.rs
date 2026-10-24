//! Nonphysical, world-anchored red map markup, stored as ordinary source calls.
use super::*;
const RED: egui::Color32 = egui::Color32::from_rgb(240, 35, 45);

enum Annotation {
    Segment(WorldPoint, WorldPoint),
    Text(String, WorldPoint),
}
fn annotations(program: &Program) -> Vec<Annotation> {
    program
        .statements
        .iter()
        .filter_map(|statement| {
            let Statement::Call(call) = statement else {
                return None;
            };
            let number = |index: usize| match call.args.get(index)? {
                Argument::Number(value) => value.parse::<f32>().ok().filter(|v| v.is_finite()),
                _ => None,
            };
            match call.name.as_str() {
                "annotation_segment" if call.args.len() == 4 => Some(Annotation::Segment(
                    WorldPoint {
                        x: number(0)?,
                        y: number(1)?,
                    },
                    WorldPoint {
                        x: number(2)?,
                        y: number(3)?,
                    },
                )),
                "annotation_text" if call.args.len() == 3 => {
                    let Argument::String(token) = &call.args[0] else {
                        return None;
                    };
                    Some(Annotation::Text(
                        serde_json::from_str(token).ok()?,
                        WorldPoint {
                            x: number(1)?,
                            y: number(2)?,
                        },
                    ))
                }
                _ => None,
            }
        })
        .collect()
}
impl RoadEditor {
    pub(super) fn annotation_screen(
        &self,
        rect: egui::Rect,
        point: WorldPoint,
    ) -> Option<egui::Pos2> {
        match self.camera_view {
            CameraView::Top => Some(screen_of(rect, self.map_pan, self.map_zoom, point)),
            CameraView::Isometric => Some(self.spatial_screen(rect, point, 0.0, true)),
            CameraView::Vehicular => {
                let dx = point.x - self.vehicle_position.x;
                let dy = point.y - self.vehicle_position.y;
                let forward = dx * self.vehicle_heading.cos() + dy * self.vehicle_heading.sin();
                let right = -dx * self.vehicle_heading.sin() + dy * self.vehicle_heading.cos();
                (forward >= 0.04).then(|| {
                    rect.center()
                        + egui::vec2(right, self.vehicle_eye_height)
                            * (rect.height() * 0.714 / forward)
                })
            }
        }
    }
    pub(super) fn cancel_placement(&mut self) {
        self.minimal.pending_road = None;
        self.minimal.placement_error = None;
        self.rollback_road_placement();
        if let Some(reference) = self.map_start_ref.as_deref() {
            if map::anchored_reference(&self.program, reference) {
                map::remove_unused_point(&mut self.program, reference);
            }
        }
        self.iso_drag_pan = false;
        self.iso_drag_orbit = false;
        self.dragging_endpoint = None;
        self.dragging_control = None;
        self.dragging_intersection = None;
        self.box_start = None;
        self.map_tool = MapTool::Select;

        self.map_start = None;
        self.map_start_ref = None;
        self.landscape_points.clear();
        self.minimal.annotation_start = None;
        self.minimal.annotation_text.clear();
        self.selected_segment = None;
        self.selected_segments.clear();
        self.selected_endpoint = None;
    }
    fn append_annotation(&mut self, name: &str, args: Vec<Argument>) {
        let mut suffix = 1;
        let id = loop {
            let id = format!("{name}_{suffix}");
            if !self.program.statements.iter().any(|s| match s {
                Statement::Call(c) => c.assigns.contains(&id),
                Statement::Road(r) => r.name == id,
                Statement::Block(b) => b.name == id,
            }) {
                break id;
            }
            suffix += 1;
        };
        self.program.statements.push(Statement::Call(Call {
            name: name.into(),
            assigns: vec![id],
            args,
        }));
        self.dirty = true;
        self.cancel_placement();
    }
    pub(super) fn annotation_input(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
    ) -> bool {
        if !matches!(
            self.map_tool,
            MapTool::AnnotationSegment | MapTool::AnnotationText
        ) {
            return false;
        }
        if self.minimal.input_suspended {
            return true;
        }
        let escape = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if escape {
            self.cancel_placement();
            return true;
        }
        if self.map_tool == MapTool::AnnotationText && self.minimal.annotation_start.is_some() {
            let point = self.minimal.annotation_start.unwrap();
            let Some(screen) = self.annotation_screen(rect, point) else {
                return true;
            };
            let screen = egui::pos2(
                screen
                    .x
                    .clamp(rect.left(), (rect.right() - 240.0).max(rect.left())),
                screen
                    .y
                    .clamp(rect.top(), (rect.bottom() - 36.0).max(rect.top())),
            );
            let enter = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            let area = egui::Area::new(egui::Id::new("annotation-text-editor"))
                .order(egui::Order::Foreground)
                .fixed_pos(screen)
                .show(ui.ctx(), |ui| {
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut self.minimal.annotation_text)
                            .id(egui::Id::new("annotation-text-input"))
                            .text_color(RED)
                            .hint_text("Annotation text")
                            .desired_width(220.0),
                    );
                    field.request_focus();
                });
            self.minimal.toolbar_rect = Some(area.response.rect);
            // A click elsewhere cancels the pending label and consumes that click.
            let outside = response.clicked()
                && response
                    .interact_pointer_pos()
                    .is_some_and(|p| !area.response.rect.contains(p));
            if enter {
                if !self.minimal.annotation_text.trim().is_empty() {
                    let args = vec![
                        Argument::String(
                            serde_json::to_string(&self.minimal.annotation_text).unwrap(),
                        ),
                        Argument::Number(point.x.to_string()),
                        Argument::Number(point.y.to_string()),
                    ];
                    self.append_annotation("annotation_text", args);
                } else {
                    self.cancel_placement();
                }
                ui.memory_mut(|m| m.surrender_focus(egui::Id::new("annotation-text-input")));
            } else if outside {
                self.cancel_placement();
                ui.memory_mut(|m| m.surrender_focus(egui::Id::new("annotation-text-input")));
            }
            return true;
        }
        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response.interact_pointer_pos() {
                let point = if self.camera_view == CameraView::Top {
                    world_of(rect, self.map_pan, self.map_zoom, pointer)
                } else {
                    iso_ground_point(
                        rect,
                        self.map_pan,
                        self.map_zoom,
                        self.orbit_target,
                        self.orbit_angle,
                        self.orbit_elevation,
                        pointer,
                    )
                };
                if let Some(start) = self
                    .minimal
                    .annotation_start
                    .filter(|_| self.map_tool == MapTool::AnnotationSegment)
                {
                    if (point.x - start.x).hypot(point.y - start.y) > 0.0001 {
                        self.append_annotation(
                            "annotation_segment",
                            [start.x, start.y, point.x, point.y]
                                .into_iter()
                                .map(|v| Argument::Number(v.to_string()))
                                .collect(),
                        );
                    }
                } else {
                    self.minimal.annotation_start = Some(point);
                }
            }
        }
        true
    }
    pub(super) fn paint_annotations(&self, ui: &egui::Ui, rect: egui::Rect) {
        let painter = ui.painter_at(rect);
        let screen = |p| self.annotation_screen(rect, p);
        for annotation in annotations(&self.program) {
            match annotation {
                Annotation::Segment(a, b) => {
                    // Clip crossing segments at the same near plane as the scene camera.
                    let (mut a, mut b) = (a, b);
                    if self.camera_view == CameraView::Vehicular {
                        let depth = |p: WorldPoint| {
                            (p.x - self.vehicle_position.x) * self.vehicle_heading.cos()
                                + (p.y - self.vehicle_position.y) * self.vehicle_heading.sin()
                        };
                        let (da, db) = (depth(a), depth(b));
                        if da < 0.04 && db < 0.04 {
                            continue;
                        }
                        if da < 0.04 || db < 0.04 {
                            let t = (0.04001 - da) / (db - da);
                            let clipped = WorldPoint {
                                x: a.x + t * (b.x - a.x),
                                y: a.y + t * (b.y - a.y),
                            };
                            if da < 0.04 {
                                a = clipped;
                            } else {
                                b = clipped;
                            }
                        }
                    }
                    if let (Some(a), Some(b)) = (screen(a), screen(b)) {
                        painter.line_segment([a, b], egui::Stroke::new(2.0, RED));
                    }
                }
                Annotation::Text(text, p) => {
                    let Some(p) = screen(p) else {
                        continue;
                    };
                    painter.text(
                        p,
                        egui::Align2::LEFT_TOP,
                        text,
                        egui::FontId::proportional(16.0),
                        RED,
                    );
                }
            }
        }
        if self.map_tool == MapTool::AnnotationSegment {
            if let (Some(start), Some(pointer)) = (
                self.minimal.annotation_start,
                ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p)),
            ) {
                if let Some(start) = screen(start) {
                    painter.line_segment([start, pointer], egui::Stroke::new(2.0, RED));
                }
            }
        }
        let hint = match self.map_tool {
            MapTool::BoxSelect => Some("Box select · drag to select roads · Esc cancel".into()),
            MapTool::Draw => Some("Road · click start and end · Esc cancel".into()),
            MapTool::Landscape(kind) => Some(format!(
                "{} · click points ({} minimum) · Enter finish · Esc cancel",
                landscape_label(kind),
                if kind == LandscapeKind::River { 2 } else { 3 }
            )),
            MapTool::AnnotationSegment => {
                Some("Annotation segment · click two points · Esc cancel".into())
            }
            MapTool::AnnotationText => {
                Some("Annotation text · click anchor, type text · Enter finish · Esc cancel".into())
            }
            _ => None,
        };
        let hint = if self.map_tool == MapTool::Draw {
            self.minimal
                .placement_error
                .as_ref()
                .map(|error| format!("{error} · choose another endpoint or Esc cancel"))
                .or(hint)
        } else {
            hint
        };
        if let Some(hint) = hint {
            painter.text(
                rect.left_bottom() + egui::vec2(12.0, -12.0),
                egui::Align2::LEFT_BOTTOM,
                hint,
                egui::FontId::proportional(13.0),
                ui.visuals().text_color(),
            );
        }
    }
}
