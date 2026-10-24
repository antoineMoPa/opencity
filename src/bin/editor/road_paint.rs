//! Road paint for the city editor.
use super::*;

pub(super) fn map_element_width(kind: RoadElementKind, zoom: f32) -> f32 {
    map::element_world_width(kind) * zoom
}

#[cfg(test)]
pub(super) fn attachment_verges(road: &RoadDefinition, lane: usize) -> Vec<usize> {
    let adjacent = if lane == 0 {
        1
    } else if lane + 1 == road.elements.len() {
        lane - 1
    } else {
        return Vec::new();
    };
    road.elements
        .get(adjacent)
        .filter(|element| element.kind == RoadElementKind::Verge)
        .map(|_| vec![adjacent])
        .unwrap_or_default()
}

pub(super) fn road_paint_order(
    program: &Program,
    projection: &map::Projection,
    profiles: &[PaintProfile],
) -> Vec<usize> {
    let mut layer_heights: Vec<f32> = projection
        .segments
        .iter()
        .map(|segment| segment.elevation)
        .collect();
    for (index, segment) in projection.segments.iter().enumerate() {
        if profiles[index].start_alignment.is_none() && profiles[index].merge_alignment.is_none() {
            continue;
        }
        for reference in [segment.start_ref.as_deref(), segment.end_ref.as_deref()] {
            let Some((parent_name, _, _)) =
                reference.and_then(|reference| map::lane_attachment(program, reference))
            else {
                continue;
            };
            if let Some(parent) = projection
                .segments
                .iter()
                .find(|parent| parent.name.as_deref() == Some(&parent_name))
            {
                // A graded ramp can average below its receiving road while meeting it
                // at the same height. Paint its mouth after that road at the join.
                layer_heights[index] = layer_heights[index].max(parent.elevation);
            }
        }
    }
    let mut order: Vec<_> = (0..projection.segments.len()).collect();
    order.sort_by(|a, b| {
        layer_heights[*a]
            .total_cmp(&layer_heights[*b])
            .then_with(|| {
                let has_join = |index: usize| {
                    profiles[index].start_alignment.is_some()
                        || profiles[index].merge_alignment.is_some()
                        || !profiles[index].start_taper.is_empty()
                        || !profiles[index].end_taper.is_empty()
                };
                has_join(*a).cmp(&has_join(*b))
            })
    });
    order
}

pub(super) fn screen_normal_at(segment: &map::Segment, t: f32) -> egui::Vec2 {
    let tangent = segment.tangent_at(t);
    let dx = tangent.x;
    let dy = -tangent.y;
    let length = (dx * dx + dy * dy).sqrt();
    if length > 0.0001 {
        egui::vec2(-dy / length, dx / length)
    } else {
        egui::vec2(0.0, 1.0)
    }
}

pub(super) fn section_widths(
    road: &RoadDefinition,
    zoom: f32,
    profile: &PaintProfile,
    t: f32,
) -> (Vec<f32>, f32) {
    let (widths, offset) = opencity::road_profiles::section_widths(road, profile, t);
    (
        widths.into_iter().map(|w| w * zoom).collect(),
        offset * zoom,
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn section_edges(road: &RoadDefinition, profile: &PaintProfile, t: f32) -> (f32, f32) {
    let (widths, left) = section_widths(road, 1.0, profile, t);
    (left, left + widths.iter().sum::<f32>())
}

pub(super) fn screen_point_in_road(
    point: egui::Pos2,
    footprint: &AttachmentFootprint,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
) -> bool {
    // The ramp boundary can land exactly on the parent's outline. Account for
    // the outline's raster width so that the coincident ramp outline is hidden.
    let radius = footprint.half_width * zoom + 0.5;
    let radius_sq = radius.powi(2);
    for segment in &footprint.segments {
        let hull = [
            segment.start,
            segment.controls[0],
            segment.controls[1],
            segment.end,
        ]
        .map(|world| screen_of(rect, pan, zoom, world));
        let (min_x, max_x) = hull
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), corner| {
                (min.min(corner.x), max.max(corner.x))
            });
        let (min_y, max_y) = hull
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), corner| {
                (min.min(corner.y), max.max(corner.y))
            });
        if point.x < min_x - radius
            || point.x > max_x + radius
            || point.y < min_y - radius
            || point.y > max_y + radius
        {
            continue;
        }
        let mut previous = screen_of(rect, pan, zoom, segment.point_at(0.0));
        for step in 1..=128 {
            let next = screen_of(rect, pan, zoom, segment.point_at(step as f32 / 128.0));
            let direction = next - previous;
            let length_sq = direction.length_sq();
            let along = if length_sq > 0.0 {
                ((point - previous).dot(direction) / length_sq).clamp(0.0, 1.0)
            } else {
                0.0
            };
            if point.distance_sq(previous + direction * along) <= radius_sq {
                return true;
            }
            previous = next;
        }
    }
    false
}

