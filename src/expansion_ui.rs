//! Shared ghost picker for editor and game. It never writes files itself.
use crate::{
    expansion::{self, End, Proposal},
    Program,
};
mod icons;
use icons::{button, Icon};

pub struct Picker {
    pub open: bool,
    snapshot: Option<Program>,
    ends: Vec<End>,
    pub proposals: Vec<Proposal>,
    pub selected: usize,
    end: Option<usize>,
    length: f32,
    curve_degrees: f32,
    grade: f32,
    element: crate::RoadElementKind,
    seed: u64,
    steps: usize,
    pub message: String,
    growth: bool,
    reveal_selected: bool,
    keyboard_focus: Option<egui::Id>,
}
impl Default for Picker {
    fn default() -> Self {
        Self {
            open: false,
            snapshot: None,
            ends: vec![],
            proposals: vec![],
            selected: 0,
            end: None,
            length: 1.,
            curve_degrees: 45.,
            grade: 0.04,
            element: crate::RoadElementKind::CarLane,
            seed: 1,
            steps: 8,
            message: String::new(),
            growth: false,
            reveal_selected: false,
            keyboard_focus: None,
        }
    }
}
impl Picker {
    /// Ghosts and end handles drawn over a consumer's world camera.
    pub fn world_overlay(
        &mut self,
        ctx: &egui::Context,
        program: &Program,
        viewport: egui::Rect,
        project: impl Fn([f32; 3]) -> Option<egui::Pos2>,
    ) {
        if !self.open {
            return;
        }
        self.refresh(program);
        let painter = ctx
            .layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("expansion-world"),
            ))
            .with_clip_rect(viewport);
        let pointer = ctx.pointer_hover_pos();
        let clicked = ctx.input(|i| i.pointer.primary_clicked())
            && pointer.is_some_and(|p| viewport.contains(p))
            && pointer
                .and_then(|p| ctx.layer_id_at(p))
                .is_none_or(|layer| layer.order == egui::Order::Background);
        let mut chosen = None;
        for (i, end) in self.ends.iter().enumerate() {
            if let Some(p) = project([end.point.x, end.point.y, end.height]) {
                let selected = self.end == Some(i);
                if selected {
                    painter.circle_filled(
                        p,
                        12.,
                        egui::Color32::from_rgba_unmultiplied(240, 190, 70, 35),
                    );
                }
                painter.circle_stroke(
                    p,
                    if selected { 8. } else { 6. },
                    egui::Stroke::new(2., egui::Color32::from_rgb(240, 190, 70)),
                );
                if clicked && pointer.is_some_and(|q| q.distance(p) < 14.) {
                    chosen = Some(i);
                }
            }
        }
        if let Some(i) = chosen {
            self.choose_end(program, i);
        }
        let mut hit = None;
        let mut hit_distance = 7.;
        for (index, proposal) in self.proposals.iter().enumerate() {
            for ghost in &proposal.ghosts {
                let half = proposal.ghost_width(ghost) / 2.;
                for i in 0..32 {
                    let a = i as f32 / 32.;
                    let b = (i + 1) as f32 / 32.;
                    let pa = ghost.point_at(a);
                    let pb = ghost.point_at(b);
                    let ta = ghost.tangent_at(a);
                    let tb = ghost.tangent_at(b);
                    let ha = ta.y.atan2(ta.x);
                    let hb = tb.y.atan2(tb.x);
                    let corners = [
                        [
                            pa.x - ha.sin() * half,
                            pa.y + ha.cos() * half,
                            ghost.height_at(a),
                        ],
                        [
                            pb.x - hb.sin() * half,
                            pb.y + hb.cos() * half,
                            ghost.height_at(b),
                        ],
                        [
                            pb.x + hb.sin() * half,
                            pb.y - hb.cos() * half,
                            ghost.height_at(b),
                        ],
                        [
                            pa.x + ha.sin() * half,
                            pa.y - ha.cos() * half,
                            ghost.height_at(a),
                        ],
                    ];
                    let polygon: Option<Vec<_>> = corners.into_iter().map(&project).collect();
                    if let Some(polygon) = polygon {
                        painter.add(egui::Shape::convex_polygon(
                            polygon,
                            egui::Color32::from_rgba_unmultiplied(
                                184,
                                228,
                                210,
                                if index == self.selected { 35 } else { 5 },
                            ),
                            egui::Stroke::NONE,
                        ));
                    }
                    if let (Some(a), Some(b)) = (
                        project([pa.x, pa.y, ghost.height_at(a)]),
                        project([pb.x, pb.y, ghost.height_at(b)]),
                    ) {
                        if index == self.selected || i % 2 == 0 {
                            painter.line_segment(
                                [a, b],
                                egui::Stroke::new(
                                    if index == self.selected { 4. } else { 1. },
                                    egui::Color32::from_rgba_unmultiplied(
                                        184,
                                        228,
                                        210,
                                        if index == self.selected { 210 } else { 65 },
                                    ),
                                ),
                            );
                        }
                        if chosen.is_none() && clicked {
                            if let Some(p) = pointer {
                                let distance = screen_distance(p, a, b);
                                if distance < hit_distance {
                                    hit_distance = distance;
                                    hit = Some(index);
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(i) = hit {
            self.selected = i;
        }
    }
    pub fn refresh(&mut self, program: &Program) {
        if self.snapshot.as_ref() == Some(program) {
            return;
        }
        self.ends = expansion::ends(program);
        self.proposals.clear();
        self.end = None;
        self.selected = 0;
        self.snapshot = Some(program.clone());
    }
    pub fn start(&mut self) {
        self.open = true;
        self.growth = false;
    }
    pub fn start_growth(&mut self) {
        self.open = true;
        self.growth = true;
    }
    /// The same logical height is used by the panel and the native GPU viewport.
    pub fn viewport_rect(&self, bounds: egui::Rect) -> egui::Rect {
        let height = if self.open {
            42_f32.min((bounds.height() - 1.).max(0.))
        } else {
            0.
        };
        egui::Rect::from_min_max(
            bounds.min,
            egui::pos2(bounds.right(), bounds.bottom() - height),
        )
    }
    fn choose_end(&mut self, program: &Program, index: usize) {
        self.end = Some(index);
        self.selected = 0;
        self.proposals = expansion::propose(
            program,
            &self.ends[index],
            expansion::Options {
                length: self.length,
                curve: self.curve_degrees.to_radians(),
                grade: self.grade,
                element: self.element,
            },
        );
        self.proposals.sort_by_key(|p| icons::order(p.kind));
        self.reveal_selected = true;
    }
    fn step_end(&mut self, program: &Program, previous: bool) {
        if self.ends.is_empty() {
            return;
        }
        let index = match self.end {
            Some(i) if previous => (i + self.ends.len() - 1) % self.ends.len(),
            Some(i) => (i + 1) % self.ends.len(),
            None if previous => self.ends.len() - 1,
            None => 0,
        };
        self.choose_end(program, index);
    }
    pub fn selected(&self) -> Option<&Proposal> {
        self.proposals.get(self.selected)
    }
    fn accept(&mut self, program: &mut Program) {
        if let Some(p) = self.selected() {
            self.message = match p.apply(program) {
                Ok(()) => format!("Applied {}. Save the map to keep it.", p.kind.label()),
                Err(e) => e,
            };
            self.refresh(program);
        }
    }
    /// Reserves a thin strip in the caller's root UI before its canvas is laid out.
    /// `editable` is false for bundled/browser maps without an authored file.
    pub fn show(&mut self, root: &mut egui::Ui, program: &mut Program, editable: bool) -> bool {
        if !self.open {
            return false;
        }
        let ctx = root.ctx().clone();
        // Enter can release a numeric/text edit earlier in this frame. That
        // same key must not also place a road after the edit commits.
        let keyboard_free = root.is_enabled()
            && ctx.memory(|m| {
                m.focused().is_none()
                    && !self
                        .keyboard_focus
                        .is_some_and(|id| m.had_focus_last_frame(id))
            });
        if root.is_enabled()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.open = false;
            ctx.request_repaint();
            return false;
        }
        self.refresh(program);
        let before = program.clone();
        let mut enter = false;
        if keyboard_free && !self.growth {
            if !self.proposals.is_empty() {
                let previous =
                    ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
                let next = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
                if previous || next {
                    self.selected = (self.selected
                        + if previous {
                            self.proposals.len() - 1
                        } else {
                            1
                        })
                        % self.proposals.len();
                    self.reveal_selected = true;
                }
            }
            enter = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        }
        let bounds = root.available_rect_before_wrap();
        let height = bounds.bottom() - self.viewport_rect(bounds).bottom();
        egui::Panel::bottom("expansion-fringe")
            .exact_size(height)
            .show_separator_line(false)
            .frame(
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgb(27, 41, 34))
                    .inner_margin(egui::Margin::symmetric(8, 7)),
            )
            .show(root, |ui| {
                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                *ui.visuals_mut() = egui::Visuals::dark();
                ui.spacing_mut().item_spacing = egui::vec2(4., 0.);
                ui.spacing_mut().interact_size.y = 26.;
                ui.horizontal(|ui| {
                    // Keep confirmation and close reachable even on narrow windows.
                    let controls_width = if self.message.is_empty() { 64. } else { 96. };
                    let content = egui::vec2((ui.available_width() - controls_width).max(0.), 28.);
                    ui.allocate_ui_with_layout(
                        content,
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            egui::ScrollArea::horizontal()
                                .id_salt("expansion-fringe-scroll")
                                .scroll_bar_visibility(
                                    egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                                )
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if self.growth {
                                            self.growth_controls(ui);
                                        } else {
                                            self.extension_controls(ui, program);
                                        }
                                    });
                                });
                        },
                    );
                    if !self.message.is_empty() {
                        button(ui, Icon::Message, false, &self.message);
                    }
                    let help = if !editable {
                        "Preview only: open an authored .road file"
                    } else if self.growth {
                        "Grow network"
                    } else {
                        "Place extension · Enter"
                    };
                    let enabled = editable && (self.growth || self.selected().is_some());
                    let apply = ui
                        .add_enabled_ui(enabled, |ui| button(ui, Icon::Apply, true, help))
                        .inner;
                    if (apply.clicked() || enter) && enabled {
                        apply.surrender_focus();
                        if self.growth {
                            let count = expansion::grow_with_options(
                                program,
                                self.seed,
                                self.steps,
                                expansion::Options {
                                    length: self.length,
                                    curve: self.curve_degrees.to_radians(),
                                    grade: self.grade,
                                    element: self.element,
                                },
                            );
                            self.message = format!("Grew {count} steps; save to keep the result.");
                            self.refresh(program);
                        } else {
                            self.accept(program);
                        }
                    }
                    if button(ui, Icon::Close, false, "Done · Esc").clicked() {
                        self.open = false;
                        ctx.request_repaint();
                    }
                });
            });
        self.reveal_selected = false;
        self.keyboard_focus = ctx.memory(|m| m.focused());
        *program != before
    }
    fn extension_controls(&mut self, ui: &mut egui::Ui, program: &Program) {
        let previous = button(ui, Icon::Previous, false, "Previous open end");
        if previous.clicked() {
            self.step_end(program, true);
            previous.surrender_focus();
        }
        let label = match self.end {
            Some(i) => format!("Open end {} / {}", i + 1, self.ends.len()),
            None => format!("{} open ends", self.ends.len()),
        };
        let endpoint = button(ui, Icon::End, false, &label);
        if endpoint.clicked() {
            self.step_end(program, false);
            endpoint.surrender_focus();
        }
        let next = button(ui, Icon::Next, false, "Next open end");
        if next.clicked() {
            self.step_end(program, false);
            next.surrender_focus();
        }
        ui.separator();
        let mut pick = None;
        for (i, proposal) in self.proposals.iter().enumerate() {
            let response = ui
                .push_id(proposal.kind.label(), |ui| {
                    button(
                        ui,
                        Icon::Proposal(proposal.kind),
                        self.selected == i,
                        proposal.kind.label(),
                    )
                })
                .inner;
            if self.reveal_selected && self.selected == i {
                response.scroll_to_me(Some(egui::Align::Center));
            }
            if response.clicked() {
                pick = Some(i);
                response.surrender_focus();
            }
        }
        if let Some(i) = pick {
            self.selected = i;
        }
        let kind = self.selected().map(|p| p.kind);
        let Some(kind) = kind else {
            return;
        };
        if kind == expansion::Kind::Stop {
            return;
        }
        ui.separator();
        let mut changed = ui
            .add(
                egui::DragValue::new(&mut self.length)
                    .range(0.05..=100.)
                    .speed(0.05)
                    .suffix(" u"),
            )
            .on_hover_text("Length in map units")
            .changed();
        match kind {
            expansion::Kind::CurveLeft | expansion::Kind::CurveRight => {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.curve_degrees)
                            .range(5.0..=145.0)
                            .speed(1.)
                            .suffix("°"),
                    )
                    .on_hover_text("Curve angle")
                    .changed();
            }
            expansion::Kind::Rise | expansion::Kind::Fall => {
                let mut percent = self.grade * 100.;
                if ui
                    .add(
                        egui::DragValue::new(&mut percent)
                            .range(0.5..=10.)
                            .speed(0.5)
                            .suffix("%"),
                    )
                    .on_hover_text("Grade")
                    .changed()
                {
                    self.grade = percent / 100.;
                    changed = true;
                }
            }
            expansion::Kind::AddElement | expansion::Kind::RemoveElement => {
                let kinds = element_kinds();
                let index = kinds
                    .iter()
                    .position(|kind| *kind == self.element)
                    .unwrap_or(0);
                let previous = button(ui, Icon::Previous, false, "Previous element");
                if previous.clicked() {
                    self.element = kinds[(index + kinds.len() - 1) % kinds.len()];
                    changed = true;
                    previous.surrender_focus();
                }
                ui.label(egui::RichText::new(element_label(self.element)).size(11.));
                let next = button(ui, Icon::Next, false, "Next element");
                if next.clicked() {
                    self.element = kinds[(index + 1) % kinds.len()];
                    changed = true;
                    next.surrender_focus();
                }
            }
            _ => {}
        }
        if changed {
            if let Some(index) = self.end {
                self.choose_end(program, index);
                self.selected = self
                    .proposals
                    .iter()
                    .position(|p| p.kind == kind)
                    .unwrap_or(0);
            }
        }
    }
    fn growth_controls(&mut self, ui: &mut egui::Ui) {
        if button(ui, Icon::Previous, false, "Back to extensions").clicked() {
            self.growth = false;
        }
        ui.label(egui::RichText::new("Seed").size(11.));
        ui.add(egui::DragValue::new(&mut self.seed));
        ui.label(egui::RichText::new("Steps").size(11.));
        ui.add(egui::DragValue::new(&mut self.steps).range(1..=32));
        ui.add(
            egui::DragValue::new(&mut self.length)
                .range(0.05..=100.)
                .speed(0.05)
                .suffix(" u"),
        )
        .on_hover_text("Length in map units");
    }
}
fn element_kinds() -> [crate::RoadElementKind; 16] {
    use crate::RoadElementKind::*;
    [
        CarLane,
        Sidewalk,
        ParkingLane,
        BikeLane,
        BusLane,
        BusStop,
        Middle,
        Verge,
        Barrier,
        BuildingEdge,
        TrainTrack,
        TramTrack,
        SubwayTrack,
        Runway,
        Taxiway,
        Apron,
    ]
}
fn screen_distance(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let v = b - a;
    let t = if v.length_sq() > 0. {
        (p - a).dot(v) / v.length_sq()
    } else {
        0.
    }
    .clamp(0., 1.);
    p.distance(a + v * t)
}

