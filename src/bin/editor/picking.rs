//! Picking for the city editor.
use super::*;

pub(super) fn line_crosses_rect(a: egui::Pos2, b: egui::Pos2, rect: egui::Rect) -> bool {
    let mut near = 0.0_f32;
    let mut far = 1.0_f32;
    for (start, end, low, high) in [
        (a.x, b.x, rect.min.x, rect.max.x),
        (a.y, b.y, rect.min.y, rect.max.y),
    ] {
        let delta = end - start;
        if delta.abs() <= f32::EPSILON {
            if start < low || start > high {
                return false;
            }
        } else {
            let first = (low - start) / delta;
            let second = (high - start) / delta;
            near = near.max(first.min(second));
            far = far.min(first.max(second));
            if near > far {
                return false;
            }
        }
    }
    true
}

pub(super) fn grid_point(point: WorldPoint) -> WorldPoint {
    WorldPoint {
        x: (point.x * 4.0).round() / 4.0,
        y: (point.y * 4.0).round() / 4.0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HoverPoint {
    Element(usize),
    Junction(usize),
}

pub(super) fn nearest_visible_points<T: Copy>(
    points: impl Iterator<Item = (T, egui::Pos2)>,
    rect: egui::Rect,
    pointer: egui::Pos2,
) -> Vec<T> {
    let mut ranked: Vec<_> = points
        .filter(|(_, screen)| rect.contains(*screen))
        .map(|(point, screen)| (pointer.distance_sq(screen), point))
        .collect();
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
    ranked.into_iter().take(4).map(|(_, point)| point).collect()
}

pub(super) fn junction_candidates(
    program: &Program,
    projection: &map::Projection,
) -> Vec<(WorldPoint, Option<String>)> {
    let placed_centers: Vec<_> = map::intersections(program, projection)
        .into_iter()
        .filter(|intersection| !intersection.ports.is_empty())
        .map(|intersection| intersection.point_ref)
        .collect();
    let mut candidates: Vec<_> = projection
        .points
        .iter()
        .filter(|point| !placed_centers.contains(&point.name))
        .map(|point| (point.position, Some(point.name.clone())))
        .collect();
    for segment in &projection.segments {
        for (position, reference) in [
            (segment.start, &segment.start_ref),
            (segment.end, &segment.end_ref),
        ] {
            if !candidates
                .iter()
                .any(|(candidate, _)| *candidate == position)
            {
                candidates.push((
                    position,
                    if segment.source.editable() {
                        reference.clone()
                    } else {
                        None
                    },
                ));
            }
        }
    }
    candidates
}

pub(super) fn junction_near(
    candidates: &[(WorldPoint, Option<String>)],
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<(WorldPoint, Option<String>)> {
    candidates
        .iter()
        .filter_map(|(position, reference)| {
            let distance = pointer.distance_sq(screen_of(rect, pan, zoom, *position));
            (distance <= 16.0 * 16.0).then(|| (distance, *position, reference.clone()))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, position, reference)| (position, reference))
}

pub(super) fn intersection_port_near(
    program: &Program,
    projection: &map::Projection,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<(WorldPoint, String, String)> {
    map::intersections(program, projection)
        .into_iter()
        .flat_map(|intersection| {
            intersection.ports.into_iter().map(move |port| {
                (
                    port.position,
                    port.point_ref,
                    intersection.road_name.clone(),
                    intersection.position,
                )
            })
        })
        .filter_map(|(position, reference, road_name, center)| {
            let distance = pointer.distance_sq(screen_of(rect, pan, zoom, position));
            let radius = (screen_of(rect, pan, zoom, position)
                .distance(screen_of(rect, pan, zoom, center))
                * 0.42)
                .clamp(3.0, 12.0);
            (distance <= radius * radius).then_some((distance, position, reference, road_name))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, position, reference, road_name)| (position, reference, road_name))
}

pub(super) fn placed_intersection_at(
    program: &Program,
    projection: &map::Projection,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<String> {
    if intersection_port_near(program, projection, rect, pan, zoom, pointer).is_some() {
        return None;
    }
    map::intersections(program, projection)
        .into_iter()
        .rev()
        .find_map(|intersection| {
            (!intersection.ports.is_empty()
                && intersection_paint(program, projection, &intersection, rect, pan, zoom)
                    .is_some_and(|paint| paint.square.contains(pointer)))
            .then_some(intersection.point_ref)
        })
}

#[derive(Clone, Copy)]
pub(super) struct ElementEndCandidate {
    pub(super) position: WorldPoint,
    pub(super) source: SegmentSource,
    pub(super) element_index: usize,
    pub(super) t: f32,
    pub(super) kind: RoadElementKind,
}

pub(super) fn element_end_candidates(
    projection: &map::Projection,
    program: &Program,
) -> Vec<ElementEndCandidate> {
    let mut candidates = Vec::new();
    for segment in projection
        .segments
        .iter()
        .filter(|segment| segment.source.editable())
    {
        let Some(road) = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == segment.road_name => Some(road),
                _ => None,
            })
        else {
            continue;
        };
        for (element_index, element) in road.elements.iter().enumerate() {
            for t in [0.0, 1.0] {
                if let Some(position) = map::element_position(segment, road, element_index, t) {
                    candidates.push(ElementEndCandidate {
                        position,
                        source: segment.source,
                        element_index,
                        t,
                        kind: element.kind,
                    });
                }
            }
        }
    }
    candidates
}

pub(super) fn element_end_near(
    candidates: &[ElementEndCandidate],
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<ElementEndCandidate> {
    candidates
        .iter()
        .copied()
        .filter_map(|candidate| {
            let distance = pointer.distance_sq(screen_of(rect, pan, zoom, candidate.position));
            (distance <= 12.0 * 12.0).then_some((distance, candidate))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, candidate)| candidate)
}

pub(super) fn segment_near(
    segments: &[map::Segment],
    program: &Program,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<SegmentSource> {
    segments
        .iter()
        .filter_map(|segment| {
            let half_width = program
                .statements
                .iter()
                .find_map(|statement| match statement {
                    Statement::Road(road) if road.name == segment.road_name => Some(
                        road.elements
                            .iter()
                            .map(|element| map_element_width(element.kind, zoom))
                            .sum::<f32>()
                            / 2.0,
                    ),
                    _ => None,
                })
                .unwrap_or(0.0);
            let radius = (half_width + 3.0).max(16.0);
            let nearest = (0..24)
                .filter_map(|step| {
                    let a = screen_of(rect, pan, zoom, segment.point_at(step as f32 / 24.0));
                    let b = screen_of(rect, pan, zoom, segment.point_at((step + 1) as f32 / 24.0));
                    let line = b - a;
                    let length_squared = line.length_sq();
                    if length_squared < 1.0 {
                        return (pointer.distance_sq(a) < radius * radius)
                            .then_some((pointer.distance_sq(a), step as f32 / 24.0));
                    }
                    let along = ((pointer - a).dot(line) / length_squared).clamp(0.0, 1.0);
                    let distance = pointer.distance_sq(a + line * along);
                    (distance < radius * radius).then_some((distance, (step as f32 + along) / 24.0))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))?;
            Some((segment.source, segment.height_at(nearest.1), nearest.0))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.2.total_cmp(&a.2)))
        .map(|candidate| candidate.0)
}

pub(super) fn control_near(
    segments: &[map::Segment],
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<(SegmentSource, bool)> {
    segments
        .iter()
        .rev()
        .filter(|segment| segment.source.editable())
        .find_map(|segment| {
            let [first, second] = segment.controls;
            if pointer.distance_sq(screen_of(rect, pan, zoom, first)) <= 12.0 * 12.0 {
                Some((segment.source, false))
            } else if pointer.distance_sq(screen_of(rect, pan, zoom, second)) <= 12.0 * 12.0 {
                Some((segment.source, true))
            } else {
                None
            }
        })
}

pub(super) fn endpoint_near(
    segments: &[map::Segment],
    program: &Program,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    pointer: egui::Pos2,
) -> Option<(SegmentSource, bool)> {
    segments
        .iter()
        .rev()
        .filter(|segment| segment.source.editable())
        .find_map(|segment| {
            let start = screen_of(rect, pan, zoom, segment.start);
            let end = screen_of(rect, pan, zoom, segment.end);
            if pointer.distance_sq(start) <= 14.0 * 14.0
                && !segment.start_ref.as_deref().is_some_and(|name| {
                    map::anchored_reference(program, name)
                        && map::lane_attachment(program, name).is_none()
                })
            {
                Some((segment.source, false))
            } else if pointer.distance_sq(end) <= 14.0 * 14.0
                && !segment.end_ref.as_deref().is_some_and(|name| {
                    map::anchored_reference(program, name)
                        && map::lane_attachment(program, name).is_none()
                })
            {
                Some((segment.source, true))
            } else {
                None
            }
        })
}
