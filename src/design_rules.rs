//! Design checks for a parsed `.road` program.
//!
//! These checks are advisory: parsing does not reject a road network with design issues.

use std::collections::HashMap;

use crate::{map, Program, RoadDefinition, Statement};

/// A design rule that applies to road network geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesignRule {
    UnconnectedRampEnd,
    RampAtGradeCrossing,
}

impl DesignRule {
    pub fn code(self) -> &'static str {
        match self {
            Self::UnconnectedRampEnd => "unconnected_ramp_end",
            Self::RampAtGradeCrossing => "ramp_at_grade_crossing",
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
    match road.name.as_str() {
        "ramp_mouth" => RoadNetworkRole::RampMouth,
        "ramp" => RoadNetworkRole::Ramp,
        name if name.ends_with("_ramp") => RoadNetworkRole::Ramp,
        _ => RoadNetworkRole::Other,
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
        let source = "road ramp: verge-carlane-verge; end; road street: carlane; end; a = map_point(0, 0); b = map_point(1, 0); c = map_point(2, 0); d = map_point(3, 0); street(a, 0.3, 0, 0.7, 0, b); exit = ramp(b, 1.3, 0, 1.7, 0, c); street(c, 2.3, 0, 2.7, 0, d);";
        assert!(check_design_rules(&parse(source).unwrap()).is_empty());
    }

    #[test]
    fn disconnected_ramp_end_is_reported_from_road_source() {
        let source = "road ramp: verge-carlane-verge; end; road street: carlane; end; a = map_point(0, 0); b = map_point(1, 0); near = map_point(1.02, 0); c = map_point(2, 0); exit = ramp(a, 0.3, 0, 0.7, 0, b); street(near, 1.3, 0, 1.7, 0, c);";
        let issues = check_design_rules(&parse(source).unwrap());
        assert!(issues
            .iter()
            .any(|issue| issue.rule == DesignRule::UnconnectedRampEnd
                && issue.at == map::WorldPoint { x: 1.0, y: 0.0 }));
    }

    #[test]
    fn same_grade_crossing_requires_height_or_junction() {
        let source = "road ramp: verge-carlane-verge; end; road street: carlane; end; a = map_point(-2, 0); b = map_point(2, 0); c = map_point(0, -2); d = map_point(0, 2); exit = ramp(a, -0.7, 0, 0.7, 0, b); street(c, 0, -0.7, 0, 0.7, d);";
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
        let source = "road ramp: verge-carlane-verge; end; road street: carlane; end; a = map_point(0, 0); b = map_point(4, 0); exit = ramp(a, 1, 2, 3, 2, b); street(a, 1, -2, 3, 4, b);";
        let issues = check_design_rules(&parse(source).unwrap());
        assert!(issues
            .iter()
            .any(|issue| issue.rule == DesignRule::RampAtGradeCrossing));
    }
}
