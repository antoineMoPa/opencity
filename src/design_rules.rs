//! Design checks for a parsed `.road` program.
//!
//! These checks are advisory: parsing does not reject a road network with design issues.

use std::collections::HashMap;

use crate::{map, Program, RoadDefinition, RoadDirection, RoadElementKind, Statement};

/// A design rule that applies to road network geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesignRule {
    InvalidMergeConnection,
    VehiclePathObstructed,
    GeometryUnresolved,
    UnconnectedRampEnd,
    RampAtGradeCrossing,
    VehicleLaneDiscontinuity,
    VehicleLaneCrossesNonCarBand,
    VehicleLaneMergeHeightMismatch,
}

impl DesignRule {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidMergeConnection => "invalid_merge_connection",
            Self::VehiclePathObstructed => "vehicle_path_obstructed",
            Self::GeometryUnresolved => "geometry_unresolved",
            Self::UnconnectedRampEnd => "unconnected_ramp_end",
            Self::RampAtGradeCrossing => "ramp_at_grade_crossing",
            Self::VehicleLaneDiscontinuity => "vehicle_lane_discontinuity",
            Self::VehicleLaneCrossesNonCarBand => "vehicle_lane_crosses_non_car_band",
            Self::VehicleLaneMergeHeightMismatch => "vehicle_lane_merge_height_mismatch",
        }
    }
}

/// One design issue, positioned in `.road` world coordinates.
#[derive(Clone, Debug)]
pub struct DesignIssue {
    pub rule: DesignRule,
    pub message: String,
    pub at: map::WorldPoint,
    pub roads: [String; 2],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RoadNetworkRole {
    Ramp,
    RampMouth,
    Other,
}

fn road_network_role(road: &RoadDefinition) -> RoadNetworkRole {
    match road.role {
        crate::RoadRole::Merge => RoadNetworkRole::RampMouth,
        crate::RoadRole::Ramp => RoadNetworkRole::Ramp,
        crate::RoadRole::Ordinary | crate::RoadRole::Highway => RoadNetworkRole::Other,
    }
}

/// Checks ramp endpoints and crossings in a parsed program.
pub fn check_design_rules(program: &Program) -> Vec<DesignIssue> {
    let projection = map::project(program);
    check_design_rules_with_projection(program, &projection)
}

/// Checks a parsed program using an existing projection, avoiding another projection pass.
pub fn check_design_rules_with_projection(
    program: &Program,
    projection: &map::Projection,
) -> Vec<DesignIssue> {
    let mut issues = Vec::new();
    let mut road_ends: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, segment) in projection.segments.iter().enumerate() {
        for reference in [segment.start_ref.as_deref(), segment.end_ref.as_deref()]
            .into_iter()
            .flatten()
        {
            road_ends.entry(reference).or_default().push(index);
        }
    }
    for (index, segment) in projection.segments.iter().enumerate() {
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
        if road_network_role(road) == RoadNetworkRole::Other {
            continue;
        }
        if road.role == crate::RoadRole::Merge {
            let paved = crate::road_profiles::merge_profile_is_paved(road);
            let lane_attachment = [segment.start_ref.as_deref(),segment.end_ref.as_deref()].into_iter().flatten().any(|reference| {
        let Some((parent,lane,_))=map::lane_attachment(program,reference) else{return false;};
        let Some(parent)=projection.segments.iter().find(|s|s.name.as_deref()==Some(&parent)) else{return false;};
        program.statements.iter().any(|s|matches!(s,Statement::Road(r) if r.name==parent.road_name && r.elements.get(lane).is_some_and(|e|e.kind==RoadElementKind::CarLane)))
    });
            if !paved || !lane_attachment {
                issues.push(DesignIssue {
                    rule: DesignRule::InvalidMergeConnection,
                    message: format!(
                        "{} needs a paved merge profile and an explicit attachment to a car lane",
                        segment_name(segment)
                    ),
                    at: segment.start,
                    roads: [segment_name(segment).into(), String::new()],
                });
            }
        }
        for (reference, position, side) in [
            (segment.start_ref.as_deref(), segment.start, "start"),
            (segment.end_ref.as_deref(), segment.end, "end"),
        ] {
            let connected = reference.is_some_and(|reference| {
                road_ends
                    .get(reference)
                    .is_some_and(|indices| indices.iter().any(|other| *other != index))
                    || map::anchored_reference(program, reference)
            });
            if !connected {
                let name = segment.name.as_deref().unwrap_or(&segment.road_name);
                issues.push(DesignIssue {
                    rule: DesignRule::UnconnectedRampEnd,
                    message: format!("{name} {side} must join a road or lane"),
                    at: position,
                    roads: [name.to_owned(), String::new()],
                });
            }
        }
    }

