//! Viewport top for the city editor.
use super::*;

impl RoadEditor {
    pub(super) fn map_canvas(&mut self, root: &mut egui::Ui) {
        if self.camera_transition.is_some() {
            self.camera_transition_canvas(root);
            return;
        }
        if self.camera_view == CameraView::Vehicular {
            self.vehicular_canvas(root);
            return;
        }
        if self.camera_view == CameraView::Isometric {
            self.isometric_canvas(root);
            return;
        }
        egui::Frame::NONE.show(root, |ui| {
            let palette = editor_palette(ui.ctx().theme());
            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            self.map_rect = Some(rect);
            let before = self.projected_map();
            if self.fit_scene_once {
                self.fit_scene_once = false;
                let landscape = project_landscape_features(&self.program);
                let networks = self.network_world_bounds();
                if !before.segments.is_empty() || !landscape.features.is_empty() || networks.is_some() {
                    let mut min = WorldPoint { x: f32::INFINITY, y: f32::INFINITY };
                    let mut max = WorldPoint { x: f32::NEG_INFINITY, y: f32::NEG_INFINITY };
                    if let Some(bounds)=networks {
                        min=WorldPoint{x:bounds[0],y:bounds[1]};max=WorldPoint{x:bounds[2],y:bounds[3]};
                    }
                    for segment in &before.segments {
                        for point in [segment.start, segment.controls[0], segment.controls[1], segment.end] {
                            min.x = min.x.min(point.x);
                            min.y = min.y.min(point.y);
                            max.x = max.x.max(point.x);
                            max.y = max.y.max(point.y);
                        }
                    }
                    for feature in &landscape.features {
                        for point in &feature.points {
                            min.x = min.x.min(point[0]); min.y = min.y.min(point[1]);
                            max.x = max.x.max(point[0]); max.y = max.y.max(point[1]);
                        }
                    }
                    let width = (max.x - min.x).max(0.1);
                    let height = (max.y - min.y).max(0.1);
                    self.map_zoom = ((rect.width() - 76.0).max(80.0) / width)
                        .min((rect.height() - 76.0).max(80.0) / height)
                        .clamp(self.map_minimum_zoom(), 120.0);
                    self.map_pan = egui::vec2(
                        -(min.x + max.x) * self.map_zoom / 2.0,
                        (min.y + max.y) * self.map_zoom / 2.0,
                    );
                }
            }
            if response.hovered() {
                let scroll = ui.input(|input| input.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    let pointer = ui.ctx().pointer_hover_pos().unwrap_or(rect.center());
                    let anchored = world_of(rect, self.map_pan, self.map_zoom, pointer);
                    self.map_zoom = (self.map_zoom * (1.0 + scroll * 0.002))
                        .clamp(self.map_minimum_zoom(), MAX_MAP_ZOOM);
                    self.map_pan = pointer
                        - rect.center()
                        - egui::vec2(anchored.x * self.map_zoom, -anchored.y * self.map_zoom);
                }
            }
            let intersection_block = self.intersection_placement_input(ui, &response, rect);
            let annotation_block = self.annotation_input(ui, &response, rect);
            let box_block = self.box_select_input(ui, &response, rect, &before, false);
            let network_block = self.network_map_ui(ui, &response, rect, &before, false);
            let point_block = network_block || self.point_options_ui(ui, &response, rect, &before, false);
            let spatial_block = intersection_block || point_block || box_block || annotation_block || self.minimal.pointer_over_toolbar(ui.ctx()) || self.spatial_edit_ui(ui, rect, &before, false);
            let junctions = junction_candidates(&self.program, &before);
            let element_ends = element_end_candidates(&before, &self.program);
            if !spatial_block && response.drag_started_by(egui::PointerButton::Primary) {
                let origin = ui
                    .input(|input| input.pointer.press_origin())
                    .or_else(|| response.interact_pointer_pos());
                if let Some(origin) = origin.filter(|point| rect.contains(*point)) {
                    let handles: Vec<_> = before.segments.iter()
                        .filter(|segment| Some(segment.source) == self.selected_segment).cloned().collect();
                    self.dragging_intersection = if self.map_tool == MapTool::Select {
                        placed_intersection_at(&self.program, &before, rect,
                            self.map_pan, self.map_zoom, origin)
                    } else { None };
                    self.dragging_endpoint = if self.map_tool == MapTool::Select {
                        self.dragging_intersection.is_none().then(|| endpoint_near(
                            &handles,
                            &self.program,
                            rect,
                            self.map_pan,
                            self.map_zoom,
                            origin,
                        ).or_else(|| endpoint_near(&before.segments, &self.program, rect,
                            self.map_pan, self.map_zoom, origin))).flatten()
                    } else {
                        None
                    };
                    self.dragging_control = if self.map_tool == MapTool::Select
                        && self.dragging_endpoint.is_none()
                        && self.dragging_intersection.is_none()
                    {
                        control_near(&before.segments, rect, self.map_pan, self.map_zoom, origin)
                    } else {
                        None
                    };
                    if let Some((source, is_end)) = self.dragging_endpoint {
                        self.selected_segment = Some(source);
                        self.selected_segments = vec![source];
                        self.selected_endpoint = Some((source, is_end));
                    }
                    if let Some((source, _)) = self.dragging_control {
                        self.selected_segment = Some(source);
                        self.selected_segments = vec![source];
                        self.selected_endpoint = None;
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
                        && self.dragging_intersection.is_none()
                        && segment_near(
                            &before.segments,
                            &self.program,
                            rect,
                            self.map_pan,
                            self.map_zoom,
                            origin,
                        )
                        .is_none();
                    if self.map_tool == MapTool::Select && ui.input(|input| input.modifiers.shift) {
                        self.box_start = Some(origin);
                        self.dragging_endpoint = None;
                        self.dragging_control = None;
                        self.dragging_intersection = None;
                        self.dragging_background = false;
                    }
                }
            }
            if response.dragged_by(egui::PointerButton::Secondary)
                || (self.dragging_background && self.box_start.is_none() && response.dragged_by(egui::PointerButton::Primary))
            {
                self.map_pan += response.drag_delta();
            }
            if ui.input(|input| {
                input.key_pressed(egui::Key::Escape)
                    || (matches!(self.map_tool, MapTool::Draw | MapTool::Draft)
                        && input.key_pressed(egui::Key::Enter))
            }) {
                self.rollback_road_placement();
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
                self.box_start = None;
                self.map_tool = MapTool::Select;
                self.landscape_points.clear();
            }
            if matches!(self.map_tool, MapTool::Landscape(_))
                && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                self.finish_landscape();
            }
            if self.map_tool == MapTool::Select
                && self.selected_intersection.is_some()
                && !ui.ctx().egui_wants_keyboard_input()
                && ui.input(|input| input.key_pressed(egui::Key::Backspace)
                    || input.key_pressed(egui::Key::Delete))
            {
                let reference = self.selected_intersection.take().unwrap();
                if map::remove_placed_intersection(&mut self.program, &reference) {
                    self.dirty = true;
                    self.status = "Intersection removed".into();
                }
            }
            if self.map_tool == MapTool::Select
                && (self.selected_segment.is_some() || !self.selected_segments.is_empty())
                && self.selected_intersection.is_none()
                && !ui.ctx().egui_wants_keyboard_input()
                && ui.input(|input| {
                    input.key_pressed(egui::Key::Backspace) || input.key_pressed(egui::Key::Delete)
                })
            {
                self.delete_selected_segments();
            }
            if self.map_tool == MapTool::Select && !ui.ctx().egui_wants_keyboard_input() {
                if let Some((source, is_end)) = self.selected_endpoint {
                    if ui.input(|input| input.key_pressed(egui::Key::E)) {
                        self.start_extension(source, is_end);
                    } else if let Some(reverse) = ui.input(|input| input.key_pressed(egui::Key::R)
                        .then_some(input.modifiers.shift)) {
                        if map::rotate_endpoint(&mut self.program, source, is_end,
                            if reverse { 15.0 } else { -15.0 }) {
                            self.dirty = true;
                            self.status = "Road end rotated 15°".into();
                        }
                    }
                }
            }
            if let Some(pointer) = ui
                .ctx()
                .pointer_hover_pos()
                .filter(|point| !spatial_block && rect.contains(*point))
            {
                let pointer_world = world_of(rect, self.map_pan, self.map_zoom, pointer);
                let free_point = if ui.input(|input| input.modifiers.shift) {
                    grid_point(pointer_world)
                } else {
                    pointer_world
                };
                if self.map_tool == MapTool::Draft && response.clicked_by(egui::PointerButton::Primary) {
                    if let Some(start) = self.map_start {
                        if start != free_point {
                            if let Some(road) = self.road().cloned() {
                                let index = map::append(&mut self.program, &road, start, free_point);
                                self.selected_segment = Some(SegmentSource::TopLevel(index));
                                self.selected_segments = vec![SegmentSource::TopLevel(index)];
                                self.dirty = true;
                            }
                        }
                    }
                    self.map_start = Some(free_point);
                    self.map_start_ref = None;
                    self.status = "Draft pen active".into();
                }
                if matches!(self.map_tool, MapTool::Landscape(_))
                    && response.clicked_by(egui::PointerButton::Primary) {
                    self.landscape_points.push(free_point);
                    self.status = format!("{} points placed",
                        self.landscape_points.len());
                }
                let port_hit = intersection_port_near(&self.program, &before, rect,
                    self.map_pan, self.map_zoom, pointer);
                let port_mismatch = self.map_tool == MapTool::Draw
                    && response.clicked_by(egui::PointerButton::Primary)
                    && port_hit.as_ref().is_some_and(|(_, _, road_name)|
                        self.road().is_some_and(|road| road.name != *road_name));
                if port_mismatch {
                    self.status = "This port needs a road with the intersection's cross section".into();
                }
                let port_busy = port_hit.as_ref().is_some_and(|(_, reference, _)|
                    map::port_in_use(&before, reference));
                if self.map_tool == MapTool::Draw && port_busy
                    && response.clicked_by(egui::PointerButton::Primary) {
                    self.status = "This intersection port already has a road".into();
                }
                let selecting_port = self.map_tool == MapTool::Select;
                if self.map_tool == MapTool::Draw
                    && response.clicked_by(egui::PointerButton::Primary)
                    && !port_mismatch
                    && !port_busy
                {
                    let Some(placement_click_before) = self.begin_road_click() else { return; };
                    let matched_port = port_hit.as_ref().filter(|(point, _, road_name)|
                        self.road().is_some_and(|road| road.name == *road_name)
                        && terrain_placement::height(&self.program, *point) <= 0.0);
                    let target_endpoint = self.draw_endpoint_near(&before, rect, pointer, false);
                    let element_end = if target_endpoint.is_none() {
                        element_end_near(&self.draw_element_ends(&before, rect, false), rect, self.map_pan, self.map_zoom, pointer)
                    } else { None };
                    let picked = matched_port.map(|(point, reference, _)| (*point, Some(reference.clone())))
                        .or_else(|| target_endpoint.and_then(|(source, is_end)|
                        map::link_endpoint(&mut self.program, source, is_end)
                            .map(|(point, reference)| (point, Some(reference)))))
                        .or_else(|| element_end.map(|candidate| (candidate.position, None)))
                        .or_else(|| junction_near(&junctions, rect, self.map_pan, self.map_zoom, pointer)
                            .filter(|(point, _)| terrain_placement::height(&self.program, *point) <= 0.0));
                    let (point, mut point_ref) = picked.unwrap_or((free_point, None));
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
                        let faces_ports = self.map_start_ref.as_deref().is_none_or(|reference|
                            map::port_faces_road(&self.program, reference, point))
                            && point_ref.as_deref().is_none_or(|reference|
                                map::port_faces_road(&self.program, reference, start));
                        if !faces_ports {
                            self.status = "Draw away from the intersection face of each port".into();
                        }
                        if start != point && faces_ports {
                            if let Some(road) = self.road().cloned() {
                                let pending_road = self.minimal.pending_road.clone();
                                let selected_road = self.selected_road;
                                if self.ensure_placement_road_type().is_none() { return; }
                                let branch_ref = self.map_start_ref.clone();
                                let exit = branch_ref.as_deref()
                                    .filter(|reference| map::anchored_reference(&self.program, reference))
                                    .and_then(|reference| map::append_exit(
                                        &mut self.program, &road, self.branch_mode, start, reference, point,
                                        point_ref.as_deref()));
                                let (index, end_ref, exit_side, through_added) = if let Some(exit) = exit {
                                    map::create_two_segment_split(&mut self.program,
                                        branch_ref.as_deref().unwrap(), exit.entry, exit.full,
                                        self.branch_mode);
                                    let SegmentSource::TopLevel(index) = exit.full else { unreachable!() };
                                    (index, exit.end_ref, Some(exit.side), exit.through.is_some())
                                } else {
                                    let (index, end_ref) = map::append_connected(
                                        &mut self.program, &road, start,
                                        self.map_start_ref.as_deref(), point,
                                        point_ref.as_deref());
                                    if let Some(reference) = branch_ref.as_deref() {
                                        map::create_lane_split(&mut self.program, reference,
                                            SegmentSource::TopLevel(index), self.branch_mode);
                                    }
                                    (index, end_ref, None, false)
                                };
                                if !self.finish_terrain_road(index, point, point_ref.as_deref(), pending_road, selected_road, placement_click_before) { return; }
                                self.selected_segment = Some(SegmentSource::TopLevel(index));
                                if exit_side.is_some() {
                                    let segment_road = map::project(&self.program).segments.into_iter()
                                        .find(|segment| segment.source == SegmentSource::TopLevel(index))
                                        .map(|segment| segment.road_name);
                                    if let Some(name) = segment_road {
                                        self.selected_road = self.program.statements.iter().position(
                                            |statement| matches!(statement, Statement::Road(road) if road.name == name));
                                        self.road_name_draft = name;
                                    }
                                }
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
                                self.status = if let Some(side) = exit_side {
                                    if through_added {
                                        format!("{} exit and divided through road placed", side.label())
                                    } else {
                                        format!("{} branch placed", side.label())
                                    }
                                } else {
                                    format!("{} placed", preset_label(&road.name))
                                };
                                if self.map_start_ref.as_deref().is_some_and(|reference|
                                    map::intersection_port_direction(&self.program, reference).is_some()) {
                                    self.map_start = None;
                                    self.map_start_ref = None;
                                    self.map_tool = MapTool::Select;
                                    self.status = "Road connected to intersection port".into();
                                }
                            }
                        }
                    } else {
                        self.map_start = Some(point);
                        self.map_start_ref = point_ref;
                        self.map_start_height = Some(terrain_placement::height(&self.program, point));
                    }
                    self.hold_road_draft();
                }
                let clicked_port = port_hit.is_some()
                    && response.clicked_by(egui::PointerButton::Primary);
                if selecting_port && self.map_tool == MapTool::Select
                    && response.clicked_by(egui::PointerButton::Primary) {
                    if let Some((position, reference, road_name)) = port_hit {
                        if map::port_in_use(&before, &reference) {
                            self.status = "This intersection port already has a road".into();
                        } else {
                        self.selected_road = self.program.statements.iter().position(|statement|
                            matches!(statement, Statement::Road(road) if road.name == road_name));
                        self.road_name_draft = road_name;
                        self.map_tool = MapTool::Draw;
                        self.map_start = Some(position);
                        self.map_start_ref = Some(reference);
                        self.selected_segment = None;
                        self.selected_intersection = None;
                        self.selected_endpoint = None;
                        #[cfg(not(target_arch = "wasm32"))]
                        { self.connection_id = None; }
                                #[cfg(not(target_arch = "wasm32"))]
                                { self.connection_member = None; }
                                self.status = "Drawing from port".into();
                        }
                    }
                }
                if !spatial_block && self.map_tool == MapTool::Select && !clicked_port {
                    let body_hit = response.clicked_by(egui::PointerButton::Primary)
                        .then(|| placed_intersection_at(&self.program, &before, rect,
                            self.map_pan, self.map_zoom, pointer)).flatten();
                    if let Some(reference) = body_hit.as_ref() {
                        self.selected_intersection = Some(reference.clone());
                        self.selected_segment = None;
                        self.selected_endpoint = None;
                    } else if response.clicked_by(egui::PointerButton::Primary) {
                        self.selected_intersection = None;
                        let picked_endpoint = endpoint_near(&before.segments, &self.program,
                            rect, self.map_pan, self.map_zoom, pointer)
                            .filter(|(source, is_end)| {
                                let segment = before.segments.iter()
                                    .find(|segment| segment.source == *source).unwrap();
                                let point = if *is_end { segment.end } else { segment.start };
                                pointer.distance(screen_of(rect, self.map_pan, self.map_zoom, point)) <= 9.0
                            });
                        if let Some((source, is_end)) = picked_endpoint {
                            self.selected_segment = Some(source);
                            self.selected_endpoint = Some((source, is_end));
                            self.segment_name_draft = before.segments.iter()
                                .find(|segment| segment.source == source)
                                .and_then(|segment| segment.name.clone()).unwrap_or_default();
                            self.status = "Road end selected".into();

                        } else if let Some(candidate) = element_end_near(&element_ends, rect,
                            self.map_pan, self.map_zoom, pointer) {
                            self.selected_segment = Some(candidate.source);
                            self.selected_endpoint = None;
                            self.segment_name_draft = before.segments.iter()
                                .find(|segment| segment.source == candidate.source)
                                .and_then(|segment| segment.name.clone()).unwrap_or_default();
                        } else {
                        let selected = segment_near(
                            &before.segments,
                            &self.program,
                            rect,
                            self.map_pan,
                            self.map_zoom,
                            pointer,
                        );
                        self.selected_segment = selected;
                        self.selected_endpoint = None;
                        self.selected_landscape = if selected.is_none() {
                            landscape_at(&self.program, rect, self.map_pan, self.map_zoom, pointer)
                        } else { None };
                        self.segment_name_draft = before
                            .segments
                            .iter()
                            .find(|segment| Some(segment.source) == self.selected_segment)
                            .and_then(|segment| segment.name.clone())
                            .unwrap_or_default();
                        }
                        self.selected_segments = self.selected_segment.into_iter().collect();

                    }
                    if response.clicked_by(egui::PointerButton::Primary) {
                        #[cfg(not(target_arch = "wasm32"))]
                        self.connection_selected_in_viewport();
                    }
                    if response.dragged_by(egui::PointerButton::Primary) {
                        if let Some(reference) = self.dragging_intersection.as_deref() {
                            let delta = response.drag_delta();
                            if map::translate_intersection(&mut self.program, reference,
                                WorldPoint { x: delta.x / self.map_zoom, y: -delta.y / self.map_zoom }) {
                                self.selected_intersection = Some(reference.to_owned());
                                self.dirty = true;
                            }
                        }
                        if let Some((source, is_end)) = self.dragging_endpoint {
                            if self.drag_lane_attachment(source, is_end, pointer, rect, &before, false) || map::set_connected_endpoint(&mut self.program, source, is_end, free_point)
                            {
                                self.dirty = true;
                            }
                        }
                        if let Some((source, second)) = self.dragging_control {
                            if map::set_control(&mut self.program, source, second, free_point) {
                                self.dirty = true;
                            }
                        }
                    }
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                if let (Some(start), Some(end)) = (self.box_start.take(), response.interact_pointer_pos()) {
                    let selection = egui::Rect::from_two_pos(start, end);
                    self.selected_segments = self.projected_map().segments.iter().cloned()
                        .filter(|segment| {
                            let path: Vec<_> = (0..=64).map(|step| screen_of(rect,
                                self.map_pan, self.map_zoom,
                                segment.point_at(step as f32 / 64.0))).collect();
                            path.windows(2).any(|pair| line_crosses_rect(pair[0], pair[1], selection))
                        })
                        .map(|segment| segment.source).collect();
                    self.selected_segment = self.selected_segments.last().copied();
                    self.focus_orbit_on_selection();
                    self.status = format!("Selected {} segment(s)", self.selected_segments.len());
                }
                if let (Some((source, is_end)), Some(pointer)) =
                    (self.dragging_endpoint.filter(|(source, is_end)| !self.endpoint_attached(*source, *is_end)), response.interact_pointer_pos()) {
                    let projection = self.projected_map();
                    if let Some((_, reference, road_name)) = intersection_port_near(
                        &self.program, &projection, rect, self.map_pan, self.map_zoom, pointer) {
                        let matching = projection.segments.iter().any(|segment|
                            segment.source == source && segment.road_name == road_name);
                        if matching && map::attach_endpoint_to_port(&mut self.program,
                            source, is_end, &reference) {
                            self.dirty = true;
                            self.status = "Road end connected to intersection port".into();
                        } else if !matching {
                            self.status = "This port needs a road with the intersection's cross section".into();
                        }
                    }
                }
                self.dragging_endpoint = None;
                self.dragging_control = None;
                self.dragging_intersection = None;
                self.dragging_background = false;
            }

            let painter = ui.painter_at(rect);
            let projection = self.projected_map();
            let scene = self.scene_geometry(ui.ctx(), palette, &projection);
            painter.add(egui::Shape::Callback(eframe::egui_wgpu::Callback::new_paint_callback(
                rect, scene3d::SceneCallback {
 transition: self.camera_transition_matrix(rect),
                    scene, rect, pan: self.map_pan, zoom: self.map_zoom,
                    target: self.orbit_target, angle: self.orbit_angle, elevation: self.orbit_elevation,
                    eye_height: 0.0,
                    camera: CameraView::Top, background: palette.map_background,
                    target_format: self.render_format,
                })));
            self.paint_selection_overlay(&painter, rect, &projection, palette, CameraView::Top);
            self.paint_annotations(ui, rect);
            self.paint_network_overlays(ui, rect, false);
            self.paint_intersection_preview(ui, rect);
            let annotation_scale = (self.map_zoom / ANNOTATION_OVERVIEW_ZOOM
                * ANNOTATION_OVERVIEW_SCALE).min(1.0);
            if let (Some(start), Some(pointer)) = (self.box_start, response.interact_pointer_pos()) {
                painter.rect_stroke(egui::Rect::from_two_pos(start, pointer), 0.0,
                    egui::Stroke::new(1.5, palette.map_selected), egui::StrokeKind::Inside);
            }
            let intersections = map::intersections(&self.program, &projection);
            let road_widths: HashMap<_, _> = self.program.statements.iter().filter_map(|statement| {
                let Statement::Road(road) = statement else { return None; };
                Some((road.name.as_str(), road.elements.iter()
                    .map(|element| map::element_world_width(element.kind)).sum::<f32>()))
            }).collect();
            let intersection_squares: Vec<_> = intersections.iter().filter_map(|intersection| {
                let mut width = *road_widths.get(intersection.road_name.as_str())?;
                for (_, road_name) in &intersection.arm_roads {
                    if let Some(arm_width) = road_widths.get(road_name.as_str()) {
                        width = width.max(*arm_width);
                    }
                }
                let center = screen_of(rect, self.map_pan, self.map_zoom, intersection.position);
                let size = width * self.map_zoom;
                Some(egui::Rect::from_center_size(center, egui::vec2(size, size)))
            }).collect();
            for intersection in &intersections {
                if self.selected_intersection.as_deref() == Some(&intersection.point_ref) {
                    if let Some(paint) = intersection_paint(&self.program, &projection,
                        intersection, rect, self.map_pan, self.map_zoom) {
                        painter.rect_stroke(paint.square, 0.0,
                            egui::Stroke::new(2.0, palette.map_selected), egui::StrokeKind::Inside);
                    }
                }
                for port in &intersection.ports {
                    let point = screen_of(rect, self.map_pan, self.map_zoom, port.position);
                    let center = screen_of(rect, self.map_pan, self.map_zoom, intersection.position);
                    let radius = (point.distance(center) * 0.35).clamp(2.5, 6.0)
                        * annotation_scale;
                    let connected = projection.segments.iter().any(|segment|
                        segment.start_ref.as_deref() == Some(&port.point_ref)
                            || segment.end_ref.as_deref() == Some(&port.point_ref));
                    painter.circle_filled(point, radius, palette.map_background);
                    painter.circle_stroke(point, radius,
                        egui::Stroke::new(2.0 * annotation_scale,
                            if connected { palette.map_subtle } else { palette.map_preview }));
                }
            }
            if let Some(segment) = projection.segments.iter()
                .find(|segment| Some(segment.source) == self.selected_segment) {
                if segment.start_z != segment.end_z {
                    for (position, height) in [(segment.start, segment.start_z),
                        (segment.end, segment.end_z)] {
                        painter.text(screen_of(rect, self.map_pan, self.map_zoom, position)
                            + egui::vec2(0.0, -12.0), egui::Align2::CENTER_BOTTOM,
                            format!("z {height:.2}"), egui::FontId::proportional(11.0),
                            palette.map_text);
                    }
                }
                for point in [segment.start, segment.end]
                    .map(|point| screen_of(rect, self.map_pan, self.map_zoom, point)) {
                    if intersection_squares.iter().any(|square| square.contains(point)) {
                        continue;
                    }
                    painter.circle_filled(point, 7.0, palette.map_text);
                    painter.circle_stroke(point, 7.0,
                        egui::Stroke::new(2.0, palette.map_selected));
                }
                for (anchor, control) in [
                    (segment.start, segment.controls[0]),
                    (segment.end, segment.controls[1]),
                ] {
                    let a = screen_of(rect, self.map_pan, self.map_zoom, anchor);
                    let b = screen_of(rect, self.map_pan, self.map_zoom, control);
                    if intersection_squares.iter().any(|square| square.contains(a)) {
                        continue;
                    }
                    painter.line_segment([a, b], egui::Stroke::new(1.0, palette.map_selected));
                    painter.circle_filled(b, 5.0, palette.map_selected);
                }
            }

            if self.point_options.is_none() && matches!(self.map_tool, MapTool::Draw | MapTool::Select) {
                if let Some(pointer) = ui
                    .ctx()
                    .pointer_hover_pos()
                    .filter(|point| rect.contains(*point))
                {
                    let element_ends: Vec<_> = element_end_candidates(&projection, &self.program)
                        .into_iter().filter(|candidate| !intersection_squares.iter().any(|square|
                            square.contains(screen_of(rect, self.map_pan, self.map_zoom,
                                candidate.position)))).collect();
                    let junctions: Vec<_> = junction_candidates(&self.program, &projection)
                        .into_iter().filter(|(point, _)| !intersection_squares.iter().any(|square|
                            square.contains(screen_of(rect, self.map_pan, self.map_zoom, *point))))
                        .collect();
                    let nearby = nearest_visible_points(
                        element_ends.iter().enumerate().map(|(index, candidate)|
                            (HoverPoint::Element(index), screen_of(rect, self.map_pan,
                                self.map_zoom, candidate.position)))
                        .chain(junctions.iter().enumerate().map(|(index, (point, _))|
                            (HoverPoint::Junction(index), screen_of(rect, self.map_pan,
                                self.map_zoom, *point)))), rect, pointer);
                    let hovered_end =
                        element_end_near(&element_ends, rect, self.map_pan, self.map_zoom, pointer)
                            .filter(|candidate| nearby.iter().any(|point| match point {
                                HoverPoint::Element(index) => {
                                    let shown = element_ends[*index];
                                    shown.source == candidate.source
                                        && shown.element_index == candidate.element_index
                                        && shown.t == candidate.t
                                }
                                HoverPoint::Junction(_) => false,
                            }));
                    for index in nearby.iter().filter_map(|point| match point {
                        HoverPoint::Element(index) => Some(*index),
                        HoverPoint::Junction(_) => None,
                    }) {
                        let candidate = &element_ends[index];
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
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        let screen =
                            screen_of(rect, self.map_pan, self.map_zoom, candidate.position);
                        painter.text(
                            screen + egui::vec2(11.0, -12.0),
                            egui::Align2::LEFT_BOTTOM,
                            format!(
                                "{} {} end{}",
                                kind_label(candidate.kind),
                                candidate.element_index + 1,
                                if self.map_tool == MapTool::Select { " | click for options" } else { "" }
                            ),
                            egui::FontId::proportional(11.0),
                            palette.map_text,
                        );
                    }
                    for index in nearby.iter().filter_map(|point| match point {
                        HoverPoint::Element(_) => None,
                        HoverPoint::Junction(index) => Some(*index),
                    }) {
                        let (point, reference) = &junctions[index];
                        let screen = screen_of(rect, self.map_pan, self.map_zoom, *point);
                        let distance = pointer.distance(screen);
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
            if let Some(start) = self.map_start {
                let a = screen_of(rect, self.map_pan, self.map_zoom, start);
                painter.circle_filled(a, 5.0, palette.map_preview);
                if let Some(pointer) = ui
                    .ctx()
                    .pointer_hover_pos()
                    .filter(|point| rect.contains(*point))
                {
                    let stroke = egui::Stroke::new(3.0, palette.map_preview);
                    let end = endpoint_near(&projection.segments, &self.program,
                            rect, self.map_pan, self.map_zoom, pointer)
                        .and_then(|(source, is_end)| projection.segments.iter()
                            .find(|segment| segment.source == source)
                            .map(|segment| if is_end { segment.end } else { segment.start }))
                        .or_else(|| element_end_near(&element_ends, rect, self.map_pan, self.map_zoom, pointer)
                            .map(|candidate| candidate.position)
                        )
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
            if let MapTool::Landscape(kind) = self.map_tool {
                let mut points: Vec<_> = self.landscape_points.iter()
                    .map(|point| screen_of(rect, self.map_pan, self.map_zoom, *point)).collect();
                if let Some(pointer) = ui.ctx().pointer_hover_pos().filter(|point| rect.contains(*point)) {
                    points.push(pointer);
                }
                if points.len() >= 2 {
                    painter.add(egui::Shape::line(points.clone(),
                        egui::Stroke::new(2.0, palette.map_preview)));
                }
                if kind != LandscapeKind::River && self.landscape_points.len() >= 3 {
                    painter.line_segment([points[0], *points.last().unwrap()],
                        egui::Stroke::new(1.0, palette.map_preview));
                }
                for point in self.landscape_points.iter().map(|point|
                    screen_of(rect, self.map_pan, self.map_zoom, *point)) {
                    painter.circle_filled(point, 4.0, palette.map_preview);
                }
            }
        });
    }
}
