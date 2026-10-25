//! End ordinary approach pavement at a junction's footprint, not its center.
use super::*;

fn boundary_parameter(
    segment: &map::Segment,
    center: map::WorldPoint,
    half: f32,
    from_end: bool,
) -> f32 {
    let inside = |t: f32| {
        let p = segment.point_at(t);
        (p.x - center.x).abs() <= half && (p.y - center.y).abs() <= half
    };
    let parameter = |step: f32| if from_end { 1. - step } else { step };
    let mut previous = 0.;
    for step in 1..=128 {
        let next = step as f32 / 128.;
        if !inside(parameter(next)) {
            let (mut low, mut high) = (previous, next);
            for _ in 0..26 {
                let middle = (low + high) / 2.;
                if inside(parameter(middle)) {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            return parameter((low + high) / 2.);
        }
        previous = next;
    }
    parameter(1.)
}

pub(super) fn apply(program: &Program, projection: &map::Projection, profiles: &mut [RoadProfile]) {
    let widths: HashMap<_, _> = program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Road(road) => Some((
                road.name.as_str(),
                road.elements
                    .iter()
                    .map(|element| map::element_world_width(element.kind))
                    .sum::<f32>()
                    / 2.,
            )),
            _ => None,
        })
        .collect();
    let mut endpoints: HashMap<&str, Vec<(usize, bool)>> = HashMap::new();
    for (index, segment) in projection.segments.iter().enumerate() {
        if let Some(name) = segment.start_ref.as_deref() {
            endpoints.entry(name).or_default().push((index, false));
        }
        if let Some(name) = segment.end_ref.as_deref() {
            endpoints.entry(name).or_default().push((index, true));
        }
    }
    for intersection in map::intersections(program, projection) {
        let road_half = std::iter::once(intersection.road_name.as_str())
            .chain(intersection.arm_roads.iter().map(|(_, name)| name.as_str()))
            .filter_map(|name| widths.get(name))
            .copied()
            .fold(0., f32::max);
        let half = map::intersection_footprint_half(&intersection, road_half);
        if half <= 0. {
            continue;
        }
        let inside = |point: map::WorldPoint| {
            (point.x - intersection.position.x).abs() <= half
                && (point.y - intersection.position.y).abs() <= half
        };
        let mut pending: Vec<_> = std::iter::once(intersection.point_ref.as_str())
            .chain(
                intersection
                    .ports
                    .iter()
                    .map(|port| port.point_ref.as_str()),
            )
            .flat_map(|name| endpoints.get(name).into_iter().flatten().copied())
            .collect();
        let mut visited = std::collections::HashSet::new();
        while let Some((index, from_end)) = pending.pop() {
            if !visited.insert((index, from_end)) {
                continue;
            }
            let segment = &projection.segments[index];
            if !inside(if from_end { segment.end } else { segment.start }) {
                continue;
            }
            let parameter = boundary_parameter(segment, intersection.position, half, from_end);
            if from_end {
                profiles[index].end_clip =
                    Some(profiles[index].end_clip.unwrap_or(1.).min(parameter));
            } else {
                profiles[index].start_clip =
                    Some(profiles[index].start_clip.unwrap_or(0.).max(parameter));
            }
            let other = if from_end { segment.start } else { segment.end };
            if inside(other) {
                pending.push((index, !from_end));
                let reference = if from_end {
                    segment.start_ref.as_deref()
                } else {
                    segment.end_ref.as_deref()
                };
                if let Some(reference) = reference {
                    pending.extend(endpoints.get(reference).into_iter().flatten().copied());
                }
            }
        }
    }
}