    let roles: HashMap<&str, RoadNetworkRole> = program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Road(road) => Some((road.name.as_str(), road_network_role(road))),
            _ => None,
        })
        .collect();
    for ramp in projection
        .segments
        .iter()
        .filter(|segment| roles.get(segment.road_name.as_str()) == Some(&RoadNetworkRole::Ramp))
    {
        for other in projection.segments.iter().filter(|segment| {
            roles.get(segment.road_name.as_str()) == Some(&RoadNetworkRole::Other)
        }) {
            if let Some(at) = segment_crossing_at_grade(ramp, other) {
                let ramp_name = ramp.name.as_deref().unwrap_or(&ramp.road_name);
                let other_name = other.name.as_deref().unwrap_or(&other.road_name);
                issues.push(DesignIssue {
                    rule: DesignRule::RampAtGradeCrossing,
                    message: format!(
                        "{ramp_name} crosses {other_name} at the same height without a junction"
                    ),
                    at,
                    roads: [ramp_name.to_owned(), other_name.to_owned()],
                });
            }
        }
    }
    let geometry = crate::road_geometry::RoadGeometry::compile(program, projection);
    for &index in &geometry.unresolved {
        let segment = &projection.segments[index];
        issues.push(DesignIssue {
            rule: DesignRule::GeometryUnresolved,
            message: format!(
                "{} exceeds geometric subdivision budget",
                segment_name(segment)
            ),
            at: segment.start,
            roads: [segment_name(segment).into(), String::new()],
        });
    }
    for (lane, other, at, kind) in geometry.band_intrusions(program, projection) {
        let lane = segment_name(&projection.segments[lane]);
        let other = segment_name(&projection.segments[other]);
        issues.push(DesignIssue {
            rule: DesignRule::VehicleLaneCrossesNonCarBand,
            message: format!("{lane}'s full lane width overlaps {other} {kind:?}"),
            at,
            roads: [lane.into(), other.into()],
        });
    }
    for (wall, lane, at) in geometry.obstructions() {
        let wall = segment_name(&projection.segments[wall]);
        let lane = segment_name(&projection.segments[lane]);
        issues.push(DesignIssue {
            rule: DesignRule::VehiclePathObstructed,
            message: format!("{wall} wall obstructs {lane}'s vehicle corridor"),
            at,
            roads: [wall.into(), lane.into()],
        });
    }
    issues.extend(check_vehicle_lane_continuity(program, projection));
    issues.extend(check_vehicle_lane_intrusions(program, projection));
    issues
}

fn car_lane_indices(road: &RoadDefinition) -> impl Iterator<Item = usize> + '_ {
    road.elements
        .iter()
        .enumerate()
        .filter_map(|(index, element)| (element.kind == RoadElementKind::CarLane).then_some(index))
}

fn road_definitions(program: &Program) -> HashMap<&str, &RoadDefinition> {
    program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Road(road) => Some((road.name.as_str(), road)),
            _ => None,
        })
        .collect()
}

fn segment_name(segment: &map::Segment) -> &str {
    segment.name.as_deref().unwrap_or(&segment.road_name)
}

fn distance(a: map::WorldPoint, b: map::WorldPoint) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

fn enters_seam(direction: RoadDirection, t: f32) -> Option<bool> {
    match direction {
        RoadDirection::Forward => Some(t > 0.5),
        RoadDirection::Reverse => Some(t < 0.5),
        RoadDirection::Both => None,
    }
}

fn tangent_match(a: map::WorldPoint, b: map::WorldPoint, allow_reverse: bool) -> bool {
    let magnitude = (a.x * a.x + a.y * a.y).sqrt() * (b.x * b.x + b.y * b.y).sqrt();
    if magnitude < 0.000001 {
        return false;
    }
    let dot = (a.x * b.x + a.y * b.y) / magnitude;
    if allow_reverse {
        dot.abs() > 0.7
    } else {
        dot > 0.7
    }
}

