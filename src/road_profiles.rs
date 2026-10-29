//! Shared world-space cross-section alignment and taper rules.
use crate::{map, Program, RoadDefinition, RoadElementKind, Statement};
use std::collections::HashMap;
#[path = "road_profiles/intersections.rs"]
mod intersections;

#[derive(Clone, Debug, Default)]
pub struct RoadProfile {
    pub start_clip: Option<f32>,
    pub end_clip: Option<f32>,
    pub start_alignment: Option<usize>,
    pub merge_alignment: Option<usize>,
    pub start_taper: Vec<usize>,
    pub end_taper: Vec<usize>,
    pub start_parent: Option<AttachmentFootprint>,
    pub end_parent: Option<AttachmentFootprint>,
}

#[derive(Clone, Debug)]
pub struct AttachmentFootprint {
    pub segments: Vec<map::Segment>,
    pub half_width: f32,
}

pub fn attachment_footprint(
    projection: &map::Projection,
    parent_index: usize,
    road: &RoadDefinition,
) -> AttachmentFootprint {
    let parent = &projection.segments[parent_index];
    let joins = road_joins(projection);
    let mut included = vec![false; projection.segments.len()];
    included[parent_index] = true;
    loop {
        let mut expanded = false;
        for &(incoming, outgoing) in &joins {
            let same_profile = projection.segments[incoming].road_name == parent.road_name
                && projection.segments[outgoing].road_name == parent.road_name;
            if same_profile
                && straight_join(
                    &projection.segments[incoming],
                    &projection.segments[outgoing],
                )
            {
                if included[incoming] && !included[outgoing] {
                    included[outgoing] = true;
                    expanded = true;
                } else if included[outgoing] && !included[incoming] {
                    included[incoming] = true;
                    expanded = true;
                }
            }
        }
        if !expanded {
            break;
        }
    }
    let segments = projection
        .segments
        .iter()
        .enumerate()
        .filter_map(|(index, segment)| included[index].then(|| segment.clone()))
        .collect();
    let total: f32 = road
        .elements
        .iter()
        .map(|element| map::element_world_width(element.kind))
        .sum();
    AttachmentFootprint {
        segments,
        half_width: total / 2.0,
    }
}

pub fn road_joins(projection: &map::Projection) -> Vec<(usize, usize)> {
    let mut ends: HashMap<&str, Vec<(usize, bool)>> = HashMap::new();
    for (index, segment) in projection.segments.iter().enumerate() {
        if let Some(reference) = segment.start_ref.as_deref() {
            ends.entry(reference).or_default().push((index, false));
        }
        if let Some(reference) = segment.end_ref.as_deref() {
            ends.entry(reference).or_default().push((index, true));
        }
    }
    ends.into_values()
        .filter_map(|ends| {
            if ends.len() != 2 {
                return None;
            }
            match (ends[0], ends[1]) {
                ((a, true), (b, false)) => Some((a, b)),
                ((a, false), (b, true)) => Some((b, a)),
                _ => None,
            }
        })
        .collect()
}

pub fn straight_join(incoming: &map::Segment, outgoing: &map::Segment) -> bool {
    let a = incoming.tangent_at(1.0);
    let b = outgoing.tangent_at(0.0);
    let lengths = (a.x * a.x + a.y * a.y).sqrt() * (b.x * b.x + b.y * b.y).sqrt();
    lengths > 0.000001 && (a.x * b.x + a.y * b.y) / lengths > 0.95
}

/// Indices of the wider road's elements outside an exact, contiguous match.
pub fn extra_elements_at_join(
    wide: &RoadDefinition,
    narrow: &RoadDefinition,
) -> Option<Vec<usize>> {
    if wide.elements.len() <= narrow.elements.len() {
        return None;
    }
    let start = wide
        .elements
        .windows(narrow.elements.len())
        .position(|window| window == narrow.elements.as_slice())?;
    Some(
        (0..wide.elements.len())
            .filter(|index| *index < start || *index >= start + narrow.elements.len())
            .collect(),
    )
}

pub fn road_for_segment<'a>(
    program: &'a Program,
    segment: &map::Segment,
) -> Option<&'a RoadDefinition> {
    program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Road(road) if road.name == segment.road_name => Some(road),
            _ => None,
        })
}

pub fn ramp_element_at_attachment(
    parent_road: &RoadDefinition,
    ramp_road: &RoadDefinition,
    element_index: usize,
) -> Option<usize> {
    if element_index >= parent_road.elements.len() || ramp_road.elements.is_empty() {
        return None;
    }
    // Merge mouths carry their paved shoulders across the parent's edge,
    // rather than copying the parent's outward cross-section.
    if parent_road.elements[element_index].kind == RoadElementKind::CarLane
        && ramp_road.role == crate::RoadRole::Merge
        && merge_profile_is_paved(ramp_road)
        && ramp_road
            .elements
            .iter()
            .filter(|e| e.kind == RoadElementKind::CarLane)
            .count()
            == 1
    {
        return ramp_road
            .elements
            .iter()
            .position(|e| e.kind == RoadElementKind::CarLane);
    }
    if parent_road.elements[element_index].kind == RoadElementKind::CarLane
        && ramp_road.elements.len() == 1
        && ramp_road.elements[0].kind == RoadElementKind::CarLane
    {
        return Some(0);
    }
    let outward = &parent_road.elements[element_index..];
    let inward = &parent_road.elements[..=element_index];
    let elements = &ramp_road.elements;
    if elements == outward || elements.iter().rev().eq(inward.iter()) {
        Some(0)
    } else if elements == inward || elements.iter().rev().eq(outward.iter()) {
        Some(elements.len() - 1)
    } else {
        None
    }
}

