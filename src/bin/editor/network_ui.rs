//! Read-only inspection of explicit components and existing profile intersections.
use super::*;
use opencity::network::{self, BandKind, NetworkInstance, Point, Resolved};

pub(super) struct LegacySnapshot {
    program: Program,
    projection: map::Projection,
    intersection: map::Intersection,
}
pub(super) struct Viewer {
    title: String,
    geometry: Resolved,
    bounds: [f32; 4],
    legacy: Option<LegacySnapshot>,
    zoom: f32,
    pan: egui::Vec2,
    surfaces: bool,
    markings: bool,
    paths: bool,
    ports: bool,
    movement: Option<usize>,
}
pub(super) struct NetworkOptions {
    viewer: Viewer,
    position: Point,
    program: Program,
}
impl Viewer {
    fn explicit(instance: &NetworkInstance) -> Self {
        let geometry = instance.geometry.clone();
        let bounds = geometry.bounds();
        Self {
            title: instance.name.clone(),
            geometry,
            bounds,
            legacy: None,
            zoom: 1.,
            pan: egui::Vec2::ZERO,
            surfaces: true,
            markings: true,
            paths: true,
            ports: true,
            movement: None,
        }
    }
    fn legacy(
        program: &Program,
        projection: &map::Projection,
        intersection: map::Intersection,
    ) -> Option<Self> {
        if let Some(instance) =
            network::default_four_way_instance(program, projection, &intersection)
        {
            return Some(Self::explicit(&instance));
        }
        let paint = intersection_paint(
            program,
            projection,
            &intersection,
            egui::Rect::from_center_size(egui::Pos2::ZERO, egui::vec2(10., 10.)),
            egui::Vec2::ZERO,
            1.,
        )?;
        let half = paint.square.width() / 2.;
        Some(Self {
            title: intersection.point_ref.clone(),
            geometry: Resolved {
                band_meshes: vec![],
                ports: vec![],
                paths: vec![],
                anchors: vec![],
                movements: vec![],
                bands: vec![],
                warnings: vec![],
            },
            bounds: [-half, -half, half, half],
            legacy: Some(LegacySnapshot {
                program: program.clone(),
                projection: projection.clone(),
                intersection,
            }),
            zoom: 1.,
            pan: egui::Vec2::ZERO,
            surfaces: true,
            markings: true,
            paths: true,
            ports: true,
            movement: None,
        })
    }
}

