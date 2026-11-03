//! Conservative continuations through explicit graph nodes, never XY proximity.
use crate::{map, Program, RoadDefinition, RoadDirection, RoadRole};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Continuation {
    pub segments: [usize; 2],
    pub reference: String,
}

#[derive(Default)]
pub struct Continuations {
    pub links: Vec<Continuation>,
    pub ambiguous_nodes: Vec<String>,
    pub unused_boundaries: Vec<String>,
}

#[derive(Clone, Copy)]
struct End {
    segment: usize,
    end: bool,
}

#[derive(PartialEq, Eq)]
enum Family {
    Street,
    Highway,
    Connector,
}

fn family(role: RoadRole) -> Family {
    match role {
        RoadRole::Ordinary => Family::Street,
        RoadRole::Highway => Family::Highway,
        RoadRole::Ramp | RoadRole::Merge => Family::Connector,
    }
}

fn flow(road: &RoadDefinition, end: bool) -> BTreeMap<&'static str, [usize; 3]> {
    let mut modes = BTreeMap::new();
    for (i, element) in road.elements.iter().enumerate() {
        let Some(direction) = road.lane_direction(i) else {
            continue;
        };
        let index = match direction {
            RoadDirection::Both => 2,
            RoadDirection::Forward if end => 0,
            RoadDirection::Reverse if !end => 0,
            RoadDirection::Forward | RoadDirection::Reverse => 1,
        };
        modes.entry(element.kind.keyword()).or_insert([0; 3])[index] += 1;
    }
    modes
}

fn score(program: &Program, projection: &map::Projection, a: End, b: End) -> Option<f32> {
    if a.segment == b.segment {
        return None;
    }
    let first = &projection.segments[a.segment];
    let second = &projection.segments[b.segment];
    let ar = crate::road_profiles::road_for_segment(program, first)?;
    let br = crate::road_profiles::road_for_segment(program, second)?;
    if family(ar.role) != family(br.role) {
        return None;
    }
    let af = flow(ar, a.end);
    let mut bf = flow(br, b.end);
    for count in bf.values_mut() {
        count.swap(0, 1);
    }
    if af.is_empty() || af != bf {
        return None;
    }
    let ta = f32::from(a.end);
    let tb = f32::from(b.end);
    if (first.height_at(ta) - second.height_at(tb)).abs() > 0.005 {
        return None;
    }
    let x = first.tangent_at(ta);
    let y = second.tangent_at(tb);
    let lengths = x.x.hypot(x.y) * y.x.hypot(y.y);
    if lengths <= 0.000001 {
        return None;
    }
    let outward_a = if a.end { -1.0 } else { 1.0 };
    let outward_b = if b.end { -1.0 } else { 1.0 };
    let cosine = -(x.x * y.x + x.y * y.y) * outward_a * outward_b / lengths;
    (cosine >= 0.95).then_some(cosine)
}

/// Resolve mutually unique mainline continuations. Lane attachments remain branches.
pub fn continuation_links(program: &Program, projection: &map::Projection) -> Continuations {
    let boundaries: BTreeSet<_> = projection.road_names.boundaries.iter().cloned().collect();
    let intersections = map::intersections(program, projection);
    let mut ports = BTreeMap::new();
    let mut known = BTreeSet::new();
    for junction in &intersections {
        known.insert(junction.point_ref.clone());
        for port in &junction.ports {
            ports.insert(port.point_ref.clone(), junction.point_ref.clone());
        }
    }
    let mut nodes = BTreeMap::<String, Vec<End>>::new();
    for (i, segment) in projection.segments.iter().enumerate() {
        for (reference, end) in [(&segment.start_ref, false), (&segment.end_ref, true)] {
            let Some(reference) = reference else { continue };
            known.insert(reference.clone());
            let node = ports.get(reference).unwrap_or(reference);
            if boundaries.contains(reference) || boundaries.contains(node) {
                continue;
            }
            nodes
                .entry(node.clone())
                .or_default()
                .push(End { segment: i, end });
        }
    }
    let mut result = Continuations::default();
    for (reference, ends) in nodes {
        let mut choices = vec![None; ends.len()];
        let mut ambiguous = false;
        for (i, &a) in ends.iter().enumerate() {
            let mut candidates: Vec<_> = ends
                .iter()
                .enumerate()
                .filter_map(|(j, &b)| score(program, projection, a, b).map(|s| (j, s)))
                .collect();
            candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
            if let Some(&(best, value)) = candidates.first() {
                if candidates
                    .get(1)
                    .is_some_and(|next| value - next.1 <= 0.025)
                {
                    ambiguous = true;
                } else {
                    choices[i] = Some(best);
                }
            }
        }
        if ambiguous {
            result.ambiguous_nodes.push(reference.clone());
        }
        for (i, choice) in choices.iter().enumerate() {
            if let Some(j) = *choice {
                if i < j && choices[j] == Some(i) {
                    result.links.push(Continuation {
                        segments: [ends[i].segment, ends[j].segment],
                        reference: reference.clone(),
                    });
                }
            }
        }
    }
    result.unused_boundaries = boundaries.difference(&known).cloned().collect();
    result
}