/// Merge surfaces may include shoulders, but never sidewalks or barriers.
pub fn merge_profile_is_paved(road: &RoadDefinition) -> bool {
    road.elements
        .iter()
        .any(|e| e.kind == RoadElementKind::CarLane)
        && road
            .elements
            .iter()
            .all(|e| matches!(e.kind, RoadElementKind::CarLane | RoadElementKind::Verge))
}

pub fn road_profiles(program: &Program, projection: &map::Projection) -> Vec<RoadProfile> {
    let mut profiles: Vec<_> = projection
        .segments
        .iter()
        .map(|_| RoadProfile::default())
        .collect();
    for split in map::lane_splits_with_projection(program, projection) {
        let Some((parent_index, parent)) = projection
            .segments
            .iter()
            .enumerate()
            .find(|(_, segment)| segment.name.as_deref() == Some(&split.parent))
        else {
            continue;
        };
        let Some((ramp_index, ramp)) = projection
            .segments
            .iter()
            .enumerate()
            .find(|(_, segment)| segment.name.as_deref() == Some(&split.ramp))
        else {
            continue;
        };
        let Some(parent_road) = road_for_segment(program, parent) else {
            continue;
        };
        let Some(ramp_road) = road_for_segment(program, ramp) else {
            continue;
        };
        if let Some(lane) = ramp_element_at_attachment(parent_road, ramp_road, split.element_index)
        {
            profiles[ramp_index].start_alignment = Some(lane);
            profiles[ramp_index].start_parent =
                Some(attachment_footprint(projection, parent_index, parent_road));
        }
    }
    for (index, segment) in projection.segments.iter().enumerate() {
        let Some((parent_name, element_index, _)) = segment
            .end_ref
            .as_deref()
            .and_then(|reference| map::lane_attachment(program, reference))
        else {
            continue;
        };
        let Some((parent_index, parent)) = projection
            .segments
            .iter()
            .enumerate()
            .find(|(_, candidate)| candidate.name.as_deref() == Some(&parent_name))
        else {
            continue;
        };
        let (Some(parent_road), Some(ramp_road)) = (
            road_for_segment(program, parent),
            road_for_segment(program, segment),
        ) else {
            continue;
        };
        if let Some(lane) = ramp_element_at_attachment(parent_road, ramp_road, element_index) {
            profiles[index].merge_alignment = Some(lane);
            profiles[index].end_parent =
                Some(attachment_footprint(projection, parent_index, parent_road));
        }
    }
    for (incoming, outgoing) in road_joins(projection) {
        let a = &projection.segments[incoming];
        let b = &projection.segments[outgoing];
        if !straight_join(a, b) {
            continue;
        }
        // The exit ramp adjoining a mouth can reach beneath the receiving
        // highway before the mouth starts. Clip that hidden outline against
        // the same parent, while leaving its lane shape and paint order alone.
        if road_for_segment(program, a).is_some_and(|r| r.role == crate::RoadRole::Ramp)
            && road_for_segment(program, b).is_some_and(|r| r.role == crate::RoadRole::Merge)
        {
            profiles[incoming].end_parent = profiles[outgoing].end_parent.clone();
        } else if road_for_segment(program, a).is_some_and(|r| r.role == crate::RoadRole::Merge)
            && road_for_segment(program, b).is_some_and(|r| r.role == crate::RoadRole::Ramp)
        {
            profiles[outgoing].start_parent = profiles[incoming].start_parent.clone();
        }
        let (Some(a_road), Some(b_road)) =
            (road_for_segment(program, a), road_for_segment(program, b))
        else {
            continue;
        };
        if let Some(extra) = extra_elements_at_join(a_road, b_road) {
            profiles[incoming].end_taper = extra;
        } else if let Some(extra) = extra_elements_at_join(b_road, a_road) {
            profiles[outgoing].start_taper = extra;
        }
    }
    intersections::apply(program, projection, &mut profiles);
    profiles
}

pub fn section_widths(road: &RoadDefinition, profile: &RoadProfile, t: f32) -> (Vec<f32>, f32) {
    let mut widths: Vec<_> = road
        .elements
        .iter()
        .map(|element| map::element_world_width(element.kind))
        .collect();
    let smooth = |value: f32| {
        let x = value.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    for index in &profile.start_taper {
        widths[*index] *= smooth(t / 0.25);
    }
    for index in &profile.end_taper {
        widths[*index] *= smooth((1.0 - t) / 0.25);
    }
    let total: f32 = widths.iter().sum();
    let start_offset = profile
        .start_alignment
        .and_then(|index| {
            widths
                .get(index)
                .map(|selected| total / 2.0 - widths[..index].iter().sum::<f32>() - selected / 2.0)
        })
        .unwrap_or(0.0);
    let end_offset = profile
        .merge_alignment
        .and_then(|index| {
            widths
                .get(index)
                .map(|selected| total / 2.0 - widths[..index].iter().sum::<f32>() - selected / 2.0)
        })
        .unwrap_or(0.0);
    (
        widths,
        -total / 2.0 + start_offset * (1.0 - t).powi(2) + end_offset * t.powi(2),
    )
}