fn color(material: network::Material, theme: egui::Theme) -> Color32 {
    let [r, g, b] = material.rgb_for_theme(theme == egui::Theme::Light);
    Color32::from_rgb(r, g, b)
}
fn segment_distance(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let delta = b - a;
    let t = if delta.length_sq() > 0. {
        ((p - a).dot(delta) / delta.length_sq()).clamp(0., 1.)
    } else {
        0.
    };
    p.distance(a + delta * t)
}
fn inside_quad(point: egui::Pos2, quad: &[egui::Pos2]) -> bool {
    let area: f32 = (0..4)
        .map(|i| {
            let a = quad[i];
            let b = quad[(i + 1) % 4];
            a.x * b.y - a.y * b.x
        })
        .sum();
    if area.abs() < 0.0001 {
        return false;
    }
    let mut positive = false;
    let mut negative = false;
    for i in 0..4 {
        let a = quad[i];
        let b = quad[(i + 1) % 4];
        let cross = (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x);
        positive |= cross > 0.0001;
        negative |= cross < -0.0001;
    }
    !(positive && negative)
}
fn clip_polygon(mut points: Vec<egui::Pos2>, rect: egui::Rect) -> Vec<egui::Pos2> {
    for (axis, bound, greater) in [
        (0, rect.left(), true),
        (0, rect.right(), false),
        (1, rect.top(), true),
        (1, rect.bottom(), false),
    ] {
        let coordinate = |p: egui::Pos2| if axis == 0 { p.x } else { p.y };
        let inside = |p: egui::Pos2| {
            if greater {
                coordinate(p) >= bound
            } else {
                coordinate(p) <= bound
            }
        };
        let mut clipped = Vec::new();
        for i in 0..points.len() {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            let ain = inside(a);
            let bin = inside(b);
            if ain {
                clipped.push(a);
            }
            if ain != bin {
                let t = (bound - coordinate(a)) / (coordinate(b) - coordinate(a));
                clipped.push(a + (b - a) * t);
            }
        }
        points = clipped;
    }
    points
}
fn draw_line(
    painter: &egui::Painter,
    points: &[Point],
    screen: &impl Fn(Point) -> egui::Pos2,
    stroke: egui::Stroke,
) {
    if points.len() > 1 {
        painter.add(egui::Shape::line(
            points.iter().copied().map(screen).collect(),
            stroke,
        ));
    }
}
pub(super) fn draw_geometry(
    painter: &egui::Painter,
    geometry: &Resolved,
    screen: &impl Fn(Point) -> egui::Pos2,
    surfaces: bool,
    markings: bool,
    paths: bool,
    ports: bool,
    movement: Option<usize>,
    theme: egui::Theme,
) {
    let mut bands: Vec<_> = geometry.bands.iter().collect();
    bands.sort_by_key(|b| b.kind == BandKind::Marking);
    for band in bands {
        if (band.kind == BandKind::Surface && !surfaces)
            || (band.kind == BandKind::Marking && !markings)
        {
            continue;
        }
        {
            let mut mesh = egui::Mesh::default();
            for quad in geometry.band_quads(band) {
                let polygon =
                    clip_polygon(quad.into_iter().map(screen).collect(), painter.clip_rect());
                if polygon.len() < 3 {
                    continue;
                }
                let first = mesh.vertices.len() as u32;
                for point in &polygon {
                    mesh.colored_vertex(*point, color(band.material, theme));
                }
                for index in 1..polygon.len() - 1 {
                    mesh.indices.extend_from_slice(&[
                        first,
                        first + index as u32,
                        first + index as u32 + 1,
                    ]);
                }
            }
            painter.add(egui::Shape::mesh(mesh));
        }
    }
    if paths {
        for path in &geometry.paths {
            draw_line(
                painter,
                &path.points,
                screen,
                egui::Stroke::new(1., Color32::from_rgb(92, 159, 195)),
            );
        }
        for (name, point) in &geometry.anchors {
            let pos = screen(*point);
            painter.circle_filled(pos, 3., Color32::from_rgb(207, 159, 236));
            painter.text(
                pos + egui::vec2(5., -6.),
                egui::Align2::LEFT_BOTTOM,
                name,
                egui::FontId::proportional(11.),
                Color32::from_rgb(207, 159, 236),
            );
        }
    }
    if let Some(selected) = movement.and_then(|i| geometry.movements.get(i)) {
        let stroke = egui::Stroke::new(3., Color32::from_rgb(101, 225, 170));
        draw_line(painter, &selected.points, screen, stroke);
        let i = selected.points.len() * 2 / 3;
        if i + 1 < selected.points.len() {
            let a = screen(selected.points[i]);
            let b = screen(selected.points[i + 1]);
            let angle = (b.y - a.y).atan2(b.x - a.x);
            painter.arrow(a, egui::vec2(angle.cos(), angle.sin()) * 18., stroke);
        }
    }
    if ports {
        for port in &geometry.ports {
            let pos = screen(port.position);
            let hue = if port.role == network::LaneRole::In {
                Color32::from_rgb(100, 219, 171)
            } else {
                Color32::from_rgb(242, 180, 105)
            };
            let direction = port.heading.vector();
            let next = screen(Point {
                x: port.position.x + direction[0],
                y: port.position.y + direction[1],
                z: port.position.z,
            });
            let angle = (next.y - pos.y).atan2(next.x - pos.x);
            painter.circle_filled(pos, 4., hue);
            painter.arrow(
                pos,
                egui::vec2(angle.cos(), angle.sin()) * 17.,
                egui::Stroke::new(1.5, hue),
            );
            painter.text(
                pos + egui::vec2(6., -8.),
                egui::Align2::LEFT_BOTTOM,
                format!("{}.{}", port.name, port.role.keyword()),
                egui::FontId::proportional(11.),
                hue,
            );
        }
    }
}