fn element_label(kind: crate::RoadElementKind) -> &'static str {
    use crate::RoadElementKind::*;
    match kind {
        CarLane => "Car lane",
        Sidewalk => "Sidewalk",
        ParkingLane => "Parking lane",
        BikeLane => "Bike lane",
        BusLane => "Bus lane",
        BusStop => "Bus stop",
        Middle => "Median",
        Verge => "Verge",
        Barrier => "Barrier",
        BuildingEdge => "Building edge",
        TrainTrack => "Train track",
        TramTrack => "Tram track",
        SubwayTrack => "Subway track",
        Runway => "Runway",
        Taxiway => "Taxiway",
        Apron => "Apron",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;
    fn frame(
        picker: &mut Picker,
        ctx: &egui::Context,
        program: &mut Program,
        events: Vec<egui::Event>,
        editable: bool,
    ) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280., 800.),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                picker.show(ui, program, editable);
                let viewport = ui.available_rect_before_wrap();
                picker.world_overlay(ui.ctx(), program, viewport, |p| {
                    Some(egui::pos2(900. + p[0] * 80., 200. - p[1] * 80.))
                });
            },
        );
        output.textures_delta.clear();
    }
    fn key(key: egui::Key) -> Vec<egui::Event> {
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .collect()
    }
    #[test]
    fn fringe_reserves_exactly_42_points_and_never_picks_a_covered_end() {
        for width in [320., 640., 1280., 2048.] {
            let mut program =
                crate::parse("road street: carlane carlane; end a=street(0,0,1,0);").unwrap();
            let mut picker = Picker::default();
            picker.start();
            let ctx = egui::Context::default();
            let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 800.));
            for selected in [false, true] {
                if selected {
                    picker.choose_end(&program, 1);
                }
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(bounds),
                        ..Default::default()
                    },
                    |ui| {
                        picker.show(ui, &mut program, true);
                        let viewport = ui.available_rect_before_wrap();
                        assert_eq!(
                            viewport.bottom(),
                            758.,
                            "width={width}, selected={selected}"
                        );
                        assert_eq!(viewport, picker.viewport_rect(bounds));
                    },
                );
                output.textures_delta.clear();
            }
            picker.open = false;
            assert_eq!(picker.viewport_rect(bounds), bounds);
        }
    }
    #[test]
    fn focused_numeric_edit_cannot_apply_or_cycle_a_proposal() {
        let mut program =
            crate::parse("road street: carlane carlane; end street(0,0,1,0);").unwrap();
        let before = program.clone();
        let mut picker = Picker::default();
        picker.start();
        picker.refresh(&program);
        picker.choose_end(&program, 1);
        let ctx = egui::Context::default();
        let mut text = String::from("1.0");
        let id = egui::Id::new("focused-geometry-value");
        for events in [vec![], key(egui::Key::Enter), key(egui::Key::Tab)] {
            ctx.memory_mut(|m| m.request_focus(id));
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280., 800.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.add(egui::TextEdit::singleline(&mut text).id(id));
                    picker.show(ui, &mut program, true);
                },
            );
            output.textures_delta.clear();
            assert_eq!(program, before);
            assert_eq!(picker.selected().unwrap().kind, expansion::Kind::Continue);
        }
    }
    #[test]
    fn world_click_tab_and_enter_accept_ghost_without_preview_edits() {
        let mut program =
            crate::parse("road street: carlane middle carlane; end a=street(0,0,1,0);").unwrap();
        let before = program.clone();
        let ctx = egui::Context::default();
        let mut picker = Picker::default();
        picker.open = true;
        frame(&mut picker, &ctx, &mut program, vec![], true);
        let p = egui::pos2(980., 200.);
        let mut events = vec![egui::Event::PointerMoved(p)];
        for pressed in [true, false] {
            events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        frame(&mut picker, &ctx, &mut program, events, true);
        assert!(!picker.proposals.is_empty());
        assert_eq!(program, before);
        assert_eq!(picker.selected().unwrap().kind, expansion::Kind::Continue);
        frame(&mut picker, &ctx, &mut program, key(egui::Key::Tab), true);
        assert_eq!(picker.selected().unwrap().kind, expansion::Kind::CurveLeft);
        frame(
            &mut picker,
            &ctx,
            &mut program,
            key(egui::Key::Enter),
            false,
        );
        assert_eq!(program, before);
        frame(&mut picker, &ctx, &mut program, key(egui::Key::Enter), true);
        assert_eq!(map::project(&program).segments.len(), 2);
        assert!(picker.proposals.is_empty());
    }
    #[test]
    fn intersection_only_seed_exposes_four_pickable_ends() {
        let mut program = crate::parse(include_str!("../examples/expansion-seed.road")).unwrap();
        let ctx = egui::Context::default();
        let mut picker = Picker::default();
        picker.open = true;
        frame(&mut picker, &ctx, &mut program, vec![], true);
        assert_eq!(picker.ends.len(), 4);
        picker.choose_end(&program, 0);
        assert!(picker
            .proposals
            .iter()
            .any(|p| p.kind == expansion::Kind::Continue));
    }
}