pub(super) fn tuck_mouth_lane_at_seam(
    row: &mut [egui::Pos2],
    t: f32,
    profile: &PaintProfile,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
) {
    if row.len() == 2 {
        // A car-only mouth supplies the drivable merge surface across the
        // parent's verge. Collapsing either edge would close that surface.
        return;
    }
    // The parent covers the preceding ramp where it passes beneath the verge.
    // Open the mouth lane into that verge gradually from the first visible
    // section, so its full width does not appear abruptly at the seam.
    let seam = if let Some(parent) = profile.end_parent.as_ref() {
        profile
            .merge_alignment
            .map(|lane| (parent, lane, 1.0 - (t / 0.35).clamp(0.0, 1.0)))
    } else if let Some(parent) = profile.start_parent.as_ref() {
        profile
            .start_alignment
            .map(|lane| (parent, lane, 1.0 - ((1.0 - t) / 0.35).clamp(0.0, 1.0)))
    } else {
        None
    };
    let Some((parent, lane, blend)) = seam else {
        return;
    };
    if blend <= 0.0 || lane + 1 >= row.len() {
        return;
    }
    let left_inside = screen_point_in_road(row[lane], parent, rect, pan, zoom);
    let right_inside = screen_point_in_road(row[lane + 1], parent, rect, pan, zoom);
    if left_inside == right_inside {
        return;
    }
    let (inside, outside) = if left_inside {
        (lane, lane + 1)
    } else {
        (lane + 1, lane)
    };
    let mut within = row[inside];
    let mut beyond = row[outside];
    for _ in 0..8 {
        let middle = within + (beyond - within) * 0.5;
        if screen_point_in_road(middle, parent, rect, pan, zoom) {
            within = middle;
        } else {
            beyond = middle;
        }
    }
    let eased = blend * blend * (3.0 - 2.0 * blend);
    row[inside] += (beyond - row[inside]) * eased;
}

pub(super) fn trim_adjoining_ramp_to_parent(
    row: &mut [egui::Pos2],
    profile: &PaintProfile,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
) {
    // This footprint is inherited only by the exit_ramp next to a mouth.
    // Keep its surface outside the receiving road while collapsing the lane
    // and verge portions already buried beneath that road to its outer edge.
    if profile.start_alignment.is_some() || profile.merge_alignment.is_some() {
        return;
    }
    let Some(parent) = profile
        .start_parent
        .as_ref()
        .or(profile.end_parent.as_ref())
    else {
        return;
    };
    let inside: Vec<_> = row
        .iter()
        .map(|point| screen_point_in_road(*point, parent, rect, pan, zoom))
        .collect();
    let crossing = if inside[0] && !inside[row.len() - 1] {
        inside
            .iter()
            .position(|inside| !inside)
            .map(|outside| (0, outside))
    } else if !inside[0] && inside[row.len() - 1] {
        inside
            .iter()
            .rposition(|inside| !inside)
            .map(|outside| (outside + 1, row.len()))
    } else {
        None
    };
    let Some((start, end)) = crossing else {
        return;
    };
    let (inside_index, outside_index) = if start == 0 {
        (end - 1, end)
    } else {
        (start, start - 1)
    };
    let mut within = row[inside_index];
    let mut beyond = row[outside_index];
    for _ in 0..8 {
        let middle = within + (beyond - within) * 0.5;
        if screen_point_in_road(middle, parent, rect, pan, zoom) {
            within = middle;
        } else {
            beyond = middle;
        }
    }
    for point in &mut row[start..end] {
        *point = beyond;
    }
}