/// Find car-lane seams whose lane surfaces do not meet at the same height.
/// Free external ports and explicit intersection ports have no ordinary seam.
pub fn check_vehicle_lane_continuity(
    program: &Program,
    projection: &map::Projection,
) -> Vec<DesignIssue> {
    let roads = road_definitions(program);
    let intersection_centers: std::collections::HashSet<_> =
        map::intersections(program, projection)
            .into_iter()
            .map(|intersection| intersection.point_ref)
            .collect();
    let mut ends: HashMap<&str, Vec<(&map::Segment, f32)>> = HashMap::new();
    for segment in &projection.segments {
        if let Some(reference) = segment.start_ref.as_deref() {
            ends.entry(reference).or_default().push((segment, 0.0));
        }
        if let Some(reference) = segment.end_ref.as_deref() {
            ends.entry(reference).or_default().push((segment, 1.0));
        }
    }
    let mut issues = Vec::new();
    for (reference, attached) in ends {
        if let Some((parent_name, lane_index, parent_t)) = map::lane_attachment(program, reference)
        {
            let Some(parent) = projection
                .segments
                .iter()
                .find(|segment| segment.name.as_deref() == Some(&parent_name))
            else {
                continue;
            };
            let Some(parent_road) = roads.get(parent.road_name.as_str()) else {
                continue;
            };
            if parent_road
                .elements
                .get(lane_index)
                .map(|element| element.kind)
                != Some(RoadElementKind::CarLane)
            {
                continue;
            }
            let Some(parent_point) =
                map::element_position(parent, parent_road, lane_index, parent_t)
            else {
                continue;
            };
            for (segment, t) in attached {
                let Some(road) = roads.get(segment.road_name.as_str()) else {
                    continue;
                };
                let connected = car_lane_indices(road).any(|lane| {
                    let ramp_tangent = segment.tangent_at(t);
                    let parent_tangent = parent.tangent_at(parent_t);
                    let ramp_sign = if road.lane_direction(lane) == Some(RoadDirection::Reverse) {
                        -1.0
                    } else {
                        1.0
                    };
                    let parent_sign =
                        if parent_road.lane_direction(lane_index) == Some(RoadDirection::Reverse) {
                            -1.0
                        } else {
                            1.0
                        };
                    let ramp_travel = map::WorldPoint {
                        x: ramp_tangent.x * ramp_sign,
                        y: ramp_tangent.y * ramp_sign,
                    };
                    let parent_travel = map::WorldPoint {
                        x: parent_tangent.x * parent_sign,
                        y: parent_tangent.y * parent_sign,
                    };
                    map::element_position(segment, road, lane, t)
                        .is_some_and(|point| distance(point, parent_point) <= 0.035)
                        && tangent_match(
                            ramp_travel,
                            parent_travel,
                            road.lane_direction(lane) == Some(RoadDirection::Both),
                        )
                }) && (segment.height_at(t) - parent.height_at(parent_t)).abs()
                    <= 0.005;
                if !connected {
                    issues.push(DesignIssue {
                        rule: DesignRule::VehicleLaneDiscontinuity,
                        message: format!(
                            "{} car lane does not meet {} lane {} at the same position and height",
                            segment_name(segment),
                            parent_name,
                            lane_index
                        ),
                        at: segment.point_at(t),
                        roads: [segment_name(segment).into(), parent_name.clone()],
                    });
                }
            }
            continue;
        }
        if attached.len() < 2
            || intersection_centers.contains(reference)
            || map::intersection_port_direction(program, reference).is_some()
        {
            continue;
        }
        for (segment, t) in &attached {
            let Some(road) = roads.get(segment.road_name.as_str()) else {
                continue;
            };
            for lane in car_lane_indices(road) {
                let Some(point) = map::element_position(segment, road, lane, *t) else {
                    continue;
                };
                let connected = attached.iter().any(|(other, other_t)| {
                    if std::ptr::eq(*segment, *other) {
                        return false;
                    }
                    let Some(other_road) = roads.get(other.road_name.as_str()) else {
                        return false;
                    };
                    car_lane_indices(other_road).any(|other_lane| {
                        map::element_position(other, other_road, other_lane, *other_t).is_some_and(
                            |other_point| {
                                let travel_matches = match (
                                    road.lane_direction(lane)
                                        .and_then(|direction| enters_seam(direction, *t)),
                                    other_road
                                        .lane_direction(other_lane)
                                        .and_then(|direction| enters_seam(direction, *other_t)),
                                ) {
                                    (Some(a), Some(b)) => a != b,
                                    _ => true,
                                };
                                let a_tangent = segment.tangent_at(*t);
                                let b_tangent = other.tangent_at(*other_t);
                                let a_sign =
                                    if road.lane_direction(lane) == Some(RoadDirection::Reverse) {
                                        -1.0
                                    } else {
                                        1.0
                                    };
                                let b_sign = if other_road.lane_direction(other_lane)
                                    == Some(RoadDirection::Reverse)
                                {
                                    -1.0
                                } else {
                                    1.0
                                };
                                let allow_reverse = road.lane_direction(lane)
                                    == Some(RoadDirection::Both)
                                    || other_road.lane_direction(other_lane)
                                        == Some(RoadDirection::Both);
                                travel_matches
                                    && tangent_match(
                                        map::WorldPoint {
                                            x: a_tangent.x * a_sign,
                                            y: a_tangent.y * a_sign,
                                        },
                                        map::WorldPoint {
                                            x: b_tangent.x * b_sign,
                                            y: b_tangent.y * b_sign,
                                        },
                                        allow_reverse,
                                    )
                                    && distance(point, other_point) <= 0.0225
                                    && (segment.height_at(*t) - other.height_at(*other_t)).abs()
                                        <= 0.005
                            },
                        )
                    })
                });
                if !connected {
                    issues.push(DesignIssue {
                        rule: DesignRule::VehicleLaneDiscontinuity,
                        message: format!(
                            "{} lane {} has no car-lane continuation at {}",
                            segment_name(segment),
                            lane,
                            reference
                        ),
                        at: segment.point_at(*t),
                        roads: [segment_name(segment).into(), String::new()],
                    });
                }
            }
        }
    }
    issues
}

