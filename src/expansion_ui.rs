//! Shared ghost picker for editor and game. It never writes files itself.
use crate::{
    expansion::{self, End, Proposal},
    map::{self, WorldPoint},
    Program,
};

pub struct Picker {
    pub open: bool,
    snapshot: Option<Program>,
    projection: Option<map::Projection>,
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
    pan: egui::Vec2,
    zoom: f32,
}
impl Default for Picker {
    fn default() -> Self {
        Self {
            open: false,
            snapshot: None,
            projection: None,
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
            pan: egui::Vec2::ZERO,
            zoom: 1.,
        }
    }
}
impl Picker {
    /// Ghosts and end handles drawn over a consumer's world camera.
    pub fn world_overlay(
        &mut self,
        ctx: &egui::Context,
        program: &Program,
        project: impl Fn([f32; 3]) -> Option<egui::Pos2>,
    ) {
        if !self.open {
            return;
        }
        self.refresh(program);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("expansion-world"),
        ));
        let pointer = ctx.pointer_hover_pos();
        let clicked = ctx.input(|i| i.pointer.primary_clicked())
            && pointer
                .and_then(|p| ctx.layer_id_at(p))
                .is_none_or(|layer| layer.order == egui::Order::Background);
        let mut chosen = None;
        for (i, end) in self.ends.iter().enumerate() {
            if let Some(p) = project([end.point.x, end.point.y, end.height]) {
                painter.circle_stroke(
                    p,
                    8.,
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
                                80,
                                210,
                                240,
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
                                        80,
                                        210,
                                        240,
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
        self.projection = Some(map::project(program));
        self.proposals.clear();
        self.end = None;
        self.selected = 0;
        self.snapshot = Some(program.clone());
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
    /// `editable` is false for bundled/browser maps without an authored file.
    pub fn show(&mut self, ctx: &egui::Context, program: &mut Program, editable: bool) -> bool {
        if !self.open {
            return false;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.open = false;
            return false;
        }
        self.refresh(program);
        let before = program.clone();
        let mut open = self.open;
        egui::Window::new("Expand road network")
            .open(&mut open)
            .default_size(egui::vec2(680., 620.))
            .show(ctx, |ui| {
                ui.label("Pick an open end. Tab cycles ghosts; click to select; Enter applies.");
                if !editable {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "Preview only: open a .road file.",
                    );
                }
                ui.horizontal(|ui| {
                    ui.label("Length");
                    let mut changed = ui
                        .add(
                            egui::DragValue::new(&mut self.length)
                                .range(0.05..=100.)
                                .speed(0.05),
                        )
                        .changed();
                    ui.label("Curve °");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.curve_degrees)
                                .range(5.0..=145.0)
                                .speed(1.),
                        )
                        .changed();
                    ui.label("Grade");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.grade)
                                .range(0.005..=0.1)
                                .speed(0.005),
                        )
                        .changed();
                    if changed {
                        if let Some(i) = self.end {
                            self.choose_end(program, i);
                        }
                    }
                    if ui.button("Refresh").clicked() {
                        self.snapshot = None;
                        self.refresh(program);
                    }
                    ui.label(format!("{} open ends", self.ends.len()));
                });
                let previous = self.element;
                egui::ComboBox::from_label("Element to start or end")
                    .selected_text(element_label(self.element))
                    .show_ui(ui, |ui| {
                        use crate::RoadElementKind::*;
                        for kind in [
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
                        ] {
                            ui.selectable_value(&mut self.element, kind, element_label(kind));
                        }
                    });
                if self.element != previous {
                    if let Some(i) = self.end {
                        self.choose_end(program, i);
                    }
                }
                if !self.proposals.is_empty() {
                    let next =
                        ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
                    let previous =
                        ui.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
                    if next {
                        self.selected = (self.selected + 1) % self.proposals.len();
                    }
                    if previous {
                        self.selected =
                            (self.selected + self.proposals.len() - 1) % self.proposals.len();
                    }
                }
                self.canvas(ui, program);
                ui.horizontal_wrapped(|ui| {
                    let labels: Vec<_> = self.proposals.iter().map(|p| p.kind.label()).collect();
                    for (i, label) in labels.into_iter().enumerate() {
                        if ui.selectable_label(self.selected == i, label).clicked() {
                            self.selected = i;
                        }
                    }
                });
                let enter =
                    ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                let apply = ui
                    .add_enabled(
                        editable && self.selected().is_some(),
                        egui::Button::new("Apply selected extension"),
                    )
                    .clicked();
                if apply || (enter && editable) {
                    self.accept(program);
                }
                ui.collapsing("Auto-growth experiment", |ui| {
                    ui.label(
                        "Grow from this network, checking each step. Editor Undo is available.",
                    );
                    ui.horizontal(|ui| {
                        ui.label("Seed");
                        ui.add(egui::DragValue::new(&mut self.seed));
                        ui.label("Steps");
                        ui.add(egui::DragValue::new(&mut self.steps).range(1..=32));
                        if ui
                            .add_enabled(editable, egui::Button::new("Grow"))
                            .clicked()
                        {
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
                        }
                    });
                });
                ui.label(&self.message);
            });
        self.open = open;
        *program != before
    }
    fn canvas(&mut self, ui: &mut egui::Ui, program: &Program) {
        let projection = self
            .projection
            .as_ref()
            .expect("picker refreshed before painting");
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 320.),
            egui::Sense::click_and_drag(),
        );
        let points: Vec<_> = projection
            .segments
            .iter()
            .flat_map(|s| [s.start, s.end])
            .chain(self.ends.iter().map(|end| end.point))
            .chain(
                self.proposals
                    .iter()
                    .flat_map(|p| p.ghosts.iter().flat_map(|s| [s.start, s.end])),
            )
            .collect();
        if points.is_empty() {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Place a road or open the expansion seed map",
                egui::FontId::proportional(14.),
                ui.visuals().text_color(),
            );
            return;
        }
        let min = WorldPoint {
            x: points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min),
            y: points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min),
        };
        let max = WorldPoint {
            x: points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max),
            y: points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max),
        };
        let scale = ((rect.width() - 32.) / (max.x - min.x).max(0.1))
            .min((rect.height() - 32.) / (max.y - min.y).max(0.1))
            * self.zoom;
        let center = WorldPoint {
            x: (min.x + max.x) / 2.,
            y: (min.y + max.y) / 2.,
        };
        let screen = |p: WorldPoint| {
            rect.center()
                + self.pan
                + egui::vec2((p.x - center.x) * scale, -(p.y - center.y) * scale)
        };
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4., ui.visuals().extreme_bg_color);
        for segment in &projection.segments {
            let line = (0..=32)
                .map(|i| screen(segment.point_at(i as f32 / 32.)))
                .collect();
            painter.add(egui::Shape::line(
                line,
                egui::Stroke::new(2., ui.visuals().text_color().gamma_multiply(0.45)),
            ));
        }
        let click = response
            .clicked()
            .then(|| response.interact_pointer_pos())
            .flatten();
        let mut ghost_hit = None;
        let mut hit_distance = 7.;
        for (index, proposal) in self.proposals.iter().enumerate() {
            let color = if index == self.selected {
                ui.visuals().selection.stroke.color
            } else {
                ui.visuals().weak_text_color().gamma_multiply(0.35)
            };
            for ghost in &proposal.ghosts {
                for i in 0..32 {
                    let a = screen(ghost.point_at(i as f32 / 32.));
                    let b = screen(ghost.point_at((i + 1) as f32 / 32.));
                    if index == self.selected || i % 2 == 0 {
                        painter.line_segment(
                            [a, b],
                            egui::Stroke::new(if index == self.selected { 4. } else { 1. }, color),
                        );
                    }
                    if let Some(p) = click {
                        let distance = screen_distance(p, a, b);
                        if distance < hit_distance {
                            hit_distance = distance;
                            ghost_hit = Some(index);
                        }
                    }
                }
            }
        }
        let mut end_hit = None;
        for (i, end) in self.ends.iter().enumerate() {
            let p = screen(end.point);
            painter.circle_filled(
                p,
                5.,
                if self.end == Some(i) {
                    ui.visuals().selection.stroke.color
                } else {
                    ui.visuals().warn_fg_color
                },
            );
            if click.is_some_and(|c| c.distance(p) < 12.) {
                end_hit = Some(i);
            }
        }
        if let Some(i) = end_hit {
            self.choose_end(program, i);
        } else if let Some(i) = ghost_hit {
            self.selected = i;
        }
        if response.dragged() {
            self.pan += response.drag_delta();
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            self.zoom = (self.zoom * (scroll * 0.002).exp()).clamp(0.25, 20.);
        }
    }
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
                picker.world_overlay(ui.ctx(), program, |p| {
                    Some(egui::pos2(900. + p[0] * 80., 200. - p[1] * 80.))
                });
                picker.show(ui.ctx(), program, editable);
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
        assert_eq!(picker.selected().unwrap().kind, expansion::Kind::Stop);
        frame(&mut picker, &ctx, &mut program, key(egui::Key::Tab), true);
        assert_eq!(picker.selected().unwrap().kind, expansion::Kind::Continue);
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
