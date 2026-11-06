//! A junction's local street frame, inferred from its connected approaches.
use super::*;

/// Rotation modulo quarter turns; cardinal maps retain their existing frame.
pub fn intersection_rotation(projection: &Projection, junction: &Intersection) -> f32 {
    let mut sine = 0.;
    let mut cosine = 0.;
    let mut count = 0;
    for segment in &projection.segments {
        for (reference, end) in [(&segment.start_ref, false), (&segment.end_ref, true)] {
            if !reference.as_ref().is_some_and(|r| {
                r == &junction.point_ref || junction.ports.iter().any(|port| &port.point_ref == r)
            }) {
                continue;
            }
            let direction = segment.tangent_at(if end { 1. } else { 0. });
            if direction.x.hypot(direction.y) < 0.000001 {
                continue;
            }
            let angle = direction.y.atan2(direction.x) * 4.;
            sine += angle.sin();
            cosine += angle.cos();
            count += 1;
        }
    }
    if count == 0 || sine.hypot(cosine) < 0.000001 {
        return 0.;
    }
    let degrees = sine.atan2(cosine).to_degrees() / 4.;
    if degrees.abs() < 0.0001 {
        0.
    } else {
        degrees
    }
}

pub fn junction_local_direction(direction: WorldPoint, rotation: f32) -> WorldPoint {
    let [x, y] = opencity::blocks::rotate_vector([direction.x, direction.y], -rotation);
    WorldPoint { x, y }
}