fn closest_segment_parameter(segment: &map::Segment, point: map::WorldPoint) -> (f32, f32) {
    const STEPS: usize = 32;
    let mut best = (0.0, f32::INFINITY);
    for index in 0..STEPS {
        let start = segment.point_at(index as f32 / STEPS as f32);
        let end = segment.point_at((index + 1) as f32 / STEPS as f32);
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length2 = dx * dx + dy * dy;
        if length2 < 0.00000001 {
            continue;
        }
        let local =
            (((point.x - start.x) * dx + (point.y - start.y) * dy) / length2).clamp(0.0, 1.0);
        let nearest = map::WorldPoint {
            x: start.x + dx * local,
            y: start.y + dy * local,
        };
        let d = distance(point, nearest);
        if d < best.1 {
            best = ((index as f32 + local) / STEPS as f32, d);
        }
    }
    best
}

fn inside_segment_end_planes(segment: &map::Segment, point: map::WorldPoint, t: f32) -> bool {
    if t < 0.0001 {
        let tangent = segment.tangent_at(0.0);
        return (point.x - segment.start.x) * tangent.x + (point.y - segment.start.y) * tangent.y
            >= -0.0001;
    }
    if t > 0.9999 {
        let tangent = segment.tangent_at(1.0);
        return (point.x - segment.end.x) * tangent.x + (point.y - segment.end.y) * tangent.y
            <= 0.0001;
    }
    true
}