impl RoadEditor {
    pub(super) fn map_minimum_zoom(&self) -> f32 {
        if self
            .program
            .statements
            .iter()
            .any(|s| matches!(s, Statement::Network(_)))
        {
            MIN_COMPONENT_MAP_ZOOM
        } else {
            MIN_MAP_ZOOM
        }
    }
    pub(super) fn network_world_bounds(&mut self) -> Option<[f32; 4]> {
        let networks = self.projected_networks();
        let mut bounds = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for instance in &networks.instances {
            let b = instance.geometry.bounds();
            for point in [
                Point {
                    x: b[0],
                    y: b[1],
                    z: 0.,
                },
                Point {
                    x: b[2],
                    y: b[3],
                    z: 0.,
                },
            ] {
                let p = instance.world_point(point);
                bounds[0] = bounds[0].min(p.x);
                bounds[1] = bounds[1].min(p.y);
                bounds[2] = bounds[2].max(p.x);
                bounds[3] = bounds[3].max(p.y);
            }
        }
        bounds[0].is_finite().then_some(bounds)
    }
    pub(super) fn fit_network_scene(&mut self, rect: egui::Rect) {
        let Some(bounds) = self.network_world_bounds() else {
            return;
        };
        self.orbit_target = WorldPoint {
            x: (bounds[0] + bounds[2]) / 2.,
            y: (bounds[1] + bounds[3]) / 2.,
        };
        self.map_pan = egui::Vec2::ZERO;
        let networks = self.projected_networks();
        let mut projected = egui::Rect::NOTHING;
        for instance in &networks.instances {
            for p in instance
                .geometry
                .paths
                .iter()
                .flat_map(|path| &path.points)
                .chain(instance.geometry.ports.iter().map(|port| &port.position))
            {
                let p = instance.world_point(*p);
                let point = iso_screen(
                    rect,
                    egui::Vec2::ZERO,
                    1.,
                    self.orbit_target,
                    self.orbit_angle,
                    self.orbit_elevation,
                    WorldPoint { x: p.x, y: p.y },
                    p.z,
                );
                projected.extend_with(point);
            }
        }
        if projected.is_finite() {
            self.map_zoom = ((rect.width() - 80.).max(10.) / projected.width().max(0.1))
                .min((rect.height() - 80.).max(10.) / projected.height().max(0.1))
                .clamp(MIN_COMPONENT_MAP_ZOOM, 120.);
        }
    }
    pub(super) fn projected_networks(&mut self) -> Arc<network::NetworkProjection> {
        if let Some((program, projection)) = &self.minimal.networks_cache {
            if program == &self.program {
                return Arc::clone(projection);
            }
        }
        let projection = Arc::new(network::project(&self.program));
        self.minimal.networks_cache = Some((self.program.clone(), Arc::clone(&projection)));
        projection
    }
    pub(super) fn network_map_ui(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        projection: &map::Projection,
        iso: bool,
    ) -> bool {
        if self.map_tool != MapTool::Select || self.minimal.input_suspended {
            self.minimal.network_options = None;
            return false;
        }
        if let Some(options) = self.minimal.network_options.take() {
            if options.program != self.program || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                return true;
            }
            let pos = self.spatial_screen(
                rect,
                WorldPoint {
                    x: options.position.x,
                    y: options.position.y,
                },
                options.position.z,
                iso,
            );
            let mut open = false;
            let mut close = false;
            let popup = egui::Area::new(egui::Id::new("intersection-open-options"))
                .order(egui::Order::Middle)
                .fixed_pos(pos + egui::vec2(12., 12.))
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(&options.viewer.title);
                        open = ui.button("Open intersection viewer").clicked();
                        close = ui.button("Close").clicked();
                    });
                });
            if open {
                self.point_options = None;
                self.minimal.intersection_viewer = Some(options.viewer);
            } else if !close
                && !(ui.input(|i| i.pointer.any_pressed())
                    && ui
                        .ctx()
                        .pointer_hover_pos()
                        .is_none_or(|p| !popup.response.rect.contains(p)))
            {
                self.minimal.network_options = Some(options);
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
        let networks = self.projected_networks();
        let hit = networks.instances.iter().rev().find(|instance| {
            let project = |p: Point| {
                let p = instance.world_point(p);
                self.spatial_screen(rect, WorldPoint { x: p.x, y: p.y }, p.z, iso)
            };
            let near = instance.geometry.paths.iter().any(|path| {
                path.points
                    .windows(2)
                    .any(|pair| segment_distance(pointer, project(pair[0]), project(pair[1])) <= 9.)
            });
            near || instance.geometry.bands.iter().any(|band| {
                instance.geometry.path(&band.path).is_some_and(|_| {
                    instance.geometry.band_quads(band).iter().any(|quad| {
                        let points: Vec<_> = quad.iter().copied().map(project).collect();
                        inside_quad(pointer, &points)
                    })
                })
            })
        });
        let selected = if let Some(instance) = hit {
            Some((Viewer::explicit(instance), instance.origin, None))
        } else {
            let legacy = map::intersections(&self.program, projection)
                .into_iter()
                .rev()
                .find(|intersection| {
                    if !iso {
                        return intersection_paint(
                            &self.program,
                            projection,
                            intersection,
                            rect,
                            self.map_pan,
                            self.map_zoom,
                        )
                        .is_some_and(|paint| {
                            egui::Rect::from_center_size(
                                paint.square.center(),
                                paint.square.size().max(egui::vec2(10., 10.)),
                            )
                            .contains(pointer)
                        });
                    }
                    let pos = self.spatial_screen(rect, intersection.position, 0., true);
                    pointer.distance(pos) <= 12.
                });
            legacy.and_then(|intersection| {
                let position = Point {
                    x: intersection.position.x,
                    y: intersection.position.y,
                    z: 0.,
                };
                let id = intersection.point_ref.clone();
                Viewer::legacy(&self.program, projection, intersection)
                    .map(|viewer| (viewer, position, Some(id)))
            })
        };
        if let Some((viewer, position, legacy)) = selected {
            self.point_options = None;
            self.selected_intersection = legacy;
            self.selected_segment = None;
            self.selected_segments.clear();
            self.selected_endpoint = None;
            self.minimal.network_options = Some(NetworkOptions {
                viewer,
                position,
                program: self.program.clone(),
            });
            return true;
        }
        false
    }
    pub(super) fn paint_network_overlays(&mut self, ui: &egui::Ui, rect: egui::Rect, iso: bool) {
        let networks = self.projected_networks();
        let painter = ui.painter_at(rect);
        for instance in &networks.instances {
            let screen = |p: Point| {
                let p = instance.world_point(p);
                self.spatial_screen(rect, WorldPoint { x: p.x, y: p.y }, p.z, iso)
            };
            // Surfaces use the depth-tested scene; only diagnostic lines overlay it.
            draw_geometry(
                &painter,
                &instance.geometry,
                &screen,
                false,
                false,
                true,
                false,
                None,
                ui.ctx().theme(),
            );
        }
    }
    pub(super) fn intersection_viewer_ui(&mut self, root: &mut egui::Ui) {
        let Some(mut viewer) = self.minimal.intersection_viewer.take() else {
            return;
        };
        let mut back = root.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        egui::Frame::NONE
            .inner_margin(egui::Margin::same(16))
            .show(root, |ui| {
                ui.horizontal_wrapped(|ui| {
                    back |= ui
                        .add(egui::Button::new("Back").min_size(egui::vec2(72., 32.)))
                        .clicked();
                    ui.add_space(8.);
                    ui.label(&viewer.title);
                    ui.weak("Read-only");
                    if !viewer.geometry.ports.is_empty() {
                        ui.label(format!("{} ports", viewer.geometry.ports.len()));
                    }
                });
            });
        root.separator();
        egui::Panel::right("intersection-inspector")
            .default_size(270.)
            .size_range(180.0..=360.0)
            .show(root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Layers");
                    ui.checkbox(&mut viewer.surfaces, "Surfaces");
                    ui.checkbox(&mut viewer.markings, "Markings");
                    ui.checkbox(&mut viewer.paths, "Paths and anchors");
                    ui.checkbox(&mut viewer.ports, "Ports");
                    if ui.button("Fit intersection").clicked() {
                        viewer.zoom = 1.;
                        viewer.pan = egui::Vec2::ZERO;
                    }
                    ui.separator();
                    if let Some(legacy) = &viewer.legacy {
                        ui.label(format!("{} outer ports", legacy.intersection.ports.len()));
                        ui.label(format!("Road profile: {}", legacy.intersection.road_name));
                        ui.weak("This profile intersection has no explicit movement paths.");
                    } else {
                        ui.heading(format!("Movements ({})", viewer.geometry.movements.len()));
                        if ui
                            .selectable_label(viewer.movement.is_none(), "All paths")
                            .clicked()
                        {
                            viewer.movement = None;
                        }
                        for (index, movement) in viewer.geometry.movements.iter().enumerate() {
                            let m = &movement.movement;
                            if ui
                                .selectable_label(
                                    viewer.movement == Some(index),
                                    format!("{} → {} · {}", m.from, m.to, m.class),
                                )
                                .clicked()
                            {
                                viewer.movement = Some(index);
                            }
                        }
                        if let Some(movement) = viewer
                            .movement
                            .and_then(|i| viewer.geometry.movements.get(i))
                        {
                            ui.label(format!("{} path section(s)", movement.movement.via.len()));
                            for step in &movement.movement.via {
                                ui.monospace(&step.path);
                            }
                        }
                        ui.separator();
                        ui.heading(format!("Paths ({})", viewer.geometry.paths.len()));
                        for path in &viewer.geometry.paths {
                            ui.label(format!("{} · {:.2} units", path.name, path.length()));
                        }
                        ui.separator();
                        ui.heading(format!("Ports ({})", viewer.geometry.ports.len()));
                        for port in &viewer.geometry.ports {
                            ui.label(format!(
                                "{}.{} · {} · z {:.2}",
                                port.name,
                                port.role.keyword(),
                                port.heading.keyword(),
                                port.position.z
                            ));
                        }
                        for warning in &viewer.geometry.warnings {
                            ui.colored_label(ui.visuals().warn_fg_color, warning);
                        }
                    }
                    ui.separator();
                    ui.weak("Drag to pan · Scroll to zoom · Esc to return");
                });
            });
        let (rect, response) =
            root.allocate_exact_size(root.available_size(), egui::Sense::click_and_drag());
        let b = viewer.bounds;
        let fit = ((rect.width() - 64.).max(1.) / (b[2] - b[0]).max(0.001))
            .min((rect.height() - 64.).max(1.) / (b[3] - b[1]).max(0.001));
        if response.dragged() {
            viewer.pan += response.drag_delta();
        }
        if response.hovered() {
            let scroll = root.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0. {
                let old = viewer.zoom;
                viewer.zoom = (old * (scroll * 0.002).exp()).clamp(0.1, 30.);
                let anchor =
                    root.ctx().pointer_hover_pos().unwrap_or(rect.center()) - rect.center();
                viewer.pan = anchor - (anchor - viewer.pan) * (viewer.zoom / old);
            }
        }
        let scale = fit * viewer.zoom;
        let center = egui::vec2((b[0] + b[2]) / 2., (b[1] + b[3]) / 2.);
        let screen =
            |p: Point| rect.center() + viewer.pan + (egui::vec2(p.x, p.y) - center) * scale;
        let painter = root.painter_at(rect);
        painter.rect_filled(rect, 0., root.visuals().extreme_bg_color);
        if let Some(legacy) = &viewer.legacy {
            let p = legacy.intersection.position;
            let pan = viewer.pan + egui::vec2(-p.x * scale, p.y * scale);
            if viewer.surfaces {
                paint_intersection(
                    &painter,
                    rect,
                    pan,
                    scale,
                    &legacy.program,
                    &legacy.projection,
                    &legacy.intersection,
                    editor_palette(root.ctx().theme()),
                );
            }
            if viewer.ports {
                for port in &legacy.intersection.ports {
                    let pos = screen(Point {
                        x: port.position.x - p.x,
                        y: p.y - port.position.y,
                        z: 0.,
                    });
                    painter.circle_filled(pos, 4., root.visuals().selection.stroke.color);
                    painter.text(
                        pos + egui::vec2(5., -5.),
                        egui::Align2::LEFT_BOTTOM,
                        &port.point_ref,
                        egui::FontId::proportional(11.),
                        root.visuals().text_color(),
                    );
                }
            }
        } else {
            draw_geometry(
                &painter,
                &viewer.geometry,
                &screen,
                viewer.surfaces,
                viewer.markings,
                viewer.paths,
                viewer.ports,
                viewer.movement,
                root.ctx().theme(),
            );
        }
        if !back {
            self.minimal.intersection_viewer = Some(viewer);
        }
    }
}
