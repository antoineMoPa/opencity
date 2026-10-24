//! Picking iso for the city editor.
use super::*;

pub(super) fn iso_endpoint_near(
    segments: &[map::Segment],
    program: &Program,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    target: WorldPoint,
    angle: f32,
    elevation: f32,
    pointer: egui::Pos2,
) -> Option<(SegmentSource, bool)> {
    segments
        .iter()
        .rev()
        .filter(|segment| segment.source.editable())
        .flat_map(|segment| {
            [
                (
                    false,
                    segment.start,
                    segment.start_z,
                    segment.start_ref.as_deref(),
                ),
                (true, segment.end, segment.end_z, segment.end_ref.as_deref()),
            ]
            .into_iter()
            .filter_map(move |(is_end, point, height, reference)| {
                (!reference.is_some_and(|name| {
                    map::anchored_reference(program, name)
                        && map::lane_attachment(program, name).is_none()
                }))
                .then_some((segment.source, is_end, point, height))
            })
        })
        .filter_map(|(source, is_end, point, height)| {
            let distance = pointer.distance_sq(iso_screen(
                rect, pan, zoom, target, angle, elevation, point, height,
            ));
            (distance <= 12.0 * 12.0).then_some((source, is_end, distance))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(source, is_end, _)| (source, is_end))
}

pub(super) fn iso_control_near(
    segments: &[map::Segment],
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    target: WorldPoint,
    angle: f32,
    elevation: f32,
    pointer: egui::Pos2,
) -> Option<(SegmentSource, bool)> {
    segments
        .iter()
        .rev()
        .filter(|segment| segment.source.editable())
        .flat_map(|segment| {
            [
                (false, segment.controls[0], segment.height_at(1.0 / 3.0)),
                (true, segment.controls[1], segment.height_at(2.0 / 3.0)),
            ]
            .into_iter()
            .map(move |(second, point, height)| (segment.source, second, point, height))
        })
        .filter_map(|(source, second, point, height)| {
            let distance = pointer.distance_sq(iso_screen(
                rect, pan, zoom, target, angle, elevation, point, height,
            ));
            (distance <= 12.0 * 12.0).then_some((source, second, distance))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(source, second, _)| (source, second))
}

pub(super) fn iso_element_end_near(
    candidates: &[ElementEndCandidate],
    projection: &map::Projection,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    target: WorldPoint,
    angle: f32,
    elevation: f32,
    pointer: egui::Pos2,
) -> Option<ElementEndCandidate> {
    candidates
        .iter()
        .copied()
        .filter_map(|candidate| {
            let segment = projection
                .segments
                .iter()
                .find(|segment| segment.source == candidate.source)?;
            let height = segment.height_at(candidate.t);
            let distance = pointer.distance_sq(iso_screen(
                rect,
                pan,
                zoom,
                target,
                angle,
                elevation,
                candidate.position,
                height,
            ));
            (distance <= 12.0 * 12.0).then_some((candidate, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(candidate, _)| candidate)
}

pub(super) fn iso_segment_near(
    segments: &[map::Segment],
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    target: WorldPoint,
    angle: f32,
    elevation: f32,
    pointer: egui::Pos2,
) -> Option<SegmentSource> {
    segments
        .iter()
        .filter_map(|segment| {
            let distance = (0..=32)
                .map(|step| {
                    let t = step as f32 / 32.0;
                    pointer.distance_sq(iso_screen(
                        rect,
                        pan,
                        zoom,
                        target,
                        angle,
                        elevation,
                        segment.point_at(t),
                        segment.height_at(t),
                    ))
                })
                .fold(f32::INFINITY, f32::min);
            (distance <= 20.0 * 20.0).then_some((segment.source, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(source, _)| source)
}
