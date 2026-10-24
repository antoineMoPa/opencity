//! Design rules for the city editor.
use super::*;

#[derive(Clone)]
pub(super) struct DesignIssue {
    pub(super) code: &'static str,
    pub(super) message: String,
    pub(super) at: WorldPoint,
    pub(super) roads: [String; 2],
}

pub(super) fn design_rule_issues(
    program: &Program,
    projection: &map::Projection,
    profiles: &[PaintProfile],
) -> Vec<DesignIssue> {
    let mut issues: Vec<DesignIssue> =
        opencity::check_design_rules_with_projection(program, projection)
            .into_iter()
            .map(|issue| DesignIssue {
                code: issue.rule.code(),
                message: issue.message,
                at: issue.at,
                roads: issue.roads,
            })
            .collect();
    for (incoming, outgoing) in road_joins(projection) {
        let a = &projection.segments[incoming];
        let b = &projection.segments[outgoing];
        if !straight_join(a, b) {
            continue;
        }
        let (Some(a_road), Some(b_road)) =
            (road_for_segment(program, a), road_for_segment(program, b))
        else {
            continue;
        };
        let (a_left, a_right) = section_edges(a_road, &profiles[incoming], 1.0);
        let (b_left, b_right) = section_edges(b_road, &profiles[outgoing], 0.0);
        if (a_left - b_left).abs() > 0.005 || (a_right - b_right).abs() > 0.005 {
            issues.push(DesignIssue {
                code: "road_edge_mismatch",
                message: format!(
                    "Road edges do not meet: {} → {}",
                    a.name.as_deref().unwrap_or(&a.road_name),
                    b.name.as_deref().unwrap_or(&b.road_name)
                ),
                at: a.end,
                roads: [
                    a.name.clone().unwrap_or_else(|| a.road_name.clone()),
                    b.name.clone().unwrap_or_else(|| b.road_name.clone()),
                ],
            });
        }
    }
    for intersection in map::intersections(program, projection) {
        let attached: Vec<_> = if intersection.ports.is_empty() {
            map::intersection_arms(projection, &intersection.point_ref)
                .into_iter()
                .map(|(index, is_end)| (index, is_end, None))
                .collect()
        } else {
            projection
                .segments
                .iter()
                .enumerate()
                .filter_map(|(index, segment)| {
                    intersection
                        .ports
                        .iter()
                        .find(|port| {
                            segment.start_ref.as_deref() == Some(&port.point_ref)
                                || segment.end_ref.as_deref() == Some(&port.point_ref)
                        })
                        .map(|port| {
                            (
                                index,
                                segment.end_ref.as_deref() == Some(&port.point_ref),
                                Some(port.direction),
                            )
                        })
                })
                .collect()
        };
        for (index, is_end, direction) in attached {
            let segment = &projection.segments[index];
            let Some(approach) = road_for_segment(program, segment) else {
                continue;
            };
            let edge_name = direction
                .map(|direction| intersection.road_name_for_direction(direction))
                .unwrap_or_else(|| intersection.road_name_for_segment(segment, is_end));
            let Some(edge) = program
                .statements
                .iter()
                .find_map(|statement| match statement {
                    Statement::Road(road) if road.name == edge_name => Some(road),
                    _ => None,
                })
            else {
                continue;
            };
            if !approach
                .elements
                .iter()
                .map(|element| element.kind)
                .eq(edge.elements.iter().map(|element| element.kind))
            {
                let name = segment.name.as_deref().unwrap_or(&segment.road_name);
                issues.push(DesignIssue {
                    code: "intersection_edge_mismatch",
                    message: format!(
                        "{name} must match {} before {}",
                        edge.name, intersection.point_ref
                    ),
                    at: intersection.position,
                    roads: [name.to_owned(), edge.name.clone()],
                });
            }
        }
    }
    issues
}