fn mouth_overlays_parent(
    source: &map::Segment,
    source_road: &RoadDefinition,
    other: &map::Segment,
    program: &Program,
    projection: &map::Projection,
) -> bool {
    if road_network_role(source_road) != RoadNetworkRole::RampMouth
        || !crate::road_profiles::merge_profile_is_paved(source_road)
    {
        return false;
    }
    [source.start_ref.as_deref(), source.end_ref.as_deref()]
        .into_iter()
        .flatten()
        .any(|reference| {
            let Some((parent_name, _, parent_t)) = map::lane_attachment(program, reference) else {
                return false;
            };
            let Some(parent) = projection
                .segments
                .iter()
                .find(|segment| segment.name.as_deref() == Some(&parent_name))
            else {
                return false;
            };
            if std::ptr::eq(parent, other) {
                return true;
            }
            if let Some(road) = crate::road_profiles::road_for_segment(program, parent)
                .filter(|_| parent.road_name == other.road_name)
            {
                let index = projection
                    .segments
                    .iter()
                    .position(|s| std::ptr::eq(s, parent))
                    .unwrap();
                if crate::road_profiles::attachment_footprint(projection, index, road)
                    .segments
                    .iter()
                    .any(|s| s.source == other.source)
                {
                    return true;
                }
            }
            let seam = if parent_t < 0.001 {
                parent.start_ref.as_deref()
            } else if parent_t > 0.999 {
                parent.end_ref.as_deref()
            } else {
                None
            };
            seam.is_some_and(|seam| {
                other.start_ref.as_deref() == Some(seam) || other.end_ref.as_deref() == Some(seam)
            })
        })
}

/// Find car-lane center paths passing through another road's non-car surface.
/// A lane attachment is an intentional merge throat; the attached parent's
/// surface is replaced by the ramp mouth locally.
pub fn check_vehicle_lane_intrusions(
    program: &Program,
    projection: &map::Projection,
) -> Vec<DesignIssue> {
    let roads = road_definitions(program);
    let junctions = map::intersections(program, projection);
    let mut issues = Vec::new();
    let mut reported = std::collections::HashSet::new();
    let surfaces: Vec<_> = projection
        .segments
        .iter()
        .map(|segment| {
            let road = roads.get(segment.road_name.as_str())?;
            let width: f32 = road
                .elements
                .iter()
                .map(|element| map::element_world_width(element.kind))
                .sum();
            let mut min_x = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            for p in [
                segment.start,
                segment.controls[0],
                segment.controls[1],
                segment.end,
            ] {
                min_x = min_x.min(p.x);
                max_x = max_x.max(p.x);
                min_y = min_y.min(p.y);
                max_y = max_y.max(p.y);
            }
            Some((
                min_x - width / 2.0,
                max_x + width / 2.0,
                min_y - width / 2.0,
                max_y + width / 2.0,
                width,
            ))
        })
        .collect();
    for (source_index, source) in projection.segments.iter().enumerate() {
        let Some(source_road) = roads.get(source.road_name.as_str()) else {
            continue;
        };
        for lane in car_lane_indices(source_road) {
            for step in 1..100 {
                let t = step as f32 / 100.0;
                let Some(point) = map::element_position(source, source_road, lane, t) else {
                    continue;
                };
                let source_z = source.height_at(t);
                if junctions
                    .iter()
                    .any(|junction| distance(point, junction.position) < 0.16)
                {
                    continue;
                }
                for (other_index, other) in projection.segments.iter().enumerate() {
                    if source_index == other_index
                        || reported.contains(&(source_index, lane, other_index))
                    {
                        continue;
                    }
                    let Some(other_road) = roads.get(other.road_name.as_str()) else {
                        continue;
                    };
                    let Some((min_x, max_x, min_y, max_y, width)) = surfaces[other_index] else {
                        continue;
                    };
                    if point.x < min_x || point.x > max_x || point.y < min_y || point.y > max_y {
                        continue;
                    }
                    let (other_t, center_distance) = closest_segment_parameter(other, point);
                    if center_distance > width / 2.0 + 0.025
                        || !inside_segment_end_planes(other, point, other_t)
                    {
                        continue;
                    }
                    let intentional_attachment =
                        mouth_overlays_parent(source, source_road, other, program, projection);
                    if intentional_attachment {
                        let height_difference = source_z - other.height_at(other_t);
                        if height_difference.abs() > 0.01 {
                            reported.insert((source_index, lane, other_index));
                            issues.push(DesignIssue {
                                rule: DesignRule::VehicleLaneMergeHeightMismatch,
                                message: format!(
                                    "{} merge lane is {:.3} units {} {}'s surface",
                                    segment_name(source),
                                    height_difference.abs(),
                                    if height_difference < 0.0 {
                                        "below"
                                    } else {
                                        "above"
                                    },
                                    segment_name(other)
                                ),
                                at: point,
                                roads: [segment_name(source).into(), segment_name(other).into()],
                            });
                        }
                        continue;
                    }
                    if (source_z - other.height_at(other_t)).abs() > 0.04 {
                        continue;
                    }
                    let tangent = other.tangent_at(other_t);
                    let length = (tangent.x * tangent.x + tangent.y * tangent.y).sqrt();
                    if length < 0.0001 {
                        continue;
                    }
                    let center = other.point_at(other_t);
                    let offset = ((point.x - center.x) * tangent.y
                        - (point.y - center.y) * tangent.x)
                        / length;
                    let mut edge = -width / 2.0;
                    for element in &other_road.elements {
                        let band_width = map::element_world_width(element.kind);
                        let inside = offset > edge + 0.003 && offset < edge + band_width - 0.003;
                        let paved_merge_shoulder = element.kind == RoadElementKind::Verge
                            && mouth_overlays_parent(
                                other, other_road, source, program, projection,
                            );
                        if inside
                            && element.kind != RoadElementKind::CarLane
                            && !paved_merge_shoulder
                        {
                            reported.insert((source_index, lane, other_index));
                            issues.push(DesignIssue {
                                rule: DesignRule::VehicleLaneCrossesNonCarBand,
                                message: format!(
                                    "{} lane {} crosses {} {:?} at the same height",
                                    segment_name(source),
                                    lane,
                                    segment_name(other),
                                    element.kind
                                ),
                                at: point,
                                roads: [segment_name(source).into(), segment_name(other).into()],
                            });
                            break;
                        }
                        edge += band_width;
                    }
                }
            }
        }
    }
    issues
}

