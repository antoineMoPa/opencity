//! Viewport iso for the city editor.
use super::*;

impl RoadEditor {
    pub(super) fn isometric_canvas(&mut self, root: &mut egui::Ui) {
        egui::Frame::NONE.show(root, |ui| {
            let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            self.map_rect = Some(rect);
            let palette = editor_palette(ui.ctx().theme());
            let projection = self.projected_map();
            if self.fit_scene_once {
                self.fit_scene_once = false;
                if !projection.segments.is_empty() {
                    let (sum_x, sum_y) = projection.segments.iter().fold((0.0, 0.0), |(x, y), segment| {
                        let middle = segment.point_at(0.5); (x + middle.x, y + middle.y)
                    });
                    let count = projection.segments.len() as f32;
                    self.orbit_target = WorldPoint { x: sum_x / count, y: sum_y / count };
                    self.map_zoom = 55.0;
                } else {
                    self.fit_network_scene(rect);
                }
            }
            if response.hovered() && !ui.ctx().egui_wants_keyboard_input() {
                if !ui.ctx().egui_wants_keyboard_input() {
                    let elevation_step = ui.input_mut(|input| {
                        if input.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowUp) {
                            ISO_ELEVATION_STEP
                        } else if input.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowDown) {
                            -ISO_ELEVATION_STEP
                        } else {
                            0.0
                        }
                    });
                    if elevation_step != 0.0 {
                        self.adjust_isometric_elevation(elevation_step);
                    }
                }
                let scroll = ui.input(|input| input.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    let pointer = ui.ctx().pointer_hover_pos().unwrap_or(rect.center());
                    let minimum_zoom=self.map_minimum_zoom();
                    iso_zoom_at_pointer(
                        rect,
                        &mut self.map_pan,
                        &mut self.map_zoom,
                        self.orbit_target,
                        self.orbit_angle, self.orbit_elevation,
                        pointer,
                        scroll,
                        minimum_zoom,
                    );
                }
            }
            let intersection_block = self.intersection_placement_input(ui, &response, rect);
            let annotation_block = self.annotation_input(ui, &response, rect);
            let box_block = self.box_select_input(ui, &response, rect, &projection, true);
            let network_block = self.network_map_ui(ui, &response, rect, &projection, true);
            let point_block = network_block || self.point_options_ui(ui, &response, rect, &projection, true);
            let spatial_block = intersection_block || point_block || box_block || annotation_block || self.minimal.pointer_over_toolbar(ui.ctx()) || self.spatial_edit_ui(ui, rect, &projection, true);
            if response.dragged_by(egui::PointerButton::Middle) && !ui.ctx().egui_wants_keyboard_input() { self.map_pan += response.drag_delta(); }
            if !spatial_block && response.drag_started_by(egui::PointerButton::Primary) {
                let drag = ui.input(|input| iso_primary_drag(input.modifiers, input.key_down(egui::Key::B)));
                self.iso_drag_pan = drag == IsoPrimaryDrag::Pan
                    || (drag == IsoPrimaryDrag::Edit && self.map_tool != MapTool::Select);
                self.iso_drag_orbit = drag == IsoPrimaryDrag::Orbit;
                if self.iso_drag_orbit {
                    self.center_isometric_orbit(rect);
                }
                if self.iso_drag_pan || self.iso_drag_orbit {
                    self.box_start = None;
                    self.dragging_endpoint = None;
                    self.dragging_control = None;
                    self.dragging_intersection = None;
                }
            }
            if !spatial_block && response.drag_started_by(egui::PointerButton::Primary)
                && self.map_tool == MapTool::Select && !self.iso_drag_pan && !self.iso_drag_orbit {
                let origin = ui.input(|input| input.pointer.press_origin())
                    .or_else(|| response.interact_pointer_pos());
                if ui.input(|input| iso_primary_drag(input.modifiers, input.key_down(egui::Key::B)) == IsoPrimaryDrag::Box) {
                    self.box_start = origin;
                } else if let Some(origin) = origin.filter(|point| rect.contains(*point)) {
                    let handles: Vec<_> = projection.segments.iter()
                        .filter(|segment| Some(segment.source) == self.selected_segment).cloned().collect();
                    self.dragging_endpoint = iso_endpoint_near(&handles,
                        &self.program, rect, self.map_pan, self.map_zoom,
                        self.orbit_target, self.orbit_angle, self.orbit_elevation, origin)
                        .or_else(|| iso_endpoint_near(&projection.segments, &self.program,
                            rect, self.map_pan, self.map_zoom, self.orbit_target,
                            self.orbit_angle, self.orbit_elevation, origin));
                    self.dragging_control = self.dragging_endpoint.is_none().then(||
                        iso_control_near(&projection.segments, rect, self.map_pan, self.map_zoom,
                            self.orbit_target, self.orbit_angle, self.orbit_elevation, origin)).flatten();
                    self.dragging_intersection = if self.dragging_endpoint.is_none()
                        && self.dragging_control.is_none() {
                        map::intersections(&self.program, &projection).into_iter()
                            .filter(|item| !item.ports.is_empty())
                            .find(|item| origin.distance(iso_screen(rect, self.map_pan,
                                self.map_zoom, self.orbit_target, self.orbit_angle, self.orbit_elevation,
                                item.position, 0.0)) <= (self.map_zoom * 0.45).clamp(18.0, 90.0))
                            .map(|item| item.point_ref)
                    } else { None };
                    if let Some((source, is_end)) = self.dragging_endpoint {
                        self.selected_segment = Some(source);
                        self.selected_segments = vec![source];
                        self.selected_endpoint = Some((source, is_end));
                        self.selected_intersection = None;
                    } else if let Some((source, _)) = self.dragging_control {
                        self.selected_segment = Some(source);
                        self.selected_segments = vec![source];
                        self.selected_endpoint = None;
                        self.selected_intersection = None;
                    } else if let Some(reference) = self.dragging_intersection.as_ref() {
                        self.selected_intersection = Some(reference.clone());
                        self.selected_segment = None;
                        self.selected_segments.clear();
                    } else {
                        self.iso_drag_pan = true;
                    }
                }
            }
            if response.dragged_by(egui::PointerButton::Primary) && self.iso_drag_pan {
                self.map_pan += response.drag_delta();
            }
            if response.dragged_by(egui::PointerButton::Primary) && self.iso_drag_orbit {
                self.orbit_angle += response.drag_delta().x * 0.008;
                self.orbit_elevation = (self.orbit_elevation - response.drag_delta().y * 0.008)
                    .clamp(MIN_ISO_ELEVATION, MAX_ISO_ELEVATION);
            }
            if response.dragged_by(egui::PointerButton::Primary) && !self.iso_drag_pan && !self.iso_drag_orbit
                && self.box_start.is_none() {
                if let Some(pointer) = response.interact_pointer_pos() {
                    if let Some((source, is_end)) = self.dragging_endpoint {
                        if let Some(segment) = projection.segments.iter().find(|segment| segment.source == source) {
                            let height = if is_end { segment.end_z } else { segment.start_z };
                            let point = iso_world_at_height(rect, self.map_pan, self.map_zoom,
                                self.orbit_target, self.orbit_angle, self.orbit_elevation, pointer, height);
                            let point = if ui.input(|input| input.modifiers.shift) {
                                grid_point(point)
                            } else { point };
                            if self.drag_lane_attachment(source, is_end, pointer, rect, &projection, true) || map::set_connected_endpoint(&mut self.program, source, is_end, point) {
                                self.dirty = true;
                            }
                        }
                    } else if let Some((source, second)) = self.dragging_control {
                        if let Some(segment) = projection.segments.iter().find(|segment| segment.source == source) {
                            let height = segment.height_at(if second { 2.0 / 3.0 } else { 1.0 / 3.0 });
                            let point = iso_world_at_height(rect, self.map_pan, self.map_zoom,
                                self.orbit_target, self.orbit_angle, self.orbit_elevation, pointer, height);
                            let point = if ui.input(|input| input.modifiers.shift) {
                                grid_point(point)
                            } else { point };
                            if map::set_control(&mut self.program, source, second, point) {
                                self.dirty = true;
                            }
                        }
                    } else if let Some(reference) = self.dragging_intersection.as_deref() {
                        let previous = pointer - response.drag_delta();
                        let a = iso_ground_point(rect, self.map_pan, self.map_zoom,
                            self.orbit_target, self.orbit_angle, self.orbit_elevation, previous);
                        let b = iso_ground_point(rect, self.map_pan, self.map_zoom,
                            self.orbit_target, self.orbit_angle, self.orbit_elevation, pointer);
                        if map::translate_intersection(&mut self.program, reference,
                            WorldPoint { x: b.x - a.x, y: b.y - a.y }) {
                            self.dirty = true;
                        }
                    }
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                self.iso_drag_pan = false;
                self.iso_drag_orbit = false;
                if let (Some(start), Some(end)) = (self.box_start.take(), response.interact_pointer_pos()) {
                    let box_rect = egui::Rect::from_two_pos(start, end);
                    self.selected_segments = projection.segments.iter().filter(|segment| {
                        let path: Vec<_> = (0..=64).map(|step| {
                            let t = step as f32 / 64.0;
                            iso_screen(rect, self.map_pan, self.map_zoom,
                                self.orbit_target, self.orbit_angle, self.orbit_elevation,
                                segment.point_at(t), segment.height_at(t))
                        }).collect();
                        path.windows(2).any(|pair| line_crosses_rect(pair[0], pair[1], box_rect))
                    }).map(|segment| segment.source).collect();
                    self.selected_segment = self.selected_segments.last().copied();
                    self.focus_orbit_on_selection();
                    self.status = format!("Selected {} segment(s)", self.selected_segments.len());
                }
                if let (Some((source, is_end)), Some(pointer)) =
                    (self.dragging_endpoint.filter(|(source, is_end)| !self.endpoint_attached(*source, *is_end)), response.interact_pointer_pos()) {
                    let current = map::project(&self.program);
                    let port = map::intersections(&self.program, &current).into_iter()
                        .flat_map(|intersection| intersection.ports.into_iter()
                            .map(move |port| (port, intersection.road_name.clone())))
                        .find(|(port, _)| pointer.distance(iso_screen(rect, self.map_pan,
                            self.map_zoom, self.orbit_target, self.orbit_angle, self.orbit_elevation,
                            port.position, 0.0)) <= 10.0);
                    if let Some((port, road_name)) = port {
                        let matching = current.segments.iter().any(|segment|
                            segment.source == source && segment.road_name == road_name);
                        if matching && map::attach_endpoint_to_port(&mut self.program,
                            source, is_end, &port.point_ref) {
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
            }
            if let Some(pointer) = response.interact_pointer_pos().filter(|_| !spatial_block && response.clicked_by(egui::PointerButton::Primary)) {
                match self.map_tool {
                    MapTool::Select => {
                        if let Some((source, is_end)) = iso_endpoint_near(&projection.segments,
                            &self.program, rect, self.map_pan, self.map_zoom,
                            self.orbit_target, self.orbit_angle, self.orbit_elevation, pointer) {
                            self.selected_segment = Some(source);
                            self.selected_segments = vec![source];
                            self.selected_endpoint = Some((source, is_end));
                            self.selected_intersection = None;
                            self.segment_name_draft = projection.segments.iter()
                                .find(|item| item.source == source)
                                .and_then(|item| item.name.clone()).unwrap_or_default();
                            self.status = "Road end selected".into();

                        } else {
                        let port = map::intersections(&self.program, &projection).into_iter()
                            .flat_map(|intersection| intersection.ports.into_iter()
                                .map(move |port| (port, intersection.road_name.clone())))
                            .find(|(port, _)| pointer.distance(iso_screen(rect, self.map_pan,
                                self.map_zoom, self.orbit_target, self.orbit_angle, self.orbit_elevation,
                                port.position, 0.0)) <= 10.0);
                        if let Some((port, road_name)) = port {
                            if !map::port_in_use(&projection, &port.point_ref) {
                                self.selected_road = self.program.statements.iter().position(|statement|
                                    matches!(statement, Statement::Road(road) if road.name == road_name));
                                self.road_name_draft = road_name;
                                self.map_tool = MapTool::Draw;
                                self.map_start = Some(port.position);
                                self.map_start_ref = Some(port.point_ref);
                                self.selected_segment = None;
                                self.selected_intersection = None;
                                #[cfg(not(target_arch = "wasm32"))]
                                { self.connection_id = None; }
                                #[cfg(not(target_arch = "wasm32"))]
                                { self.connection_member = None; }
                                self.status = "Drawing from port".into();
                            } else {
                                self.status = "This intersection port already has a road".into();
                            }
                        } else {
                            let intersection = map::intersections(&self.program, &projection).into_iter()
                                .filter(|intersection| !intersection.ports.is_empty())
                                .find(|intersection| pointer.distance(iso_screen(rect, self.map_pan,
                                    self.map_zoom, self.orbit_target, self.orbit_angle, self.orbit_elevation,
                                    intersection.position, 0.0)) <= (self.map_zoom * 0.45).clamp(18.0, 90.0));
                            if let Some(intersection) = intersection {
                                self.selected_intersection = Some(intersection.point_ref);
                                self.selected_segment = None;
                                self.selected_segments.clear();
                            } else {
                                let selected = iso_segment_near(&projection.segments, rect,
                                    self.map_pan, self.map_zoom, self.orbit_target,
                                    self.orbit_angle, self.orbit_elevation, pointer);
                                self.selected_segment = selected;
                                self.selected_segments = self.selected_segment.into_iter().collect();
                                self.selected_intersection = None;
                                self.selected_endpoint = None;
                                self.segment_name_draft = projection.segments.iter()
                                    .find(|segment| Some(segment.source) == selected)
                                    .and_then(|segment| segment.name.clone()).unwrap_or_default();
                            }

                        }
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        self.connection_selected_in_viewport();
                    }
                    MapTool::Intersection(_) | MapTool::BoxSelect | MapTool::AnnotationSegment | MapTool::AnnotationText => {}
                    MapTool::Draft => {
                        let point = iso_ground_point(rect, self.map_pan, self.map_zoom,
                            self.orbit_target, self.orbit_angle, self.orbit_elevation, pointer);
                        if let Some(start) = self.map_start {
                            if start != point {
                                if let Some(road) = self.road().cloned() {
                                    let index = map::append(&mut self.program, &road, start, point);
                                    self.selected_segment = Some(SegmentSource::TopLevel(index));
                                    self.selected_segments = vec![SegmentSource::TopLevel(index)];
                                    self.dirty = true;
                                }
                            }
                        }
                        self.map_start = Some(point);
                        self.status = "Drawing road".into();
                    }
                    MapTool::Draw => {
                        let Some(placement_click_before) = self.begin_road_click() else { return; };
                        let (free_point, _) = terrain_placement::iso_pick(&self.program, |height|
                            iso_world_at_height(rect, self.map_pan, self.map_zoom,
                                self.orbit_target, self.orbit_angle, self.orbit_elevation, pointer, height));
                        let target_endpoint = self.draw_endpoint_near(&projection, rect, pointer, true);
                        let element_end = target_endpoint.is_none().then(|| iso_element_end_near(
                            &self.draw_element_ends(&projection, rect, true), &projection,
                            rect, self.map_pan, self.map_zoom, self.orbit_target,
                            self.orbit_angle, self.orbit_elevation, pointer)).flatten();
                        let port = map::intersections(&self.program, &projection).into_iter()
                            .flat_map(|intersection| intersection.ports.into_iter()
                                .map(move |port| (port, intersection.road_name.clone())))
                            .find(|(port, _)| pointer.distance(iso_screen(rect, self.map_pan,
                                self.map_zoom, self.orbit_target, self.orbit_angle, self.orbit_elevation,
                                port.position, 0.0)) <= 10.0);
                        let picked_port = port.as_ref().filter(|(port, road_name)|
                            !map::port_in_use(&projection, &port.point_ref)
                            && terrain_placement::height(&self.program, port.position) <= 0.0
                            && self.road().is_some_and(|road| road.name == *road_name))
                            .map(|(port, _)| (port.position, Some(port.point_ref.clone())));
                        let mut picked = picked_port.or_else(|| target_endpoint.and_then(|(source, is_end)|
                            map::link_endpoint(&mut self.program, source, is_end)
                                .map(|(point, reference)| (point, Some(reference)))))
                            .or_else(|| element_end.map(|candidate| (candidate.position, None)))
                            .unwrap_or((free_point, None));
                        if self.map_start != Some(picked.0) {
                            if let Some(candidate) = element_end {
                                picked.1 = map::create_lane_point(&mut self.program,
                                    candidate.source, candidate.element_index, candidate.t)
                                    .map(|(reference, _)| reference);
                            }
                        }
                        let (point, point_ref) = picked;
                        if let Some(start) = self.map_start {
                            if start != point {
                                if let Some(road) = self.road().cloned() {
                                    let faces = self.map_start_ref.as_deref().is_none_or(|reference|
                                        map::port_faces_road(&self.program, reference, point))
                                        && point_ref.as_deref().is_none_or(|reference|
                                            map::port_faces_road(&self.program, reference, start));
                                    if faces {
                                        let pending_road = self.minimal.pending_road.clone();
                                        let selected_road = self.selected_road;
                                        if self.ensure_placement_road_type().is_none() { return; }
                                        let branch_ref = self.map_start_ref.clone();
                                        let exit = branch_ref.as_deref()
                                            .filter(|reference| map::anchored_reference(&self.program, reference))
                                            .and_then(|reference| map::append_exit(&mut self.program,
                                                &road, self.branch_mode, start, reference, point,
                                                point_ref.as_deref()));
                                        let was_exit = exit.is_some();
                                        let (index, end_ref) = if let Some(exit) = exit {
                                            map::create_two_segment_split(&mut self.program,
                                                branch_ref.as_deref().unwrap(), exit.entry,
                                                exit.full, self.branch_mode);
                                            let SegmentSource::TopLevel(index) = exit.full else { unreachable!() };
                                            (index, exit.end_ref)
                                        } else {
                                            let result = map::append_connected(&mut self.program,
                                                &road, start, self.map_start_ref.as_deref(), point,
                                                point_ref.as_deref());
                                            if let Some(reference) = branch_ref.as_deref() {
                                                map::create_lane_split(&mut self.program, reference,
                                                    SegmentSource::TopLevel(result.0), self.branch_mode);
                                            }
                                            result
                                        };
                                        if !self.finish_terrain_road(index, point, point_ref.as_deref(), pending_road, selected_road, placement_click_before) { return; }
                                        self.selected_segment = Some(SegmentSource::TopLevel(index));
                                        self.selected_segments = vec![SegmentSource::TopLevel(index)];
                                        if was_exit {
                                            if let Some(name) = map::project(&self.program).segments.iter()
                                                .find(|segment| segment.source == SegmentSource::TopLevel(index))
                                                .map(|segment| segment.road_name.clone()) {
                                                self.selected_road = self.program.statements.iter().position(|statement|
                                                    matches!(statement, Statement::Road(road) if road.name == name));
                                                self.road_name_draft = name;
                                            }
                                        }
                                        self.segment_name_draft = map::project(&self.program).segments.iter()
                                            .find(|segment| segment.source == SegmentSource::TopLevel(index))
                                            .and_then(|segment| segment.name.clone()).unwrap_or_default();
                                        let _ = end_ref;
                                        self.map_start = None;
                                        self.map_start_ref = None;
                                        self.map_tool = MapTool::Select;
                                        self.dirty = true;
                                        self.status = "Road placed".into();
                                        if self.map_start_ref.as_deref().is_some_and(|reference|
                                            map::intersection_port_direction(&self.program, reference).is_some()) {
                                            self.map_start = None;
                                            self.map_start_ref = None;
                                            self.map_tool = MapTool::Select;
                                            self.status = "Road connected to intersection port".into();
                                        }
                                    } else {
                                        self.status = "Draw away from the intersection face of each port".into();
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
                    MapTool::Landscape(_) => {
                        let point = iso_ground_point(rect, self.map_pan, self.map_zoom,
                            self.orbit_target, self.orbit_angle, self.orbit_elevation, pointer);
                        self.landscape_points.push(point);
                        self.status = format!("{} points placed",
                            self.landscape_points.len());
                    }
                }
            }
            if !ui.ctx().egui_wants_keyboard_input() && ui.input(|input| input.key_pressed(egui::Key::Enter))
                && matches!(self.map_tool, MapTool::Landscape(_)) {
                self.finish_landscape();
            } else if !ui.ctx().egui_wants_keyboard_input() && ui.input(|input| input.key_pressed(egui::Key::Escape)
                || input.key_pressed(egui::Key::Enter)) {
                self.rollback_road_placement();
                if let Some(reference) = self.map_start_ref.as_deref() {
                    if map::anchored_reference(&self.program, reference) {
                        map::remove_unused_point(&mut self.program, reference);
                    }
                }
                self.map_start = None;
                self.map_start_ref = None;
                self.landscape_points.clear();
                self.dragging_endpoint = None;
                self.dragging_control = None;
                self.dragging_intersection = None;
                self.map_tool = MapTool::Select;
            }
            if self.map_tool == MapTool::Select && !ui.ctx().egui_wants_keyboard_input() {
                if let Some((source, is_end)) = self.selected_endpoint {
                    if ui.input(|input| input.key_pressed(egui::Key::E)) {
                        self.start_extension(source, is_end);
                    } else if let Some(reverse) = ui.input(|input|
                        input.key_pressed(egui::Key::R).then_some(input.modifiers.shift)) {
                        if map::rotate_endpoint(&mut self.program, source, is_end,
                            if reverse { 15.0 } else { -15.0 }) {
                            self.dirty = true;
                            self.status = "Road end rotated 15°".into();
                        }
                    }
                }
            }
            if self.map_tool == MapTool::Select && !ui.ctx().egui_wants_keyboard_input()
                && ui.input(|input| input.key_pressed(egui::Key::Backspace)
                    || input.key_pressed(egui::Key::Delete)) {
                if let Some(reference) = self.selected_intersection.take() {
                    if map::remove_placed_intersection(&mut self.program, &reference) {
                        self.dirty = true;
                        self.status = "Intersection removed".into();
                    }
                } else {
                    self.delete_selected_segments();
                }
            }
            let projection = self.projected_map();
            let painter = ui.painter_at(rect);
            let scene = self.scene_geometry(ui.ctx(), palette, &projection);
            painter.add(egui::Shape::Callback(eframe::egui_wgpu::Callback::new_paint_callback(
                rect, scene3d::SceneCallback {
 transition: self.camera_transition_matrix(rect),
                    scene, rect, pan: self.map_pan, zoom: self.map_zoom,
                    target: self.orbit_target, angle: self.orbit_angle, elevation: self.orbit_elevation,
                    eye_height: 0.0,
                    camera: CameraView::Isometric, background: palette.map_background,
                    target_format: self.render_format,
                })));
            self.paint_selection_overlay(&painter, rect, &projection, palette, CameraView::Isometric);
            self.paint_annotations(ui, rect);
            self.paint_network_overlays(ui, rect, true);
            self.paint_intersection_preview(ui, rect);
            if self.point_options.is_none() && matches!(self.map_tool, MapTool::Draw | MapTool::Select) {
                if let Some(pointer) = ui.ctx().pointer_hover_pos().filter(|point| rect.contains(*point)) {
                    let candidates = element_end_candidates(&projection, &self.program);
                    let nearby = nearest_visible_points(
                        candidates.iter().enumerate().filter_map(|(index, candidate)| {
                            let segment = projection.segments.iter()
                                .find(|segment| segment.source == candidate.source)?;
                            Some((index, iso_screen(rect, self.map_pan, self.map_zoom,
                                self.orbit_target, self.orbit_angle, self.orbit_elevation,
                                candidate.position, segment.height_at(candidate.t))))
                        }), rect, pointer);
                    let hovered = iso_element_end_near(&candidates, &projection, rect,
                        self.map_pan, self.map_zoom, self.orbit_target, self.orbit_angle, self.orbit_elevation, pointer)
                        .filter(|candidate| nearby.iter().any(|index| {
                            let shown = candidates[*index];
                            shown.source == candidate.source
                                && shown.element_index == candidate.element_index
                                && shown.t == candidate.t
                        }));
                    for index in nearby {
                        let candidate = candidates[index];
                        let Some(segment) = projection.segments.iter()
                            .find(|segment| segment.source == candidate.source) else { continue };
                        let screen = iso_screen(rect, self.map_pan, self.map_zoom,
                            self.orbit_target, self.orbit_angle, self.orbit_elevation, candidate.position,
                            segment.height_at(candidate.t));
                        let active = hovered.is_some_and(|item| item.source == candidate.source
                            && item.element_index == candidate.element_index && item.t == candidate.t);
                        painter.circle_stroke(screen, if active { 6.0 } else { 3.5 },
                            egui::Stroke::new(1.5, palette.map_preview));
                    }
                    if let Some(candidate) = hovered {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        let screen = iso_screen(rect, self.map_pan, self.map_zoom,
                            self.orbit_target, self.orbit_angle, self.orbit_elevation, candidate.position,
                            projection.segments.iter().find(|segment| segment.source == candidate.source)
                                .map(|segment| segment.height_at(candidate.t)).unwrap_or(0.0));
                        painter.text(screen + egui::vec2(11.0, -12.0),
                            egui::Align2::LEFT_BOTTOM,
                            format!("{} {} end{}", kind_label(candidate.kind), candidate.element_index + 1, if self.map_tool == MapTool::Select { " | click for options" } else { "" }),
                            egui::FontId::proportional(11.0), palette.map_text);
                    }
                }
            }
            if let MapTool::Landscape(kind) = self.map_tool {
                let mut points: Vec<_> = self.landscape_points.iter().map(|point|
                    iso_screen(rect, self.map_pan, self.map_zoom, self.orbit_target,
                        self.orbit_angle, self.orbit_elevation, *point, 0.0)).collect();
                if let Some(pointer) = ui.ctx().pointer_hover_pos().filter(|point| rect.contains(*point)) {
                    points.push(pointer);
                }
                if points.len() >= 2 {
                    painter.add(egui::Shape::line(points.clone(),
                        egui::Stroke::new(2.0, palette.map_preview)));
                    if kind != LandscapeKind::River && self.landscape_points.len() >= 3 {
                        painter.line_segment([points[0], *points.last().unwrap()],
                            egui::Stroke::new(1.0, palette.map_preview));
                    }
                }
            }
            if let (Some(start), Some(pointer)) = (self.box_start, response.interact_pointer_pos()) {
                painter.rect_stroke(egui::Rect::from_two_pos(start, pointer), 0.0,
                    egui::Stroke::new(1.5, palette.map_selected), egui::StrokeKind::Inside);
            }
            if let (Some(start), Some(pointer)) = (self.map_start, ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p))) {
                if self.map_tool == MapTool::Draw {
                    if let Some(segment) = self.terrain_draw_preview(&projection, rect, pointer) {
                        let points = (0..=32).map(|i| {
                            let t = i as f32 / 32.0;
                            iso_screen(rect,self.map_pan,self.map_zoom,self.orbit_target,
                                self.orbit_angle,self.orbit_elevation,segment.point_at(t),segment.height_at(t))
                        }).collect();
                        painter.add(egui::Shape::line(points,egui::Stroke::new(2.0,palette.map_preview)));
                    }
                } else {
                    painter.line_segment([iso_screen(rect, self.map_pan, self.map_zoom, self.orbit_target,
                        self.orbit_angle, self.orbit_elevation, start, 0.0), pointer], egui::Stroke::new(2.0, palette.map_preview));
                }
            }
        });
    }
}
