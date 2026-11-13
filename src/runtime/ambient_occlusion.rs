//! Deterministic, finite-radius hemisphere bake into linear vertex colors.
//! The resulting mesh needs no extra textures, attributes or per-frame rays.
use super::Mesh;
use std::collections::HashMap;

type V = [f32; 3];
fn key(position: V, normal: V) -> [u32; 6] {
    std::array::from_fn(|i| {
        if i < 3 {
            position[i].to_bits()
        } else {
            normal[i - 3].to_bits()
        }
    })
}
fn sub(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: V, b: V) -> f32 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
struct Triangle {
    origin: V,
    a: V,
    b: V,
    tolerance: f32,
}
struct Primitive {
    min: V,
    max: V,
    triangle: Option<Triangle>,
}
struct Node {
    min: V,
    max: V,
    range: std::ops::Range<usize>,
    children: Option<[usize; 2]>,
}
struct Tree {
    primitives: Vec<Primitive>,
    nodes: Vec<Node>,
}
impl Tree {
    fn new(mesh: &Mesh) -> Self {
        let primitives = mesh
            .vertices
            .chunks_exact(3)
            .filter_map(|t| {
                let p: [V; 3] = std::array::from_fn(|i| t[i].position);
                let a = sub(p[1], p[0]);
                let b = sub(p[2], p[0]);
                let area = dot(cross(a, b), cross(a, b)).sqrt();
                (area > 1e-18).then(|| Primitive {
                    min: std::array::from_fn(|i| p[0][i].min(p[1][i]).min(p[2][i])),
                    max: std::array::from_fn(|i| p[0][i].max(p[1][i]).max(p[2][i])),
                    triangle: Some(Triangle {
                        origin: p[0],
                        a,
                        b,
                        tolerance: area * 1e-6,
                    }),
                })
            })
            .collect::<Vec<_>>();
        Self::from_primitives(primitives)
    }
    fn cuboids(mesh: &Mesh) -> Self {
        let shapes = mesh
            .vertices
            .chunks_exact(36)
            .map(|vertices| Primitive {
                min: std::array::from_fn(|i| {
                    vertices
                        .iter()
                        .map(|v| v.position[i])
                        .fold(f32::INFINITY, f32::min)
                }),
                max: std::array::from_fn(|i| {
                    vertices
                        .iter()
                        .map(|v| v.position[i])
                        .fold(f32::NEG_INFINITY, f32::max)
                }),
                triangle: None,
            })
            .collect();
        Self::from_primitives(shapes)
    }
    fn from_primitives(primitives: Vec<Primitive>) -> Self {
        let mut tree = Self {
            primitives,
            nodes: Vec::new(),
        };
        if !tree.primitives.is_empty() {
            tree.branch(0..tree.primitives.len());
        }
        tree
    }
    fn branch(&mut self, range: std::ops::Range<usize>) -> usize {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for t in &self.primitives[range.clone()] {
            for i in 0..3 {
                min[i] = min[i].min(t.min[i]);
                max[i] = max[i].max(t.max[i]);
            }
        }
        let index = self.nodes.len();
        self.nodes.push(Node {
            min,
            max,
            range: range.clone(),
            children: None,
        });
        if range.len() > 8 {
            let axis = (0..3)
                .max_by(|&a, &b| (max[a] - min[a]).total_cmp(&(max[b] - min[b])))
                .unwrap();
            let middle = range.start + range.len() / 2;
            self.primitives[range.clone()].select_nth_unstable_by(range.len() / 2, |a, b| {
                (a.min[axis] + a.max[axis]).total_cmp(&(b.min[axis] + b.max[axis]))
            });
            let left = self.branch(range.start..middle);
            let right = self.branch(middle..range.end);
            self.nodes[index].children = Some([left, right]);
        }
        index
    }
    fn hit(&self, index: usize, origin: V, direction: V, epsilon: f32, nearest: &mut f32) {
        let node = &self.nodes[index];
        if box_hit(node.min, node.max, origin, direction, *nearest).is_none() {
            return;
        }
        if let Some(children) = node.children {
            for child in children {
                self.hit(child, origin, direction, epsilon, nearest);
            }
            return;
        }
        for shape in &self.primitives[node.range.clone()] {
            let Some(t) = &shape.triangle else {
                if let Some(distance) = box_hit(shape.min, shape.max, origin, direction, *nearest) {
                    *nearest = nearest.min(distance);
                }
                continue;
            };
            let h = cross(direction, t.b);
            let determinant = dot(t.a, h);
            if determinant.abs() < t.tolerance {
                continue;
            }
            let s = sub(origin, t.origin);
            let u = dot(s, h) / determinant;
            if !(-1e-5..=1.00001).contains(&u) {
                continue;
            }
            let q = cross(s, t.a);
            let v = dot(direction, q) / determinant;
            if v < -1e-5 || u + v > 1.00001 {
                continue;
            }
            let distance = dot(t.b, q) / determinant;
            if distance > epsilon && distance < *nearest {
                *nearest = distance;
            }
        }
    }
}

