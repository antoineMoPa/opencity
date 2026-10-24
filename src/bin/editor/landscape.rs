//! Landscape for the city editor.
use super::*;

impl RoadEditor {
    pub(super) fn start_landscape(&mut self, kind: LandscapeKind) {
        self.map_tool = MapTool::Landscape(kind);
        self.landscape_points.clear();
        self.map_start = None;
        self.map_start_ref = None;
        self.selected_segment = None;
        self.selected_intersection = None;
        self.selected_landscape = None;
        self.status = format!(
            "{}: click points, then press Enter to finish",
            landscape_label(kind)
        );
    }

    pub(super) fn finish_landscape(&mut self) {
        let MapTool::Landscape(kind) = self.map_tool else {
            return;
        };
        let minimum = if kind == LandscapeKind::River { 2 } else { 3 };
        if self.landscape_points.len() < minimum {
            self.status = format!("{} needs at least {minimum} points", landscape_label(kind));
            return;
        }
        let mut suffix = 1;
        let name = loop {
            let candidate = format!("{}_{}", kind.call_name(), suffix);
            let used = self
                .program
                .statements
                .iter()
                .any(|statement| match statement {
                    Statement::Call(call) => call.assigns.iter().any(|assign| assign == &candidate),
                    Statement::Road(road) => road.name == candidate,
                    Statement::Block(block) => block.name == candidate,
                });
            if !used {
                break candidate;
            }
            suffix += 1;
        };
        let mut args = Vec::with_capacity(self.landscape_points.len() * 2 + 1);
        match kind {
            LandscapeKind::Vegetation => args.push(Argument::Identifier("forest".into())),
            LandscapeKind::Terrain => args.push(Argument::Number("0.25".into())),
            LandscapeKind::Building => args.push(Argument::Number("0.25".into())),
            _ => {}
        }
        for point in &self.landscape_points {
            args.push(Argument::Number(format!("{:.4}", point.x)));
            args.push(Argument::Number(format!("{:.4}", point.y)));
        }
        self.program.statements.push(Statement::Call(Call {
            assigns: vec![name.clone()],
            name: kind.call_name().into(),
            args,
        }));
        self.selected_landscape = Some(name);
        self.landscape_points.clear();
        self.map_tool = MapTool::Select;
        self.dirty = true;
        self.status = format!("{} placed", landscape_label(kind));
    }
}

pub(super) fn landscape_screen_points(
    feature: &opencity::LandscapeFeature,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
) -> Vec<egui::Pos2> {
    feature
        .points
        .iter()
        .map(|point| {
            screen_of(
                rect,
                pan,
                zoom,
                WorldPoint {
                    x: point[0],
                    y: point[1],
                },
            )
        })
        .collect()
}

pub(super) fn point_in_landscape_polygon(points: &[egui::Pos2], point: egui::Pos2) -> bool {
    let mut inside = false;
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        if (a.y > point.y) != (b.y > point.y)
            && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
    }
    inside
}

