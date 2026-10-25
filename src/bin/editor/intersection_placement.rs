//! One-shot native intersection drafts. Preview construction never edits the city.
use super::*;

impl RoadEditor {
    pub(super) fn start_intersection_placement(&mut self, kind: map::IntersectionKind) {
        let selected = self.road().cloned();
        self.cancel_placement();
        if let Some(road) = selected {
            self.selected_road = self
                .program
                .statements
                .iter()
                .position(|s| matches!(s, Statement::Road(r) if r.name == road.name));
            if self.selected_road.is_none() {
                self.minimal.pending_road = Some(road);
            }
        } else {
            let presets = parse(include_str!("../../../examples/default_roads.road"))
                .expect("bundled road profiles");
            self.minimal.pending_road = presets.statements.into_iter().find_map(|s| match s {
                Statement::Road(road) if road.name == "neighborhood_street" => Some(road),
                _ => None,
            });
        }
        self.minimal.intersection_rotation = 0;
        self.map_tool = MapTool::Intersection(kind);
        self.status = "Intersection · click to place · R rotate · Esc cancel".into();
    }

    fn intersection_pointer(&self, rect: egui::Rect, pointer: egui::Pos2) -> WorldPoint {
        if self.camera_view == CameraView::Top {
            world_of(rect, self.map_pan, self.map_zoom, pointer)
        } else {
            terrain_placement::iso_pick_with_base(
                &self.program,
                self.path.as_deref().and_then(Path::parent),
                |height| {
                    iso_world_at_height(
                        rect,
                        self.map_pan,
                        self.map_zoom,
                        self.orbit_target,
                        self.orbit_angle,
                        self.orbit_elevation,
                        pointer,
                        height,
                    )
                },
            )
            .0
        }
    }

    fn intersection_draft(
        &self,
        position: WorldPoint,
    ) -> Option<(Program, String, Option<&'static str>)> {
        let MapTool::Intersection(kind) = self.map_tool else {
            return None;
        };
        let road = self.road()?.clone();
        let mut draft = self.program.clone();
        if !draft
            .statements
            .iter()
            .any(|s| matches!(s, Statement::Road(r) if r.name == road.name))
        {
            draft.statements.push(Statement::Road(road.clone()));
        }
        let center = map::place_intersection(&mut draft, kind, &road, position);
        for _ in 0..self.minimal.intersection_rotation {
            map::rotate_intersection(&mut draft, &center);
        }
        let half = road
            .elements
            .iter()
            .map(|e| map::element_world_width(e.kind))
            .sum::<f32>()
            * 0.5;
        let blocked = if terrain_placement::intersection_obstructed_with_base(
            &self.program,
            self.path.as_deref().and_then(Path::parent),
            position,
            half,
        ) {
            Some("Intersections need ground level; choose a place clear of raised terrain")
        } else if intersection_site_occupied(&self.program, position, half) {
            Some("Choose an empty site; connect roads to the new intersection ports")
        } else {
            None
        };
        Some((draft, center, blocked))
    }

