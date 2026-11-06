//! A passive road-name readout in the viewport corner.
use super::*;
use camera_transition::ClipMatrix;

impl RoadEditor {
    pub(super) fn paint_hovered_road_name(&mut self, ui: &egui::Ui) {
        let Some(rect) = self.map_rect else { return };
        let Some(pointer) = ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p)) else {
            return;
        };
        if self.minimal.input_suspended
            || self.pending.is_some()
            || self.camera_transition.is_some()
            || ui.ctx().layer_id_at(pointer) != Some(ui.layer_id())
        {
            return;
        }
        let projection = self.projected_map();
        let Some(index) = road_under_pointer(
            &projection,
            &self.program,
            self.camera_clip_matrix(rect),
            rect,
            pointer,
        ) else {
            return;
        };
        let road = &projection.segments[index];
        let leaf = road
            .name
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&road.road_name);
        let routes = road.route_labels.join(" / ");
        let conflicting = projection
            .road_names
            .segment_groups
            .get(index)
            .and_then(|&group| projection.road_names.groups.get(group))
            .is_some_and(|group| {
                group.state == opencity::road_labels::RoadNameState::ConflictingAnchors
            });
        let mut parts = Vec::new();
        if conflicting {
            parts.push("Ambiguous road name");
        }
        if !routes.is_empty() {
            parts.push(routes.as_str());
        }
        parts.extend(road.label_path.iter().map(String::as_str));
        parts.push(leaf);
        let name = parts.join(" > ");
        let painter = ui.painter().with_clip_rect(rect);
        let color = ui.visuals().text_color();
        let mut job = egui::text::LayoutJob::simple(
            name,
            egui::FontId::proportional(9.0),
            color,
            (rect.width() - 20.0).max(1.0),
        );
        job.halign = egui::Align::RIGHT;
        let galley = painter.layout_job(job);
        let bounds = egui::Rect::from_min_size(
            rect.right_bottom() - egui::vec2(8.0, 8.0) - galley.size(),
            galley.size(),
        );
        painter.rect_filled(bounds.expand(2.0), 2.0, ui.visuals().window_fill());
        // Right-aligned galleys extend left of their origin; align their rect,
        // rather than placing the origin at the background's left edge.
        let origin = bounds.min - galley.rect.min.to_vec2();
        painter.galley(origin, galley, color);
    }
}

fn projected(matrix: ClipMatrix, rect: egui::Rect, point: [f32; 3]) -> Option<(egui::Pos2, f32)> {
    let clip = matrix.map(|r| r[0] * point[0] + r[1] * point[1] + r[2] * point[2] + r[3]);
    if !clip.iter().all(|v| v.is_finite()) || clip[3] <= 0.00001 {
        return None;
    }
    let depth = clip[2] / clip[3];
    if !(0.0..=1.0).contains(&depth) {
        return None;
    }
    Some((
        rect.center()
            + egui::vec2(
                clip[0] / clip[3] * rect.width() * 0.5,
                -clip[1] / clip[3] * rect.height() * 0.5,
            ),
        depth,
    ))
}

fn road_under_pointer(
    projection: &map::Projection,
    program: &Program,
    matrix: ClipMatrix,
    rect: egui::Rect,
    pointer: egui::Pos2,
) -> Option<usize> {
    let mut best: Option<(usize, f32, f32)> = None;
    for (index, road) in projection.segments.iter().enumerate() {
        let half_width = program
            .statements
            .iter()
            .find_map(|s| match s {
                Statement::Road(r) if r.name == road.road_name => Some(
                    r.elements
                        .iter()
                        .map(|e| map::element_world_width(e.kind))
                        .sum::<f32>()
                        * 0.5,
                ),
                _ => None,
            })
            .unwrap_or(0.0);
        let sample = |t| {
            let point = road.point_at(t);
            let height = road.height_at(t);
            let (screen, depth) = projected(matrix, rect, [point.x, point.y, height])?;
            let tangent = road.tangent_at(t);
            let angle = tangent.y.atan2(tangent.x);
            let edge = projected(
                matrix,
                rect,
                [
                    point.x + angle.sin() * half_width,
                    point.y - angle.cos() * half_width,
                    height,
                ],
            )
            .map(|p| p.0.distance(screen))
            .unwrap_or(0.0);
            Some((screen, depth, edge.max(4.0)))
        };
        let mut previous = sample(0.0);
        for step in 1..=32 {
            let current = sample(step as f32 / 32.0);
            if let (Some((a, za, ra)), Some((b, zb, rb))) = (previous, current) {
                let line = b - a;
                let along = if line.length_sq() > f32::EPSILON {
                    ((pointer - a).dot(line) / line.length_sq()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let distance = pointer.distance_sq(a + line * along);
                let radius = ra + (rb - ra) * along;
                let depth = za + (zb - za) * along;
                if distance <= radius * radius
                    && best.is_none_or(|(_, z, d)| depth > z || depth == z && distance < d)
                {
                    best = Some((index, depth, distance));
                }
            }
            previous = current;
        }
    }
    best.map(|b| b.0)
}
