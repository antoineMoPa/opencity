//! Locate the visible approach seams beyond short, fully clipped road pieces.
use super::*;
use std::collections::{HashMap, HashSet};

impl RoadGeometry {
    pub(super) fn junction_mouth(
        projection: &Projection,
        junction: &map::Intersection,
        edge: [Vertex; 2],
        seams: &[(usize, Vertex, Vertex)],
    ) -> bool {
        let rotation = map::intersection_rotation(projection, junction);
        let local = |p: WorldPoint| {
            map::junction_local_direction(
                WorldPoint {
                    x: p.x - junction.position.x,
                    y: p.y - junction.position.y,
                },
                rotation,
            )
        };
        let edge = edge.map(|v| local(v.point));
        seams.iter().any(|(_, a, b)| {
            let center = local(WorldPoint {
                x: (a.point.x + b.point.x) / 2.,
                y: (a.point.y + b.point.y) / 2.,
            });
            let horizontal = center.x.abs() > center.y.abs();
            let sign = (if horizontal { center.x } else { center.y }).signum();
            edge.iter().all(|p| {
                let coordinate = if horizontal { p.x } else { p.y };
                (coordinate - sign * junction.footprint_half).abs() < 0.00005
            })
        })
    }

    pub fn junction_seams(
        &self,
        projection: &Projection,
        junction: &map::Intersection,
    ) -> Vec<(usize, Vertex, Vertex)> {
        let rotation = map::intersection_rotation(projection, junction);
        let inside = |p: WorldPoint| {
            let p = map::junction_local_direction(
                WorldPoint {
                    x: p.x - junction.position.x,
                    y: p.y - junction.position.y,
                },
                rotation,
            );
            p.x.abs().max(p.y.abs()) <= junction.footprint_half + 0.000001
        };
        let mut endpoints: HashMap<&str, Vec<(usize, bool)>> = HashMap::new();
        for (index, segment) in projection.segments.iter().enumerate() {
            for (reference, end) in [(&segment.start_ref, false), (&segment.end_ref, true)] {
                if let Some(reference) = reference {
                    endpoints.entry(reference).or_default().push((index, end));
                }
            }
        }
        let mut pending: Vec<_> = std::iter::once(junction.point_ref.as_str())
            .chain(junction.ports.iter().map(|p| p.point_ref.as_str()))
            .flat_map(|r| endpoints.get(r).into_iter().flatten().copied())
            .collect();
        let mut visited = HashSet::new();
        let mut seams = Vec::new();
        while let Some((index, end)) = pending.pop() {
            if !visited.insert((index, end)) {
                continue;
            }
            let segment = &projection.segments[index];
            if !inside(if end { segment.end } else { segment.start }) {
                continue;
            }
            let other = if end { segment.start } else { segment.end };
            if inside(other) {
                let reference = if end {
                    &segment.start_ref
                } else {
                    &segment.end_ref
                };
                if let Some(reference) = reference {
                    pending.extend(
                        endpoints
                            .get(reference.as_str())
                            .into_iter()
                            .flatten()
                            .copied(),
                    );
                }
                continue;
            }
            let rows = self.cross_sections(index);
            if let Some(row) = if end { rows.last() } else { rows.first() } {
                if let (Some(&a), Some(&b)) = (row.boundaries.first(), row.boundaries.last()) {
                    seams.push((index, a, b));
                }
            }
        }
        seams
    }
}