fn segment_crossing_at_grade(a: &map::Segment, b: &map::Segment) -> Option<map::WorldPoint> {
    let bounds = |segment: &map::Segment| {
        let points = [
            segment.start,
            segment.controls[0],
            segment.controls[1],
            segment.end,
        ];
        points.iter().fold(
            (
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ),
            |(min_x, min_y, max_x, max_y), point| {
                (
                    min_x.min(point.x),
                    min_y.min(point.y),
                    max_x.max(point.x),
                    max_y.max(point.y),
                )
            },
        )
    };
    let (ax0, ay0, ax1, ay1) = bounds(a);
    let (bx0, by0, bx1, by1) = bounds(b);
    if ax0 > bx1 || bx0 > ax1 || ay0 > by1 || by0 > ay1 {
        return None;
    }
    const STEPS: usize = 12;
    for ai in 0..STEPS {
        let a0 = a.point_at(ai as f32 / STEPS as f32);
        let a1 = a.point_at((ai + 1) as f32 / STEPS as f32);
        let adx = a1.x - a0.x;
        let ady = a1.y - a0.y;
        for bi in 0..STEPS {
            let b0 = b.point_at(bi as f32 / STEPS as f32);
            let b1 = b.point_at((bi + 1) as f32 / STEPS as f32);
            let bdx = b1.x - b0.x;
            let bdy = b1.y - b0.y;
            let determinant = adx * bdy - ady * bdx;
            if determinant.abs() < 0.000001 {
                continue;
            }
            let dx = b0.x - a0.x;
            let dy = b0.y - a0.y;
            let along_a = (dx * bdy - dy * bdx) / determinant;
            let along_b = (dx * ady - dy * adx) / determinant;
            if !(0.0..=1.0).contains(&along_a) || !(0.0..=1.0).contains(&along_b) {
                continue;
            }
            let at_a = (ai as f32 + along_a) / STEPS as f32;
            let at_b = (bi as f32 + along_b) / STEPS as f32;
            if !(0.08..0.92).contains(&at_a) || !(0.08..0.92).contains(&at_b) {
                continue;
            }
            if (a.height_at(at_a) - b.height_at(at_b)).abs() < 0.04 {
                return Some(map::WorldPoint {
                    x: a0.x + adx * along_a,
                    y: a0.y + ady * along_a,
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn connected_ramp_passes_from_road_source() {
        let source = "road ramp: verge-carlane-verge;
    role ramp; end; road street: carlane; end; a = map_point(0, 0); b = map_point(1, 0); c = map_point(2, 0); d = map_point(3, 0); street(a, 0.3, 0, 0.7, 0, b); exit = ramp(b, 1.3, 0, 1.7, 0, c); street(c, 2.3, 0, 2.7, 0, d);";
        assert!(check_design_rules(&parse(source).unwrap()).is_empty());
    }

    #[test]
    fn disconnected_ramp_end_is_reported_from_road_source() {
        let source = "road ramp: verge-carlane-verge;
    role ramp; end; road street: carlane; end; a = map_point(0, 0); b = map_point(1, 0); near = map_point(1.02, 0); c = map_point(2, 0); exit = ramp(a, 0.3, 0, 0.7, 0, b); street(near, 1.3, 0, 1.7, 0, c);";
        let issues = check_design_rules(&parse(source).unwrap());
        assert!(issues
            .iter()
            .any(|issue| issue.rule == DesignRule::UnconnectedRampEnd
                && issue.at == map::WorldPoint { x: 1.0, y: 0.0 }));
    }

    #[test]
    fn same_grade_crossing_requires_height_or_junction() {
        let source = "road ramp: verge-carlane-verge;
    role ramp; end; road street: carlane; end; a = map_point(-2, 0); b = map_point(2, 0); c = map_point(0, -2); d = map_point(0, 2); exit = ramp(a, -0.7, 0, 0.7, 0, b); street(c, 0, -0.7, 0, 0.7, d);";
        let issues = check_design_rules(&parse(source).unwrap());
        assert!(issues
            .iter()
            .any(|issue| issue.rule == DesignRule::RampAtGradeCrossing));
        let bridged = parse(&format!("{source} elevation(exit, 0.2);")).unwrap();
        assert!(!check_design_rules(&bridged)
            .iter()
            .any(|issue| issue.rule == DesignRule::RampAtGradeCrossing));
    }

    #[test]
    fn shared_endpoint_does_not_hide_a_crossing_farther_along() {
        let source = "road ramp: verge-carlane-verge;
    role ramp; end; road street: carlane; end; a = map_point(0, 0); b = map_point(4, 0); exit = ramp(a, 1, 2, 3, 2, b); street(a, 1, -2, 3, 4, b);";
        let issues = check_design_rules(&parse(source).unwrap());
        assert!(issues
            .iter()
            .any(|issue| issue.rule == DesignRule::RampAtGradeCrossing));
    }

    #[test]
    fn car_lane_height_step_is_reported() {
        let source = "road street: carlane; direction forward; end; a = map_point(0, 0); seam = map_point(1, 0); b = map_point(2, 0); first = street(a, 0.3, 0, 0.7, 0, seam); second = street(seam, 1.3, 0, 1.7, 0, b); elevation(second, 0.2);";
        let issues = check_design_rules(&parse(source).unwrap());
        assert!(issues
            .iter()
            .any(|issue| issue.rule == DesignRule::VehicleLaneDiscontinuity
                && issue.roads[0] == "first"));
    }

    #[test]
    fn oncoming_car_lane_seam_is_reported() {
        let source = "road street: carlane; direction forward; end; a = map_point(0, 0); seam = map_point(1, 0); b = map_point(2, 0); first = street(a, 0.3, 0, 0.7, 0, seam); second = street(b, 1.7, 0, 1.3, 0, seam);";
        let issues = check_design_rules(&parse(source).unwrap());
        assert!(issues
            .iter()
            .any(|issue| issue.rule == DesignRule::VehicleLaneDiscontinuity));
    }

    #[test]
    fn car_path_through_sidewalk_is_reported() {
        let source = "road street: sidewalk-carlane-sidewalk; end; road crossing_ramp: carlane; direction forward;
    role ramp; end; a = map_point(-1, 0); b = map_point(1, 0); c = map_point(0, -1); d = map_point(0, 1); street(a, -0.5, 0, 0.5, 0, b); crossing_ramp(c, 0, -0.5, 0, 0.5, d);";
        let issues = check_design_rules(&parse(source).unwrap());
        assert!(issues
            .iter()
            .any(|issue| issue.rule == DesignRule::VehicleLaneCrossesNonCarBand));
    }
}