// These painters define road and junction geometry in world-scaled coordinates.
// scene3d tessellates their shapes and renders them for both cameras with wgpu.
pub(super) fn paint_profiled_road(
    painter: &egui::Painter,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    segment: &map::Segment,
    road: &RoadDefinition,
    selected: bool,
    palette: EditorPalette,
    profile: &PaintProfile,
) {
    let samples = 64;
    let mut boundaries = Vec::with_capacity(samples + 1);
    for step in 0..=samples {
        let t = step as f32 / samples as f32;
        let center = screen_of(rect, pan, zoom, segment.point_at(t));
        let normal = screen_normal_at(segment, t);
        let (widths, mut offset) = section_widths(road, zoom, profile, t);
        let mut row = Vec::with_capacity(widths.len() + 1);
        row.push(center + normal * offset);
        for width in widths {
            offset += width;
            row.push(center + normal * offset);
        }
        tuck_mouth_lane_at_seam(&mut row, t, profile, rect, pan, zoom);
        trim_adjoining_ramp_to_parent(&mut row, profile, rect, pan, zoom);
        boundaries.push(row);
    }
    for (element_index, element) in road.elements.iter().enumerate() {
        let color = kind_color(element.kind, palette.theme);
        let mut mesh = egui::epaint::Mesh::default();
        for row in &boundaries {
            mesh.colored_vertex(row[element_index], color);
            mesh.colored_vertex(row[element_index + 1], color);
        }
        for step in 0..samples {
            let a = (step * 2) as u32;
            mesh.add_triangle(a, a + 2, a + 3);
            mesh.add_triangle(a, a + 3, a + 1);
        }
        painter.add(egui::Shape::mesh(mesh));
    }
    let outline = if selected {
        palette.map_selected
    } else {
        palette.map_outline
    };
    for edge in [0, road.elements.len()] {
        let mut path = Vec::new();
        for row in &boundaries {
            let within_parent = profile
                .start_parent
                .as_ref()
                .is_some_and(|parent| screen_point_in_road(row[edge], parent, rect, pan, zoom))
                || profile
                    .end_parent
                    .as_ref()
                    .is_some_and(|parent| screen_point_in_road(row[edge], parent, rect, pan, zoom));
            if within_parent {
                if path.len() > 1 {
                    painter.add(egui::Shape::line(
                        std::mem::take(&mut path),
                        egui::Stroke::new(if selected { 2.5 } else { 1.5 }, outline),
                    ));
                } else {
                    path.clear();
                }
            } else {
                path.push(row[edge]);
            }
        }
        if path.len() > 1 {
            painter.add(egui::Shape::line(
                path,
                egui::Stroke::new(if selected { 2.5 } else { 1.5 }, outline),
            ));
        }
    }
    let mut spine = Vec::new();
    for step in 0..=samples {
        let point = screen_of(
            rect,
            pan,
            zoom,
            segment.point_at(step as f32 / samples as f32),
        );
        let within_parent = [profile.start_parent.as_ref(), profile.end_parent.as_ref()]
            .into_iter()
            .flatten()
            .any(|parent| screen_point_in_road(point, parent, rect, pan, zoom));
        if within_parent {
            if spine.len() > 1 {
                painter.add(egui::Shape::line(
                    std::mem::take(&mut spine),
                    egui::Stroke::new((zoom * 0.004).clamp(0.8, 12.0), palette.map_centerline),
                ));
            } else {
                spine.clear();
            }
        } else {
            spine.push(point);
        }
    }
    if spine.len() > 1 {
        painter.add(egui::Shape::line(
            spine,
            egui::Stroke::new((zoom * 0.004).clamp(0.8, 12.0), palette.map_centerline),
        ));
    }
}

pub(super) fn segment_is_straight(segment: &map::Segment) -> bool {
    let dx = segment.end.x - segment.start.x;
    let dy = segment.end.y - segment.start.y;
    let length_sq = dx * dx + dy * dy;
    if length_sq < 0.000001 {
        return false;
    }
    let mut prior_t = 0.0;
    for control in segment.controls {
        let px = control.x - segment.start.x;
        let py = control.y - segment.start.y;
        let t = (px * dx + py * dy) / length_sq;
        if (px * dy - py * dx).abs() > length_sq * 0.0001 || t < prior_t || t > 1.0 {
            return false;
        }
        prior_t = t;
    }
    true
}

