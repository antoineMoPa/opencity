//! Camera for the city editor.
use super::*;

impl RoadEditor {
    pub(super) fn set_camera_view(&mut self, view: CameraView) {
        if self.camera_view == view {
            return;
        }
        let rect = self.map_rect.unwrap_or_else(|| {
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))
        });
        let from = self
            .camera_transition_matrix(rect)
            .unwrap_or_else(|| self.camera_clip_matrix(rect));
        let anchor = self.camera_visible_anchor();
        let focus = WorldPoint {
            x: anchor[0],
            y: anchor[1],
        };
        if self.camera_view != CameraView::Vehicular {
            self.minimal.last_edit_view = Some(self.camera_view);
        }
        self.camera_view = view;
        match view {
            CameraView::Top => {
                self.map_pan = egui::vec2(-focus.x * self.map_zoom, focus.y * self.map_zoom);
            }
            CameraView::Isometric => {
                self.orbit_target = focus;
                self.map_pan = egui::Vec2::ZERO;
            }
            CameraView::Vehicular => {
                let stays_local = self.minimal.vehicle_initialized
                    && (focus.x - self.vehicle_position.x).hypot(focus.y - self.vehicle_position.y)
                        < 0.0001;
                if !stays_local {
                    self.place_vehicle_near_focus(focus);
                }

                self.minimal.vehicle_initialized = true;
            }
        }
        self.fit_scene_once = false;
        self.start_camera_transition(from, anchor);
    }

    fn place_vehicle_near_focus(&mut self, focus: WorldPoint) {
        let projection = self.projected_map();
        let nearest = projection
            .segments
            .iter()
            .filter(|segment| {
                road_for_segment(&self.program, segment).is_some_and(|road| {
                    road.elements
                        .iter()
                        .any(|element| element.kind == RoadElementKind::CarLane)
                })
            })
            .flat_map(|segment| {
                (0..=128).map(move |sample| {
                    let t = sample as f32 / 128.0;
                    let point = segment.point_at(t);
                    let distance = (point.x - focus.x).powi(2) + (point.y - focus.y).powi(2);
                    (segment, t, distance)
                })
            })
            .min_by(|a, b| a.2.total_cmp(&b.2));
        if let Some((segment, t, _)) = nearest {
            self.vehicle_position = segment.point_at(t);
            // The local road becomes the grade-follow source, independent of a
            // remote editor selection that must not decide the entry location.
            let source = segment.source;

            let tangent = segment.tangent_at(t);
            self.vehicle_heading = tangent.y.atan2(tangent.x);
            self.vehicle_eye_height = segment.height_at(t) + 0.027;
            if let Some(road) = road_for_segment(&self.program, segment) {
                if let Some(lane) = road
                    .elements
                    .iter()
                    .position(|element| element.kind == RoadElementKind::CarLane)
                {
                    let profiles = paint_profiles(&self.program, &projection);
                    let index = projection
                        .segments
                        .iter()
                        .position(|item| item.source == source)
                        .unwrap();
                    let (widths, left) =
                        opencity::road_profiles::section_widths(road, &profiles[index], t);
                    let offset = left + widths[..lane].iter().sum::<f32>() + widths[lane] / 2.0;
                    self.vehicle_position.x -= self.vehicle_heading.sin() * offset;
                    self.vehicle_position.y += self.vehicle_heading.cos() * offset;
                    if road.lane_direction(lane) == Some(RoadDirection::Reverse) {
                        self.vehicle_heading += std::f32::consts::PI;
                    }
                }
            }
            self.selected_segment = Some(source);
        } else {
            self.vehicle_position = focus;
            self.vehicle_eye_height = 0.027;
        }
    }

    pub(super) fn place_vehicle_camera(&mut self) {
        let projection = self.projected_map();
        let selected = self.selected_segment.and_then(|source| {
            projection
                .segments
                .iter()
                .find(|segment| segment.source == source)
        });
        let nearest = projection.segments.iter().min_by(|a, b| {
            let distance = |segment: &map::Segment| {
                let point = segment.point_at(0.5);
                (point.x - self.orbit_target.x).powi(2) + (point.y - self.orbit_target.y).powi(2)
            };
            distance(a).total_cmp(&distance(b))
        });
        if let Some(segment) = selected.or(nearest) {
            self.vehicle_position = segment.point_at(0.5);
            let tangent = segment.tangent_at(0.5);
            self.vehicle_heading = tangent.y.atan2(tangent.x);
            if let Some(road) = road_for_segment(&self.program, segment) {
                if let Some(lane) = road
                    .elements
                    .iter()
                    .position(|element| element.kind == RoadElementKind::CarLane)
                {
                    let profiles = paint_profiles(&self.program, &projection);
                    let index = projection
                        .segments
                        .iter()
                        .position(|item| item.source == segment.source)
                        .unwrap();
                    let (widths, left) =
                        opencity::road_profiles::section_widths(road, &profiles[index], 0.5);
                    let offset = left + widths[..lane].iter().sum::<f32>() + widths[lane] / 2.0;
                    self.vehicle_position.x -= self.vehicle_heading.sin() * offset;
                    self.vehicle_position.y += self.vehicle_heading.cos() * offset;
                    if road.lane_direction(lane) == Some(RoadDirection::Reverse) {
                        self.vehicle_heading += std::f32::consts::PI;
                    }
                }
            }
            // Lane widths are around 0.03–0.045 world units; keep the eye above
            // pavement paint while giving the view a driver's sense of scale.
            self.vehicle_eye_height = segment.height_at(0.5) + 0.027;
        } else {
            self.vehicle_position = self.orbit_target;
            self.vehicle_heading = std::f32::consts::FRAC_PI_2;
            self.vehicle_eye_height = 0.027;
        }
    }

    pub(super) fn follow_selected_road_height(&mut self, projection: &map::Projection) {
        let Some(index) = projection
            .segments
            .iter()
            .position(|segment| Some(segment.source) == self.selected_segment)
        else {
            return;
        };
        let segment = &projection.segments[index];
        let Some(road) = road_for_segment(&self.program, segment) else {
            return;
        };
        let distance_squared = |t| {
            let point = segment.point_at(t);
            (point.x - self.vehicle_position.x).powi(2)
                + (point.y - self.vehicle_position.y).powi(2)
        };
        let sample = (0..=64)
            .min_by(|a, b| {
                distance_squared(*a as f32 / 64.0).total_cmp(&distance_squared(*b as f32 / 64.0))
            })
            .unwrap();
        let mut low = ((sample as f32 - 1.0) / 64.0).max(0.0);
        let mut high = ((sample as f32 + 1.0) / 64.0).min(1.0);
        for _ in 0..12 {
            let first = low + (high - low) / 3.0;
            let second = high - (high - low) / 3.0;
            if distance_squared(first) < distance_squared(second) {
                high = second;
            } else {
                low = first;
            }
        }
        let t = (low + high) / 2.0;
        let profiles = paint_profiles(&self.program, projection);
        let (widths, left) = opencity::road_profiles::section_widths(road, &profiles[index], t);
        let point = segment.point_at(t);
        let tangent = segment.tangent_at(t);
        let heading = tangent.y.atan2(tangent.x);
        let dx = self.vehicle_position.x - point.x;
        let dy = self.vehicle_position.y - point.y;
        let across = -heading.sin() * dx + heading.cos() * dy;
        let along = heading.cos() * dx + heading.sin() * dy;
        if across >= left && across <= left + widths.iter().sum::<f32>() && along.abs() < 0.001 {
            self.vehicle_eye_height = segment.height_at(t) + 0.027;
        }
    }

    pub(super) fn focus_orbit_on_selection(&mut self) {
        let projection = map::project(&self.program);
        if let Some(segment) = projection
            .segments
            .iter()
            .find(|segment| Some(segment.source) == self.selected_segment)
        {
            self.orbit_target = segment.point_at(0.5);
        } else if let Some(intersection) =
            self.selected_intersection.as_deref().and_then(|reference| {
                map::intersections(&self.program, &projection)
                    .into_iter()
                    .find(|item| item.point_ref == reference)
            })
        {
            self.orbit_target = intersection.position;
        } else if let Some(feature) = self.selected_landscape.as_ref().and_then(|name| {
            project_landscape_features(&self.program)
                .features
                .into_iter()
                .find(|feature| feature.name.as_ref() == Some(name))
        }) {
            if !feature.points.is_empty() {
                let (x, y) = feature
                    .points
                    .iter()
                    .fold((0.0, 0.0), |(x, y), point| (x + point[0], y + point[1]));
                self.orbit_target = WorldPoint {
                    x: x / feature.points.len() as f32,
                    y: y / feature.points.len() as f32,
                };
            }
        }
    }

    pub(super) fn center_isometric_orbit(&mut self, rect: egui::Rect) {
        self.orbit_target = iso_ground_point(
            rect,
            self.map_pan,
            self.map_zoom,
            self.orbit_target,
            self.orbit_angle,
            self.orbit_elevation,
            rect.center(),
        );
        self.map_pan = egui::Vec2::ZERO;
    }

    pub(super) fn adjust_isometric_elevation(&mut self, step: f32) {
        if self.camera_view != CameraView::Isometric {
            return;
        }
        if let Some(rect) = self.map_rect {
            self.center_isometric_orbit(rect);
        }
        self.orbit_elevation =
            (self.orbit_elevation + step).clamp(MIN_ISO_ELEVATION, MAX_ISO_ELEVATION);
    }
}
