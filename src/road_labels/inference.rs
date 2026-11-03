//! Sparse name anchors propagated over conservative physical-road groups.
use super::graph::continuation_links;
use crate::{map, Program};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoadNameState {
    #[default]
    Unnamed,
    Named,
    ConflictingAnchors,
}

#[derive(Clone, Debug)]
pub struct RoadGroup {
    pub segments: Vec<usize>,
    pub anchors: Vec<usize>,
    pub state: RoadNameState,
    pub label_path: Vec<String>,
    pub route_labels: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ResolvedRoadNames {
    pub groups: Vec<RoadGroup>,
    pub segment_groups: Vec<usize>,
    pub boundaries: Vec<String>,
    pub ambiguous_nodes: Vec<String>,
    pub unused_boundaries: Vec<String>,
}

pub(crate) fn resolve_names(program: &Program, projection: &mut map::Projection) {
    let graph = continuation_links(program, projection);
    let mut neighbors = vec![Vec::new(); projection.segments.len()];
    for link in &graph.links {
        let [a, b] = link.segments;
        neighbors[a].push(b);
        neighbors[b].push(a);
    }
    let mut result = ResolvedRoadNames {
        segment_groups: vec![usize::MAX; neighbors.len()],
        boundaries: projection.road_names.boundaries.clone(),
        ambiguous_nodes: graph.ambiguous_nodes,
        unused_boundaries: graph.unused_boundaries,
        ..Default::default()
    };
    for first in 0..neighbors.len() {
        if result.segment_groups[first] != usize::MAX {
            continue;
        }
        let group_id = result.groups.len();
        let mut pending = vec![first];
        let mut members = Vec::new();
        while let Some(i) = pending.pop() {
            if result.segment_groups[i] != usize::MAX {
                continue;
            }
            result.segment_groups[i] = group_id;
            members.push(i);
            pending.extend(neighbors[i].iter().copied());
        }
        members.sort_unstable();
        let mut paths = Vec::new();
        let mut routes = Vec::new();
        let mut anchors = Vec::new();
        for &i in &members {
            let road = &projection.segments[i];
            if road.label_anchor || !road.route_labels.is_empty() {
                anchors.push(i);
            }
            if road.label_anchor && !road.label_path.is_empty() && !paths.contains(&road.label_path)
            {
                paths.push(road.label_path.clone());
            }
            for route in &road.route_labels {
                if !routes.contains(route) {
                    routes.push(route.clone());
                }
            }
        }
        let state = match paths.len() {
            0 if routes.is_empty() => RoadNameState::Unnamed,
            0 | 1 => RoadNameState::Named,
            _ => RoadNameState::ConflictingAnchors,
        };
        let path = if paths.len() == 1 {
            paths.remove(0)
        } else {
            Vec::new()
        };
        for &i in &members {
            let road = &mut projection.segments[i];
            if !path.is_empty() {
                road.label_path = path.clone();
            }
            road.route_labels = routes.clone();
        }
        result.groups.push(RoadGroup {
            segments: members,
            anchors,
            state,
            label_path: path,
            route_labels: routes,
        });
    }
    projection.road_names = result;
}
