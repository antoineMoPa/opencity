//! Excavation walls share the terrain-cut boundary for rendering and collision.
use super::*;
const HALF_WIDTH: f32 = 0.002;

#[derive(Clone, Debug)]
pub(super) struct RetainingWall {
    pub start: [f32; 4], // X, Y, lower and upper elevation.
    pub end: [f32; 4],
}

pub(super) fn walls(map: &ElevationMap) -> Vec<RetainingWall> {
    let mut walls = Vec::new();
    for cut in &map.spec.cuts {
        for i in 0..cut.points.len() {
            let a = cut.points[i];
            let b = cut.points[(i + 1) % cut.points.len()];
            let delta = [b[0] - a[0], b[1] - a[1]];
            let length = delta[0].hypot(delta[1]);
            let normal = [-delta[1] / length * 0.00001, delta[0] / length * 0.00001];
            let mut breaks: Vec<_> = (0..=(length / 0.1).ceil() as usize)
                .map(|j| (j as f32 * 0.1 / length).min(1.))
                .collect();
            breaks.push(1.);
            for other in &map.spec.cuts {
                for j in 0..other.points.len() {
                    let p = other.points[j];
                    let q = other.points[(j + 1) % other.points.len()];
                    let d = [q[0] - p[0], q[1] - p[1]];
                    let denominator = delta[0] * d[1] - delta[1] * d[0];
                    if denominator.abs() < 1e-10 {
                        continue;
                    }
                    let offset = [p[0] - a[0], p[1] - a[1]];
                    let t = (offset[0] * d[1] - offset[1] * d[0]) / denominator;
                    let u = (offset[0] * delta[1] - offset[1] * delta[0]) / denominator;
                    if (0. ..=1.).contains(&t) && (0. ..=1.).contains(&u) {
                        breaks.push(t);
                    }
                }
            }
            breaks.sort_by(f32::total_cmp);
            breaks.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
            let sample = |t: f32| -> Option<[f32; 4]> {
                let p = [a[0] + delta[0] * t, a[1] + delta[1] * t];
                let h = [
                    map.height_at([p[0] + normal[0], p[1] + normal[1]])?,
                    map.height_at([p[0] - normal[0], p[1] - normal[1]])?,
                ];
                Some([p[0], p[1], h[0].min(h[1]), h[0].max(h[1])])
            };
            for pair in breaks.windows(2) {
                let middle = (pair[0] + pair[1]) * 0.5;
                let Some(mid) = sample(middle) else { continue };
                if mid[3] - mid[2] < 0.0001 {
                    continue;
                }
                // Sample each interval from its own interior to avoid a coincident adjacent cut.
                let point = |t: f32| {
                    let mut p = sample(t + (middle - t) * 0.0001).unwrap_or(mid);
                    p[0] = a[0] + delta[0] * t;
                    p[1] = a[1] + delta[1] * t;
                    p
                };
                walls.push(RetainingWall {
                    start: point(pair[0]),
                    end: point(pair[1]),
                });
            }
        }
    }
    walls
}

impl RetainingWall {
    pub fn contact(
        &self,
        point: [f32; 2],
        radius: f32,
        bottom: f32,
        top: f32,
    ) -> Option<([f32; 2], f32)> {
        let radius = radius + HALF_WIDTH;
        let [a, b] = [self.start, self.end];
        if (0..2).any(|i| point[i] + radius < a[i].min(b[i]) || point[i] - radius > a[i].max(b[i]))
        {
            return None;
        }
        let d = [b[0] - a[0], b[1] - a[1]];
        let length_squared = d[0] * d[0] + d[1] * d[1];
        let t =
            (((point[0] - a[0]) * d[0] + (point[1] - a[1]) * d[1]) / length_squared).clamp(0., 1.);
        let lo = a[2] + (b[2] - a[2]) * t;
        let hi = a[3] + (b[3] - a[3]) * t;
        if bottom >= hi || top <= lo {
            return None;
        }
        let delta = [point[0] - a[0] - d[0] * t, point[1] - a[1] - d[1] * t];
        let distance = delta[0].hypot(delta[1]);
        let depth = radius - distance;
        if depth <= 0. {
            return None;
        }
        let normal = if distance > 1e-8 {
            delta.map(|v| v / distance)
        } else {
            let length = length_squared.sqrt();
            [-d[1] / length, d[0] / length]
        };
        Some((normal, depth))
    }

    pub fn append(&self, mesh: &mut Mesh) {
        let start = mesh.vertices.len();
        let [a, b] = [self.start, self.end];
        let d = [b[0] - a[0], b[1] - a[1]];
        let length = d[0].hypot(d[1]);
        let n = [-d[1] / length * HALF_WIDTH, d[0] / length * HALF_WIDTH];
        let p = |v: [f32; 4], side: f32, top: bool| {
            [
                v[0] + n[0] * side,
                v[1] + n[1] * side,
                v[if top { 3 } else { 2 }],
            ]
        };
        let [a, b, c, d, e, f, g, h] = [
            p(a, -1., false),
            p(b, -1., false),
            p(b, 1., false),
            p(a, 1., false),
            p(a, -1., true),
            p(b, -1., true),
            p(b, 1., true),
            p(a, 1., true),
        ];
        for face in [
            [a, b, f, e],
            [d, h, g, c],
            [e, f, g, h],
            [a, d, c, b],
            [a, e, h, d],
            [b, c, g, f],
        ] {
            mesh.quad(face, [0.34, 0.35, 0.34], SurfaceKind::Prop);
        }
        let a = [self.start[0], self.start[1], self.start[2]];
        let b = [self.end[0], self.end[1], self.end[2]];
        super::surface_weathering::concrete_mesh(&mut mesh.vertices[start..], |p| {
            super::surface_weathering::base(a, b, p)
        });
    }
}

#[cfg(test)]
mod tests;
