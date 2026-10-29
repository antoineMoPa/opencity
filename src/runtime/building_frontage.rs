//! Shared facade frames for generic building consumers.
use super::{City, Mesh, SurfaceKind};
use crate::LandscapeFeature;

#[derive(Clone, Copy, Debug)]
pub struct BuildingFrontage {
    pub origin: [f32; 2],
    pub along: [f32; 2],
    pub outward: [f32; 2],
    pub width: f32,
    pub depth: f32,
    pub base: f32,
}
pub(super) type Front = BuildingFrontage;
impl BuildingFrontage {
    pub fn point(self, x: f32, y: f32, z: f32) -> [f32; 3] {
        [
            self.origin[0] + self.along[0] * x + self.outward[0] * y,
            self.origin[1] + self.along[1] * x + self.outward[1] * y,
            self.base + z,
        ]
    }
    pub(super) fn rotate(self, mesh: &mut Mesh, start: usize) {
        for vertex in &mut mesh.vertices[start..] {
            let p = vertex.position;
            vertex.position = self.point(p[0], p[1], p[2]);
            let n = vertex.normal;
            vertex.normal = [
                self.along[0] * n[0] + self.outward[0] * n[1],
                self.along[1] * n[0] + self.outward[1] * n[1],
                n[2],
            ];
            if vertex.surface[0] == SurfaceKind::Window as u32 as f32
                && vertex.surface[3] >= 16.
                && self.along[0] * self.outward[1] - self.along[1] * self.outward[0] < 0.
            {
                vertex.surface[1] = 1. - vertex.surface[1];
            }
        }
    }
}

fn dist2(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1])
        / (d[0] * d[0] + d[1] * d[1]).max(1e-12))
    .clamp(0., 1.);
    (p[0] - a[0] - d[0] * t).powi(2) + (p[1] - a[1] - d[1] * t).powi(2)
}
pub(super) fn front_for(city: &City, feature: &LandscapeFeature, reach: f32) -> Option<Front> {
    let count = feature.points.len();
    if count < 3 {
        return None;
    }
    let signed_area: f32 = feature
        .points
        .iter()
        .zip(feature.points.iter().cycle().skip(1))
        .take(count)
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum();
    if !signed_area.is_finite() || signed_area.abs() <= 1e-10 {
        return None;
    }

    let mut best: Option<(f32, f32, [f32; 2], [f32; 2])> = None;
    for (edge, (&a, &b)) in feature
        .points
        .iter()
        .zip(feature.points.iter().cycle().skip(1))
        .take(count)
        .enumerate()
    {
        if feature.front_edge.is_some_and(|selected| selected != edge) {
            continue;
        }
        let width = (b[0] - a[0]).hypot(b[1] - a[1]);
        if width < 1e-6 {
            continue;
        }
        let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let mut closest = f32::INFINITY;
        for road in &city.roads {
            let bounds = road.bounds;
            if mid[0] < bounds[0] - reach
                || mid[0] > bounds[2] + reach
                || mid[1] < bounds[1] - reach
                || mid[1] > bounds[3] + reach
            {
                continue;
            }
            let mut prev = road.segment.point_at(0.);
            for step in 1..=12 {
                let t = step as f32 / 12.;
                let next = road.segment.point_at(t);
                // Avoid facing a highway deck far above or below this footprint.
                if (road.segment.height_at(t) - feature.elevation).abs() <= reach {
                    closest = closest.min(dist2(mid, [prev.x, prev.y], [next.x, next.y]));
                }
                prev = next;
            }
        }
        // With no nearby road, prefer the longest facade rather than relying on polygon order.
        let score = if closest.is_finite() {
            closest
        } else {
            reach * reach + 1. / width
        };
        if best.as_ref().is_none_or(|old| score < old.0) {
            best = Some((score, width, a, b));
        }
    }
    let (_, width, a, b) = best?;
    let along = [(b[0] - a[0]) / width, (b[1] - a[1]) / width];
    // Polygon winding defines the outside even for concave footprints.
    let outward = if signed_area > 0.0 {
        [along[1], -along[0]]
    } else {
        [-along[1], along[0]]
    };
    let depth = feature
        .points
        .iter()
        .map(|p| -((p[0] - a[0]) * outward[0] + (p[1] - a[1]) * outward[1]))
        .fold(0., f32::max);
    Some(Front {
        origin: a,
        along,
        outward,
        width,
        depth,
        base: feature.elevation,
    })
}