fn box_hit(min: V, max: V, origin: V, direction: V, limit: f32) -> Option<f32> {
    let mut near = 0_f32;
    let mut far = limit;
    for i in 0..3 {
        if direction[i].abs() < 1e-9 {
            if origin[i] < min[i] || origin[i] > max[i] {
                return None;
            }
        } else {
            let a = (min[i] - origin[i]) / direction[i];
            let b = (max[i] - origin[i]) / direction[i];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    Some(near)
}

/// Bake local self-occlusion and optionally the ground plane. Call once on an unbaked mesh.
/// `radius` and `ground` use the mesh's own coordinates; colors retain at least 52% energy.
/// Geometry, normals, material metadata and triangle count remain unchanged.
pub fn bake(mesh: &mut Mesh, radius: f32, ground: Option<f32>) {
    bake_samples(mesh, radius, ground, 16);
}
/// Four directions retain broad contact shading while prioritizing authoring latency.
pub fn bake_fast(mesh: &mut Mesh, radius: f32, ground: Option<f32>) {
    bake_samples(mesh, radius, ground, 4);
}
fn bake_samples(mesh: &mut Mesh, radius: f32, ground: Option<f32>, samples: u32) {
    if !radius.is_finite() || radius <= 0. {
        return;
    }
    let tree = Tree::new(mesh);
    colors(mesh, radius, ground, tree, samples);
}

/// Architecture emits consecutive closed cuboids; intersect those solids directly.
pub(super) fn bake_cuboids(mesh: &mut Mesh, radius: f32, ground: Option<f32>) {
    let tree = Tree::cuboids(mesh);
    colors(mesh, radius, ground, tree, 16);
}
pub(super) fn bake_cuboids_fast(mesh: &mut Mesh, radius: f32, ground: Option<f32>) {
    let tree = Tree::cuboids(mesh);
    colors(mesh, radius, ground, tree, 4);
}
fn colors(mesh: &mut Mesh, radius: f32, ground: Option<f32>, tree: Tree, samples: u32) {
    if tree.nodes.is_empty() {
        return;
    }
    let epsilon = radius * 0.0001;
    let mut interiors: HashMap<[u32; 6], (V, usize)> = HashMap::new();
    for triangle in mesh.vertices.chunks_exact(3) {
        let center: V = std::array::from_fn(|i| triangle.iter().map(|v| v.position[i] / 3.).sum());
        for v in triangle {
            let entry = interiors
                .entry(key(v.position, v.normal))
                .or_insert(([0.; 3], 0));
            for i in 0..3 {
                entry.0[i] += center[i] - v.position[i];
            }
            entry.1 += 1;
        }
    }
    let mut factors = HashMap::new();
    for vertex in &mut mesh.vertices {
        let key = key(vertex.position, vertex.normal);
        let factor = *factors.entry(key).or_insert_with(|| {
            let length = dot(vertex.normal, vertex.normal).sqrt();
            if !length.is_finite() || length < 1e-8 {
                return 1.;
            }
            let n = vertex.normal.map(|v| v / length);
            let axis = if n[2].abs() < 0.9 {
                [0., 0., 1.]
            } else {
                [0., 1., 0.]
            };
            let tangent = cross(axis, n);
            let length = dot(tangent, tangent).sqrt();
            let tangent = tangent.map(|v| v / length);
            let bitangent = cross(n, tangent);
            // Sample just inside the incident face, keeping shared corners consistent.
            let (sum, count) = interiors[&key];
            let shift = sum.map(|v| v / count as f32);
            let length = dot(shift, shift).sqrt();
            let weight = (epsilon * 2. / length.max(epsilon)).min(0.1);
            let mut origin =
                std::array::from_fn(|i| vertex.position[i] + n[i] * epsilon + shift[i] * weight);
            if let Some(z) = ground {
                origin[2] = origin[2].max(z + epsilon * 2.);
            }
            let mut occlusion = 0.;
            for sample in 0..samples {
                // Cosine-weighted disk lifting; fixed directions avoid bake noise between loads.
                let disk = ((sample as f32 + 0.5) / samples as f32).sqrt();
                let angle = sample as f32 * 2.3999631;
                let x = disk * angle.cos();
                let y = disk * angle.sin();
                let z = (1. - disk * disk).sqrt();
                let direction =
                    std::array::from_fn(|i| tangent[i] * x + bitangent[i] * y + n[i] * z);
                let mut distance = radius;
                tree.hit(0, origin, direction, epsilon * 0.5, &mut distance);
                if let Some(z) = ground {
                    if direction[2] < -1e-8 {
                        distance = distance.min(((z - origin[2]) / direction[2]).max(0.));
                    }
                }
                occlusion += (1. - distance / radius).powi(2);
            }
            1. - 0.48 * occlusion / samples as f32
        });
        for color in &mut vertex.color {
            *color *= factor;
        }
    }
}

#[cfg(test)]
mod tests;