    pub(super) fn intersection_placement_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
    ) -> bool {
        if !matches!(self.map_tool, MapTool::Intersection(_)) {
            return false;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.cancel_placement();
            return true;
        }
        if ui.input(|i| i.key_pressed(egui::Key::R)) {
            self.minimal.intersection_rotation = (self.minimal.intersection_rotation + 1) % 4;
        }
        if self.minimal.pointer_over_toolbar(ui.ctx()) {
            return true;
        }
        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response
                .interact_pointer_pos()
                .filter(|p| rect.contains(*p))
            {
                let position = self.intersection_pointer(rect, pointer);
                if let Some((draft, center, blocked)) = self.intersection_draft(position) {
                    if let Some(reason) = blocked {
                        self.status = reason.into();
                    } else {
                        let road_name = self.ensure_placement_road_type();
                        self.program = draft;
                        if let Some(name) = road_name {
                            self.selected_road = self.program.statements.iter().position(
                                |s| matches!(s, Statement::Road(road) if road.name == name),
                            );
                        }
                        self.cancel_placement();
                        self.selected_segment = None;
                        self.selected_segments.clear();
                        self.selected_endpoint = None;
                        self.selected_intersection = Some(center);
                        self.status = "Intersection placed".into();
                    }
                }
            }
        }
        true
    }

    pub(super) fn paint_intersection_preview(&self, ui: &egui::Ui, rect: egui::Rect) {
        if !matches!(self.map_tool, MapTool::Intersection(_)) {
            return;
        }
        let Some(pointer) = ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p)) else {
            return;
        };
        let position = self.intersection_pointer(rect, pointer);
        let Some((draft, center, blocked)) = self.intersection_draft(position) else {
            return;
        };
        let projection = map::project(&draft);
        let Some(intersection) = map::intersections(&draft, &projection)
            .into_iter()
            .find(|i| i.point_ref == center)
        else {
            return;
        };
        let Some(paint) = intersection_paint(
            &draft,
            &projection,
            &intersection,
            egui::Rect::from_center_size(egui::Pos2::ZERO, egui::Vec2::ZERO),
            egui::Vec2::ZERO,
            1.0,
        ) else {
            return;
        };
        let screen = |p: egui::Pos2| {
            let point = WorldPoint { x: p.x, y: -p.y };
            if self.camera_view == CameraView::Top {
                screen_of(rect, self.map_pan, self.map_zoom, point)
            } else {
                iso_screen(
                    rect,
                    self.map_pan,
                    self.map_zoom,
                    self.orbit_target,
                    self.orbit_angle,
                    self.orbit_elevation,
                    point,
                    if blocked.is_some() {
                        terrain_placement::height_with_base(
                            &self.program,
                            self.path.as_deref().and_then(Path::parent),
                            position,
                        )
                    } else {
                        0.0
                    },
                )
            }
        };
        let palette = editor_palette(ui.ctx().theme());
        let color = if blocked.is_some() {
            Color32::RED
        } else {
            palette.map_preview
        };
        let painter = ui.painter_at(rect);
        let corners = [
            paint.square.left_top(),
            paint.square.right_top(),
            paint.square.right_bottom(),
            paint.square.left_bottom(),
        ];
        painter.add(egui::Shape::convex_polygon(
            corners.into_iter().map(screen).collect(),
            color.gamma_multiply(0.2),
            egui::Stroke::new(2.0, color),
        ));
        for port in intersection.ports {
            let p = screen(egui::pos2(port.position.x, -port.position.y));
            painter.circle_filled(p, 4.0, color);
            let tip = screen(egui::pos2(
                port.position.x + port.direction.x * 0.12,
                -port.position.y - port.direction.y * 0.12,
            ));
            painter.line_segment([p, tip], egui::Stroke::new(2.0, color));
        }
        painter.text(
            pointer + egui::vec2(12.0, 16.0),
            egui::Align2::LEFT_TOP,
            if let Some(reason) = blocked {
                reason
            } else {
                "Click to place · R rotate · Esc cancel"
            },
            egui::FontId::proportional(12.0),
            color,
        );
    }
}

/// A standalone placement must not masquerade as an automatic road split.
fn intersection_site_occupied(program: &Program, center: WorldPoint, half: f32) -> bool {
    let square = [
        [center.x - half, center.y - half],
        [center.x + half, center.y - half],
        [center.x + half, center.y + half],
        [center.x - half, center.y + half],
    ];
    let projection = map::project(program);
    for index in 0..projection.segments.len() {
        let Some((rows, resolved)) =
            opencity::road_geometry::RoadGeometry::placement_cross_sections(
                program,
                &projection,
                index,
            )
        else {
            return true;
        };
        if !resolved {
            return true;
        }
        for pair in rows.windows(2) {
            let (a, b) = (&pair[0].boundaries, &pair[1].boundaries);
            if let (Some(al), Some(ar), Some(bl), Some(br)) =
                (a.first(), a.last(), b.first(), b.last())
            {
                let footprint = [al, ar, br, bl].map(|v| [v.point.x, v.point.y]);
                if terrain_placement::overlaps(&square, &footprint) {
                    return true;
                }
            }
        }
    }
    let origin = egui::Rect::from_center_size(egui::Pos2::ZERO, egui::Vec2::ZERO);
    map::intersections(program, &projection)
        .iter()
        .any(|intersection| {
            intersection_paint(
                program,
                &projection,
                intersection,
                origin,
                egui::Vec2::ZERO,
                1.0,
            )
            .is_some_and(|paint| {
                let r = paint.square;
                let footprint = [
                    r.left_top(),
                    r.right_top(),
                    r.right_bottom(),
                    r.left_bottom(),
                ]
                .map(|p| [p.x, -p.y]);
                terrain_placement::overlaps(&square, &footprint)
            })
        })
}
