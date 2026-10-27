//! Small anchored actions for actual road endpoints and lane ends.
use super::*;

#[derive(Clone, Copy)]
enum PointAction {
    Draw,
    RotateLeft,
    RotateRight,
    Profile,
    Close,
}

#[derive(Clone)]
pub(super) struct PointOptions {
    source: SegmentSource,
    end: bool,
    lane: Option<usize>,
    position: WorldPoint,
    height: f32,
    program: Program,
}

impl RoadEditor {
    /// Own only clicks, leaving the viewport's drag threshold and geometry intact.
    pub(super) fn point_options_ui(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        projection: &map::Projection,
        iso: bool,
    ) -> bool {
        if self.minimal.input_suspended
            || self.pending.is_some()
            || self.map_tool != MapTool::Select
            || self.dragging_endpoint.is_some()
            || self.dragging_control.is_some()
            || self.dragging_intersection.is_some()
            || ui.input(|i| i.pointer.is_decidedly_dragging())
        {
            self.point_options = None;
            return false;
        }
        if self
            .point_options
            .as_ref()
            .is_some_and(|p| p.program != self.program)
            || ui.input(|i| i.key_pressed(egui::Key::Escape))
        {
            return self.point_options.take().is_some();
        }
        if let Some(options) = self.point_options.clone() {
            let anchor = self.spatial_screen(rect, options.position, options.height, iso);
            let mut action = None;
            let area = egui::Area::new(egui::Id::new("road-point-options"))
                .order(egui::Order::Foreground)
                .fixed_pos(
                    (anchor + egui::vec2(14.0, 14.0))
                        .clamp(rect.min, rect.max - egui::vec2(195.0, 160.0)),
                )
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(170.0);
                        ui.label(if options.lane.is_some() {
                            "Lane end"
                        } else {
                            "Road end"
                        });
                        if options.lane.is_none()
                            || options.lane.is_some_and(|lane| {
                                projection
                                    .segments
                                    .iter()
                                    .find(|s| s.source == options.source)
                                    .and_then(|s| road_for_segment(&self.program, s))
                                    .is_some_and(|road| {
                                        road.elements[lane].kind == RoadElementKind::CarLane
                                    })
                            })
                        {
                            if ui
                                .button(if options.lane.is_some() {
                                    "Branch from lane"
                                } else {
                                    "Extend road"
                                })
                                .clicked()
                            {
                                action = Some(PointAction::Draw);
                            }
                        }
                        if options.lane.is_none() {
                            if ui.button("Rotate tangent −15°").clicked() {
                                action = Some(PointAction::RotateLeft);
                            }
                            if ui.button("Rotate tangent +15°").clicked() {
                                action = Some(PointAction::RotateRight);
                            }
                        }
                        if ui.button("Edit cross-section").clicked() {
                            action = Some(PointAction::Profile);
                        }
                        if ui.button("Close").clicked() {
                            action = Some(PointAction::Close);
                        }
                    });
                });
            if let Some(action) = action {
                self.point_options = None;
                match action {
                    PointAction::Draw => {
                        self.begin_point_road(&options);
                    }
                    PointAction::RotateLeft | PointAction::RotateRight => {
                        self.dirty |= map::rotate_endpoint(
                            &mut self.program,
                            options.source,
                            options.end,
                            if matches!(action, PointAction::RotateLeft) {
                                -15.0
                            } else {
                                15.0
                            },
                        );
                    }
                    PointAction::Profile => self.enter_section_editor(),
                    _ => {}
                }
            } else if ui.input(|i| i.pointer.any_pressed())
                && ui
                    .ctx()
                    .pointer_hover_pos()
                    .is_none_or(|p| !area.response.rect.contains(p))
            {
                self.point_options = None;
            }
            return true;
        }
        if !response.clicked_by(egui::PointerButton::Primary)
            || self.minimal.pointer_over_toolbar(ui.ctx())
        {
            return false;
        }
        let Some(pointer) = response.interact_pointer_pos() else {
            return false;
        };
        let selected: Vec<_> = projection
            .segments
            .iter()
            .filter(|s| Some(s.source) == self.selected_segment)
            .cloned()
            .collect();
        let endpoint = |segments: &[map::Segment]| {
            if iso {
                iso_endpoint_near(
                    segments,
                    &self.program,
                    rect,
                    self.map_pan,
                    self.map_zoom,
                    self.orbit_target,
                    self.orbit_angle,
                    self.orbit_elevation,
                    pointer,
                )
            } else {
                endpoint_near(
                    segments,
                    &self.program,
                    rect,
                    self.map_pan,
                    self.map_zoom,
                    pointer,
                )
            }
        };
        let end = endpoint(&selected)
            .or_else(|| endpoint(&projection.segments))
            .filter(|(source, end)| {
                projection
                    .segments
                    .iter()
                    .find(|s| s.source == *source)
                    .is_some_and(|s| {
                        pointer.distance(self.spatial_screen(
                            rect,
                            if *end { s.end } else { s.start },
                            if *end { s.end_z } else { s.start_z },
                            iso,
                        )) <= 9.0
                    })
            });
        let interaction = self.interaction_geometry();
        let candidates = &interaction.element_ends;
        let lane = if iso {
            iso_element_end_near(
                &candidates,
                projection,
                rect,
                self.map_pan,
                self.map_zoom,
                self.orbit_target,
                self.orbit_angle,
                self.orbit_elevation,
                pointer,
            )
        } else {
            element_end_near(&candidates, rect, self.map_pan, self.map_zoom, pointer)
        };
        let end = end.filter(|(source, end)| {
            let segment = projection
                .segments
                .iter()
                .find(|s| s.source == *source)
                .unwrap();
            let distance = pointer.distance(self.spatial_screen(
                rect,
                if *end { segment.end } else { segment.start },
                if *end { segment.end_z } else { segment.start_z },
                iso,
            ));
            lane.is_none_or(|candidate| {
                let segment = projection
                    .segments
                    .iter()
                    .find(|s| s.source == candidate.source)
                    .unwrap();
                distance
                    <= pointer.distance(self.spatial_screen(
                        rect,
                        candidate.position,
                        segment.height_at(candidate.t),
                        iso,
                    ))
            })
        });
        let (source, end, lane, position) = if let Some((source, end)) = end {
            let s = projection
                .segments
                .iter()
                .find(|s| s.source == source)
                .unwrap();
            (source, end, None, if end { s.end } else { s.start })
        } else if let Some(lane) = lane {
            (
                lane.source,
                lane.t == 1.0,
                Some(lane.element_index),
                lane.position,
            )
        } else {
            return false;
        };
        let segment = projection
            .segments
            .iter()
            .find(|s| s.source == source)
            .unwrap();
        self.selected_segment = Some(source);
        self.selected_segments = vec![source];
        self.selected_endpoint = lane.is_none().then_some((source, end));
        self.selected_intersection = None;
        self.segment_name_draft = segment.name.clone().unwrap_or_default();
        self.point_options = Some(PointOptions {
            source,
            end,
            lane,
            position,
            height: segment.height_at(if end { 1.0 } else { 0.0 }),
            program: self.program.clone(),
        });
        true
    }

    fn begin_point_road(&mut self, options: &PointOptions) -> bool {
        if self.begin_road_click().is_none() {
            return false;
        }
        let result = if let Some(lane) = options.lane {
            map::create_lane_point(
                &mut self.program,
                options.source,
                lane,
                if options.end { 1.0 } else { 0.0 },
            )
            .map(|(reference, point)| {
                let segment = map::project(&self.program)
                    .segments
                    .into_iter()
                    .find(|s| s.source == options.source)
                    .unwrap();
                self.selected_road =
                    self.program.statements.iter().position(
                        |s| matches!(s, Statement::Road(r) if r.name == segment.road_name),
                    );
                self.map_tool = MapTool::Draw;
                self.map_start = Some(point);
                self.map_start_ref = Some(reference);
                self.branch_mode = map::SplitMode::KeepLane;
            })
            .is_some()
        } else {
            self.start_extension(options.source, options.end)
        };
        self.map_start_height = Some(options.height);
        self.hold_road_draft();
        self.dirty = self.program != self.saved_program;
        result
    }
}
