//! Bake finite contacts into clipped junction/yard pavement. Physical City data is untouched.
use super::super::{Mesh, SurfaceKind, Vertex};
use std::collections::HashMap;

const CELL: f32 = 0.04;
#[cfg(test)]
mod tests;
const SPREAD: f32 = super::TILE_SPREAD;
#[derive(Clone, Copy)]
struct Edge {
    a: [f32; 3],
    b: [f32; 3],
    outward: [f32; 2],
    drop: f32,
    texture_along: [f32; 2],
}
impl Edge {
    fn coordinates(self, p: [f32; 3]) -> [f32; 2] {
        let d = [self.b[0] - self.a[0], self.b[1] - self.a[1]];
        let length = d[0].hypot(d[1]);
        [
            (d[0] * (p[0] - self.a[0]) + d[1] * (p[1] - self.a[1])) / length,
            self.outward[0] * (p[0] - self.a[0]) + self.outward[1] * (p[1] - self.a[1]),
        ]
    }
    fn length(self) -> f32 {
        (self.b[0] - self.a[0]).hypot(self.b[1] - self.a[1])
    }
}
fn cell(p: [f32; 3]) -> (i32, i32) {
    ((p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32)
}
fn top(v: &[Vertex]) -> bool {
    let p = v[0].position;
    let a = v[1].position;
    let b = v[2].position;
    let cross = (a[0] - p[0]) * (b[1] - p[1]) - (a[1] - p[1]) * (b[0] - p[0]);
    cross.abs() > 1e-14
}
fn interpolate(a: Vertex, b: Vertex, t: f32) -> Vertex {
    let mix = |a: f32, b: f32| a + (b - a) * t;
    Vertex {
        position: std::array::from_fn(|i| mix(a.position[i], b.position[i])),
        normal: std::array::from_fn(|i| mix(a.normal[i], b.normal[i])),
        color: std::array::from_fn(|i| mix(a.color[i], b.color[i])),
        surface: std::array::from_fn(|i| mix(a.surface[i], b.surface[i])),
    }
}
fn split(poly: &[Vertex], plane: impl Fn([f32; 3]) -> f32) -> (Vec<Vertex>, Vec<Vertex>) {
    let mut inside = Vec::new();
    let mut outside = Vec::new();
    for (&a, &b) in poly
        .iter()
        .zip(poly.iter().cycle().skip(1))
        .take(poly.len())
    {
        let d = plane(a.position);
        let e = plane(b.position);
        if d >= 0. {
            inside.push(a);
        }
        if d <= 0. {
            outside.push(a);
        }
        if (d < 0. && e > 0.) || (d > 0. && e < 0.) {
            let v = interpolate(a, b, d / (d - e));
            inside.push(v);
            outside.push(v);
        }
    }
    (inside, outside)
}
fn emit(output: &mut Vec<Vertex>, poly: &[Vertex], edge: Option<Edge>) {
    for i in 1..poly.len().saturating_sub(1) {
        let mut t = [poly[0], poly[i], poly[i + 1]];
        if !top(&t) {
            continue;
        }
        if let Some(e) = edge {
            for v in &mut t {
                let uv = e.coordinates(v.position);
                let along = e.texture_along[0]
                    + (e.texture_along[1] - e.texture_along[0]) * uv[0] / e.length();
                v.surface[1..].copy_from_slice(&[along, uv[1], -4. - e.length()]);
            }
        }
        output.extend_from_slice(&t);
    }
}
/// Split only legacy horizontal pavement at real raised contacts. Ordinary road bands
/// already carry edge coordinates and are left byte-for-byte intact. Call once per mesh
/// preparation, before GPU upload; never per frame. No decals, extra draws or collision edits.
pub fn prepare_mesh(mesh: &mut Mesh) {
    let sidewalk = SurfaceKind::Sidewalk as u32 as f32;
    let asphalt = SurfaceKind::Asphalt as u32 as f32;
    let prop = SurfaceKind::Prop as u32 as f32;
    if !mesh.vertices.chunks_exact(3).any(|t| {
        top(t)
            && (t[0].surface[0] == asphalt || t[0].surface[0] == sidewalk)
            && t[0].surface[3] >= 0.
            && t[0].surface[3] <= 2.
    }) {
        return;
    }
    let mut boundaries: HashMap<[[i32; 3]; 2], (Edge, u32)> = HashMap::new();
    let mut edges = Vec::new();
    for t in mesh.vertices.chunks_exact(3) {
        if t[0].surface[0] == sidewalk && top(t) {
            for [i, j, k] in [[0, 1, 2], [1, 2, 0], [2, 0, 1]] {
                let a = t[i].position;
                let b = t[j].position;
                let p = t[k].position;
                let d = [b[0] - a[0], b[1] - a[1]];
                let length = d[0].hypot(d[1]);
                if length < 1e-7 {
                    continue;
                }
                let mut outward = [-d[1] / length, d[0] / length];
                if outward[0] * (p[0] - a[0]) + outward[1] * (p[1] - a[1]) > 0. {
                    outward = outward.map(|v| -v);
                }
                let mut key = [a, b].map(|p| p.map(|v| (v * 1e7).round() as i32));
                key.sort_unstable();
                let texture_along = if (t[i].surface[1] - t[j].surface[1]).abs() > 1e-7 {
                    [t[i].surface[1], t[j].surface[1]]
                } else {
                    let axis = usize::from(d[1].abs() > d[0].abs());
                    [a[axis], b[axis]]
                };
                boundaries.entry(key).and_modify(|v| v.1 += 1).or_insert((
                    Edge {
                        a,
                        b,
                        outward,
                        drop: 0.0015,
                        texture_along,
                    },
                    1,
                ));
            }
        } else if (t[0].surface[0] == prop || t[0].surface[0] == sidewalk) && t[0].surface[3] == -2.
        {
            for [i, j] in [[0, 1], [1, 2], [2, 0]] {
                if t[i].surface[2] > 1e-6 || t[j].surface[2] > 1e-6 {
                    continue;
                }
                let a = t[i].position;
                let b = t[j].position;
                let d = [b[0] - a[0], b[1] - a[1]];
                let length = d[0].hypot(d[1]);
                if length < 1e-7 {
                    continue;
                }
                let outward = [-d[1] / length, d[0] / length];
                let texture_along = [t[i].surface[1], t[j].surface[1]];
                edges.push(Edge {
                    a,
                    b,
                    outward,
                    drop: 0.,
                    texture_along,
                });
                edges.push(Edge {
                    a,
                    b,
                    outward: outward.map(|v| -v),
                    drop: 0.,
                    texture_along,
                });
            }
        }
    }
    edges.extend(
        boundaries
            .into_values()
            .filter_map(|(e, n)| (n == 1).then_some(e)),
    );
    // Stable ordering prevents randomized hash iteration from changing corner ownership.
    edges.sort_by(|a, b| {
        a.a.iter()
            .chain(a.b.iter())
            .zip(b.a.iter().chain(b.b.iter()))
            .find_map(|(a, b)| {
                let c = a.total_cmp(b);
                (!c.is_eq()).then_some(c)
            })
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (id, e) in edges.iter().enumerate() {
        let steps = (e.length() / CELL).ceil().max(1.) as usize;
        for step in 0..=steps {
            let p =
                std::array::from_fn(|i| e.a[i] + (e.b[i] - e.a[i]) * step as f32 / steps as f32);
            let (x, y) = cell(p);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let ids = grid.entry((x + dx, y + dy)).or_default();
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
    }
    let mut replacements = HashMap::new();
    for (id, t) in mesh.vertices.chunks_exact(3).enumerate() {
        if !top(t)
            || (t[0].surface[0] != asphalt && t[0].surface[0] != sidewalk)
            || t[0].surface[3] < 0.
            || t[0].surface[3] > 2.
        {
            continue;
        }
        let lo = cell(std::array::from_fn(|i| {
            t.iter()
                .map(|v| v.position[i])
                .fold(f32::INFINITY, f32::min)
        }));
        let hi = cell(std::array::from_fn(|i| {
            t.iter()
                .map(|v| v.position[i])
                .fold(f32::NEG_INFINITY, f32::max)
        }));
        let mut candidates = Vec::new();
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                if let Some(ids) = grid.get(&(x, y)) {
                    candidates.extend_from_slice(ids);
                }
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        let mut remaining = vec![t.to_vec()];
        let mut output = Vec::new();
        let mut changed = false;
        for edge in candidates.into_iter().map(|i| edges[i]) {
            let mut next = Vec::new();
            for poly in remaining {
                let uv: Vec<_> = poly.iter().map(|v| edge.coordinates(v.position)).collect();
                if uv.iter().all(|p| p[0] <= 0.)
                    || uv.iter().all(|p| p[0] >= edge.length())
                    || uv.iter().all(|p| p[1] <= 0.)
                    || uv.iter().all(|p| p[1] >= SPREAD)
                {
                    next.push(poly);
                    continue;
                }
                let original = poly.clone();
                let mut inside = poly;
                let mut outside = Vec::new();
                for (axis, sign, limit) in [
                    (0, 1., 0.),
                    (0, -1., edge.length()),
                    (1, 1., 0.),
                    (1, -1., SPREAD),
                ] {
                    let (a, b) = split(&inside, |p| edge.coordinates(p)[axis] * sign + limit);
                    inside = a;
                    if b.len() >= 3 {
                        outside.push(b);
                    }
                    if inside.len() < 3 {
                        break;
                    }
                }
                let center = std::array::from_fn(|i| {
                    inside.iter().map(|v| v.position[i]).sum::<f32>() / inside.len().max(1) as f32
                });
                let center_uv = edge.coordinates(center);
                let supported = inside.len() >= 3
                    && inside.windows(3).any(top)
                    && center_uv[0] > 1e-7
                    && center_uv[0] < edge.length() - 1e-7
                    && center_uv[1] > 1e-7
                    && center_uv[1] < SPREAD - 1e-7
                    && inside.iter().all(|v| {
                        let along = (edge.coordinates(v.position)[0] / edge.length()).clamp(0., 1.);
                        let foot = edge.a[2] + (edge.b[2] - edge.a[2]) * along - edge.drop;
                        (v.position[2] - foot).abs() < 0.0003
                    });
                if supported {
                    emit(&mut output, &inside, Some(edge));
                    next.extend(outside);
                    changed = true;
                } else {
                    next.push(original);
                }
            }
            remaining = next;
        }
        if changed {
            for poly in remaining {
                emit(&mut output, &poly, None);
            }
            replacements.insert(id, output);
        }
    }
    if replacements.is_empty() {
        return;
    }
    let source = std::mem::take(&mut mesh.vertices);
    mesh.vertices.reserve(source.len());
    for (id, t) in source.chunks_exact(3).enumerate() {
        if let Some(replacement) = replacements.remove(&id) {
            mesh.vertices.extend(replacement);
        } else {
            mesh.vertices.extend_from_slice(t);
        }
    }
}
