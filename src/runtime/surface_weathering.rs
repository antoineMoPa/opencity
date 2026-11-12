//! Material-space pavement bands and concrete contact coordinates.
use super::{geometry, SurfaceKind, Triangle, Vertex};
use crate::RoadElementKind as K;
use crate::{road_geometry::CrossSection, RoadDefinition};

#[derive(Clone, Copy)]
enum Edge {
    Flush,
    Exposed,
    Raised(f32),
}
impl Edge {
    fn bits(self) -> u32 {
        match self {
            Self::Flush => 0,
            Self::Exposed => 1,
            Self::Raised(height) => 1 + (height * 10000.).round().clamp(1., 30.) as u32,
        }
    }
}
fn raised(kind: K) -> f32 {
    if kind == K::Barrier {
        0.011
    } else if geometry::kind(kind) == SurfaceKind::Sidewalk {
        0.0015
    } else {
        0.
    }
}
fn edge(kind: K, neighbor: Option<K>) -> Edge {
    let Some(neighbor) = neighbor else {
        return Edge::Exposed;
    };
    let height = raised(neighbor) - raised(kind);
    if height > 0.00005 {
        Edge::Raised(height)
    } else if geometry::kind(kind) == geometry::kind(neighbor) {
        Edge::Flush
    } else {
        Edge::Exposed
    }
}
fn packed(width: f32, layout: u32, left: Edge, right: Edge) -> f32 {
    let width = (width * 10000.).round().clamp(1., 4095.) as u32;
    (layout | width << 2 | left.bits() << 14 | right.bits() << 19) as f32
}
pub(super) fn band(
    triangles: &mut [Triangle],
    road: &RoadDefinition,
    rows: &[CrossSection],
    band: usize,
    distance: f32,
    length: f32,
) {
    let kind = road.elements[band].kind;
    let layout = match geometry::kind(kind) {
        SurfaceKind::Sidewalk => 2,
        SurfaceKind::Asphalt => 3,
        _ => return,
    };
    let width = |row: &CrossSection| {
        let a = row.boundaries[band].point;
        let b = row.boundaries[band + 1].point;
        (b.x - a.x).hypot(b.y - a.y)
    };
    let w = (width(&rows[0]) + width(&rows[1])) * 0.5;
    let left = edge(kind, band.checked_sub(1).map(|i| road.elements[i].kind));
    let right = edge(kind, road.elements.get(band + 1).map(|e| e.kind));
    let metadata = packed(w, layout, left, right);
    let uv = [
        [distance, 0.],
        [distance + length, 0.],
        [distance + length, w],
        [distance, w],
    ];
    for (t, ids) in triangles.iter_mut().zip([[0, 1, 2], [0, 2, 3]]) {
        t.data[2] = metadata;
        t.facade_uv = Some(ids.map(|i| uv[i]));
    }
}
pub(super) fn base(a: [f32; 3], b: [f32; 3], p: [f32; 3]) -> f32 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1])
        / (d[0] * d[0] + d[1] * d[1]).max(1e-12))
    .clamp(0., 1.);
    a[2] + (b[2] - a[2]) * t
}
fn coordinates(p: [f32; 3], n: [f32; 3], base: f32) -> [f32; 2] {
    [
        if n[1].abs() > n[0].abs() { p[0] } else { p[1] },
        (p[2] - base).max(0.),
    ]
}
fn normal(p: [[f32; 3]; 3]) -> [f32; 3] {
    let a: [f32; 3] = std::array::from_fn(|i| p[1][i] - p[0][i]);
    let b: [f32; 3] = std::array::from_fn(|i| p[2][i] - p[0][i]);
    let n = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let length = n.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-12);
    n.map(|v| v / length)
}
pub(super) fn concrete(t: &mut Triangle, ground: impl Fn([f32; 3]) -> f32) {
    let n = normal(t.points);
    t.render_normal = Some(n);
    t.data[2] = -2.;
    t.facade_uv = Some(t.points.map(|p| coordinates(p, n, ground(p))));
}
pub(super) fn concrete_mesh(vertices: &mut [Vertex], ground: impl Fn([f32; 3]) -> f32) {
    for triangle in vertices.chunks_exact_mut(3) {
        let n = normal(std::array::from_fn(|i| triangle[i].position));
        for v in triangle {
            v.normal = n;
            let uv = coordinates(v.position, n, ground(v.position));
            v.surface[1..].copy_from_slice(&[uv[0], uv[1], -2.]);
        }
    }
}
#[cfg(test)]
mod tests;