pub(super) fn landscape_at(
    program: &Program,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<String> {
    let mut features = project_landscape_features(program).features;
    features.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
    features.iter().rev().find_map(|feature| {
        let name = feature.name.as_ref()?;
        let points = landscape_screen_points(feature, rect, pan, zoom);
        let hit = if feature.kind == LandscapeKind::River {
            points.windows(2).any(|pair| {
                let segment = pair[1] - pair[0];
                let length2 = segment.length_sq();
                let t = if length2 > 0.0 {
                    ((pointer - pair[0]).dot(segment) / length2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                pointer.distance(pair[0] + segment * t) <= 8.0
            })
        } else {
            point_in_landscape_polygon(&points, pointer)
        };
        hit.then(|| name.clone())
    })
}

pub(super) fn paint_landscape_feature(
    painter: &egui::Painter,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    feature: &opencity::LandscapeFeature,
    selected: bool,
    palette: EditorPalette,
) {
    let points = landscape_screen_points(feature, rect, pan, zoom);

    let (fill, edge) = match (palette.theme, feature.kind) {
        (egui::Theme::Dark, LandscapeKind::Land) => (
            Color32::from_rgb(105, 158, 75),
            Color32::from_rgb(76, 130, 65),
        ),
        (egui::Theme::Light, LandscapeKind::Land) => (
            Color32::from_rgb(194, 224, 180),
            Color32::from_rgb(80, 139, 75),
        ),
        (egui::Theme::Dark, LandscapeKind::Beach) => (
            Color32::from_rgb(244, 219, 159),
            Color32::from_rgb(221, 190, 119),
        ),
        (egui::Theme::Light, LandscapeKind::Beach) => (
            Color32::from_rgb(232, 211, 159),
            Color32::from_rgb(189, 160, 99),
        ),
        (egui::Theme::Dark, LandscapeKind::Concrete) => (
            Color32::from_rgb(125, 139, 145),
            Color32::from_rgb(95, 110, 117),
        ),
        (egui::Theme::Light, LandscapeKind::Concrete) => (
            Color32::from_rgb(184, 191, 190),
            Color32::from_rgb(128, 142, 147),
        ),
        (egui::Theme::Dark, LandscapeKind::Vegetation) => (
            Color32::from_rgb(61, 128, 59),
            Color32::from_rgb(47, 109, 55),
        ),
        (egui::Theme::Light, LandscapeKind::Vegetation) => (
            Color32::from_rgb(140, 191, 126),
            Color32::from_rgb(49, 119, 62),
        ),
        (egui::Theme::Dark, LandscapeKind::Terrain) => (
            Color32::from_rgb(142, 160, 86),
            Color32::from_rgb(113, 137, 70),
        ),
        (egui::Theme::Light, LandscapeKind::Terrain) => (
            Color32::from_rgb(206, 188, 152),
            Color32::from_rgb(138, 112, 70),
        ),
        (egui::Theme::Dark, LandscapeKind::Building) => (
            Color32::from_rgb(192, 198, 194),
            Color32::from_rgb(115, 128, 127),
        ),
        (egui::Theme::Light, LandscapeKind::Building) => (
            Color32::from_rgb(178, 184, 189),
            Color32::from_rgb(86, 97, 105),
        ),
        (egui::Theme::Dark, LandscapeKind::Lake | LandscapeKind::River) => (
            Color32::from_rgb(15, 123, 187),
            Color32::from_rgb(36, 163, 211),
        ),
        (egui::Theme::Light, LandscapeKind::Lake | LandscapeKind::River) => (
            Color32::from_rgb(124, 190, 220),
            Color32::from_rgb(32, 120, 166),
        ),
    };
    let stroke = egui::Stroke::new(
        if selected { 3.0 } else { 1.5 },
        if selected { palette.map_selected } else { edge },
    );
    if feature.kind == LandscapeKind::River {
        painter.add(egui::Shape::line(
            points.clone(),
            egui::Stroke::new(10.0, fill),
        ));
        painter.add(egui::Shape::line(points.clone(), stroke));
    } else {
        if landscape_is_convex(&points) {
            painter.add(egui::Shape::convex_polygon(
                points.clone(),
                fill,
                egui::Stroke::NONE,
            ));
        } else {
            // A single mesh keeps internal triangle edges out of the shoreline.
            // Separate antialiased polygons leave visible seams in large areas.
            let mut mesh = egui::Mesh::default();
            for [a, b, c] in landscape_triangles(&points) {
                let base = mesh.vertices.len() as u32;
                for pos in [a, b, c] {
                    mesh.vertices.push(egui::epaint::Vertex {
                        pos,
                        uv: egui::epaint::WHITE_UV,
                        color: fill,
                    });
                }
                mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
            }
            painter.add(egui::Shape::mesh(mesh));
        }
        painter.add(egui::Shape::closed_line(points.clone(), stroke));
    }
    if selected {
        for point in points {
            painter.circle_filled(point, 4.0, palette.map_selected);
        }
    }
}

pub(super) fn landscape_is_convex(points: &[egui::Pos2]) -> bool {
    if points.len() < 3 {
        return false;
    }
    let mut direction = 0.0_f32;
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        let c = points[(index + 2) % points.len()];
        let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
        if cross.abs() <= 0.0001 {
            continue;
        }
        if direction != 0.0 && cross.signum() != direction {
            return false;
        }
        direction = cross.signum();
    }
    direction != 0.0
}

pub(super) fn landscape_triangles(points: &[egui::Pos2]) -> Vec<[egui::Pos2; 3]> {
    if points.len() < 3 {
        return Vec::new();
    }
    let area: f32 = points
        .iter()
        .enumerate()
        .map(|(index, a)| {
            let b = points[(index + 1) % points.len()];
            a.x * b.y - b.x * a.y
        })
        .sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let mut remaining: Vec<usize> = (0..points.len()).collect();
    let mut triangles = Vec::new();
    while remaining.len() > 3 {
        let mut ear = None;
        for index in 0..remaining.len() {
            let a = points[remaining[(index + remaining.len() - 1) % remaining.len()]];
            let b = points[remaining[index]];
            let c = points[remaining[(index + 1) % remaining.len()]];
            let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
            if cross * sign <= 0.0001 {
                continue;
            }
            let contains = remaining.iter().any(|candidate| {
                let p = points[*candidate];
                p != a && p != b && p != c && triangle_contains(a, b, c, p)
            });
            if !contains {
                ear = Some((index, [a, b, c]));
                break;
            }
        }
        let Some((index, triangle)) = ear else { break };
        triangles.push(triangle);
        remaining.remove(index);
    }
    if remaining.len() == 3 {
        triangles.push([
            points[remaining[0]],
            points[remaining[1]],
            points[remaining[2]],
        ]);
    }
    triangles
}

pub(super) fn triangle_contains(
    a: egui::Pos2,
    b: egui::Pos2,
    c: egui::Pos2,
    p: egui::Pos2,
) -> bool {
    let cross = |u: egui::Pos2, v: egui::Pos2, w: egui::Pos2| {
        (v.x - u.x) * (w.y - u.y) - (v.y - u.y) * (w.x - u.x)
    };
    let x = cross(a, b, p);
    let y = cross(b, c, p);
    let z = cross(c, a, p);
    (x >= 0.0 && y >= 0.0 && z >= 0.0) || (x <= 0.0 && y <= 0.0 && z <= 0.0)
}