pub(super) fn road_curve_samples(segment: &map::Segment) -> usize {
    if segment_is_straight(segment) {
        return 1;
    }
    let dx = segment.end.x - segment.start.x;
    let dy = segment.end.y - segment.start.y;
    let length = (dx * dx + dy * dy).sqrt().max(0.00001);
    let deviation = segment
        .controls
        .iter()
        .map(|point| {
            ((point.x - segment.start.x) * dy - (point.y - segment.start.y) * dx).abs() / length
        })
        .fold(0.0, f32::max);
    if deviation <= 0.025 {
        4
    } else if deviation <= 0.15 {
        8
    } else {
        32
    }
}

pub(super) fn paint_road_segment(
    painter: &egui::Painter,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    segment: &map::Segment,
    road: Option<&RoadDefinition>,
    selected: bool,
    palette: EditorPalette,
) {
    let detailed_markings = road.is_some_and(|road| {
        road.elements.iter().any(|element| {
            matches!(
                element.kind,
                RoadElementKind::TrainTrack
                    | RoadElementKind::TramTrack
                    | RoadElementKind::SubwayTrack
                    | RoadElementKind::Runway
                    | RoadElementKind::Taxiway
            )
        })
    });
    let samples = if detailed_markings {
        32
    } else {
        road_curve_samples(segment)
    };
    let spine: Vec<_> = (0..=samples)
        .map(|step| {
            screen_of(
                rect,
                pan,
                zoom,
                segment.point_at(step as f32 / samples as f32),
            )
        })
        .collect();
    let normals: Vec<_> = (0..spine.len())
        .map(|index| screen_normal_at(segment, index as f32 / samples as f32))
        .collect();
    let kinds: Vec<_> = road
        .map(|road| road.elements.iter().map(|element| element.kind).collect())
        .unwrap_or_else(|| vec![RoadElementKind::CarLane]);
    let total: f32 = kinds
        .iter()
        .map(|kind| map_element_width(*kind, zoom))
        .sum();
    painter.add(egui::Shape::line(
        spine.clone(),
        egui::Stroke::new(
            total + if selected { 6.0 } else { 3.0 },
            if selected {
                palette.map_selected
            } else {
                palette.map_outline
            },
        ),
    ));
    let mut offset = -total / 2.0;
    let mut track_bands = Vec::new();
    let mut airfield_bands = Vec::new();
    for kind in kinds {
        let width = map_element_width(kind, zoom);
        let center = offset + width / 2.0;
        let path: Vec<_> = spine
            .iter()
            .zip(&normals)
            .map(|(point, normal)| *point + *normal * center)
            .collect();
        painter.add(egui::Shape::line(
            path,
            egui::Stroke::new(width, kind_color(kind, palette.theme)),
        ));
        if matches!(
            kind,
            RoadElementKind::TrainTrack | RoadElementKind::TramTrack | RoadElementKind::SubwayTrack
        ) {
            track_bands.push((kind, center, width));
        }
        if matches!(kind, RoadElementKind::Runway | RoadElementKind::Taxiway) {
            airfield_bands.push((kind, center, width));
        }
        offset += width;
    }
    if !road.is_some_and(|road| {
        road.elements.iter().any(|element| {
            matches!(
                element.kind,
                RoadElementKind::Runway | RoadElementKind::Taxiway | RoadElementKind::Apron
            )
        })
    }) {
        painter.add(egui::Shape::line(
            spine.clone(),
            egui::Stroke::new((zoom * 0.004).clamp(0.8, 12.0), palette.map_centerline),
        ));
    }
    for (kind, center, width) in track_bands {
        paint_track_markings(
            painter,
            &spine,
            &normals,
            center,
            width,
            kind,
            palette.theme,
        );
    }
    for (kind, center, width) in airfield_bands {
        paint_airfield_markings(painter, &spine, &normals, center, width, kind);
    }
}
