use super::*;
use std::sync::OnceLock;
pub(super) fn parts(kind: Kind) -> &'static [Arc<Mesh>] {
    static MODELS: OnceLock<[Vec<Arc<Mesh>>; 4]> = OnceLock::new();
    let index = match kind {
        Kind::Cone => 0,
        Kind::Tree => 1,
        Kind::Bench => 2,
        Kind::Lamp => 3,
    };
    &MODELS.get_or_init(|| {
        [vec![cone()], vec![tree()], street(false), street(true)].map(|parts| {
            parts
                .into_iter()
                .map(|mut mesh| {
                    // The shared instance shader carries coverage in surface.y.
                    for vertex in &mut mesh.vertices {
                        vertex.surface[1] = 1.;
                    }
                    Arc::new(mesh)
                })
                .collect()
        })
    })[index]
}
fn cone() -> Mesh {
    let mut mesh = Mesh::default();
    let orange = [0.96, 0.24, 0.035];
    let white = [0.95, 0.95, 0.91];
    let rubber = [0.035, 0.043, 0.039];
    // Dimensions are metres here, converted once into the road world's units.
    let base = [
        [-0.183, -0.25],
        [0.183, -0.25],
        [0.25, -0.183],
        [0.25, 0.183],
        [0.183, 0.25],
        [-0.183, 0.25],
        [-0.25, 0.183],
        [-0.25, -0.183],
    ];
    for i in 0..8 {
        let a = base[i];
        let b = base[(i + 1) % 8];
        mesh.quad(
            [
                [a[0], a[1], 0.],
                [b[0], b[1], 0.],
                [b[0], b[1], 0.055],
                [a[0], a[1], 0.055],
            ],
            rubber,
            SurfaceKind::Prop,
        );
        mesh.triangle(
            [[0., 0., 0.055], [a[0], a[1], 0.055], [b[0], b[1], 0.055]],
            rubber,
            SurfaceKind::Prop,
            [0.; 3],
        );
        mesh.triangle(
            [[0.; 3], [b[0], b[1], 0.], [a[0], a[1], 0.]],
            rubber,
            SurfaceKind::Prop,
            [0.; 3],
        );
    }
    let profile = [
        (0.054, 0.145),
        (0.073, 0.145),
        (0.085, 0.13),
        (0.14, 0.129),
        (0.40, 0.116),
        (0.55, 0.1085),
        (0.68, 0.102),
        (0.83, 0.0945),
        (0.88, 0.092),
        (0.907, 0.077),
        (0.925, 0.046),
        (0.93, 0.),
    ];
    for rings in profile.windows(2) {
        let (z0, r0) = rings[0];
        let (z1, r1) = rings[1];
        let mid = (z0 + z1) * 0.5;
        let color = if (0.40..=0.55).contains(&mid) || (0.68..=0.83).contains(&mid) {
            white
        } else {
            orange
        };
        for i in 0..24 {
            let p = |step: usize, r: f32, z| {
                let a = step as f32 * std::f32::consts::TAU / 24.;
                [r * a.cos(), r * a.sin(), z]
            };
            mesh.quad(
                [
                    p(i, r0, z0),
                    p(i + 1, r0, z0),
                    p(i + 1, r1, z1),
                    p(i, r1, z1),
                ],
                color,
                SurfaceKind::Prop,
            );
        }
    }
    // Open carry slot: two uprights and a top arch, never a painted black patch.
    for x in [-0.033, 0.033] {
        mesh.cuboid(
            [x - 0.011, -0.0135, 0.919],
            [x + 0.011, 0.0135, 0.978],
            orange,
            SurfaceKind::Prop,
        );
    }
    for i in 0..12 {
        let a = i as f32 * std::f32::consts::PI / 12.;
        let b = (i + 1) as f32 * std::f32::consts::PI / 12.;
        let p = |t: f32, r: f32, y| [r * t.cos(), y, 0.968 + r * t.sin()];
        for y in [-0.0135, 0.0135] {
            let mut face = [
                p(a, 0.032, y),
                p(b, 0.032, y),
                p(b, 0.013, y),
                p(a, 0.013, y),
            ];
            if y > 0. {
                face.reverse();
            }
            mesh.quad(face, orange, SurfaceKind::Prop);
        }
        mesh.quad(
            [
                p(a, 0.032, -0.0135),
                p(b, 0.032, -0.0135),
                p(b, 0.032, 0.0135),
                p(a, 0.032, 0.0135),
            ],
            orange,
            SurfaceKind::Prop,
        );
    }
    for v in &mut mesh.vertices {
        v.position = v.position.map(|x| x / UNITS);
    }
    mesh
}
fn tree() -> Mesh {
    let mut mesh = Mesh::default();
    mesh.cylinder(
        [0.; 3],
        0.0014,
        0.032,
        [0.22, 0.12, 0.055],
        SurfaceKind::Prop,
    );
    for (x, y, z, r, h) in [
        (-0.005, 0., 0.023, 0.010, 0.018),
        (0.005, 0.003, 0.026, 0.011, 0.019),
        (0., -0.003, 0.032, 0.01, 0.018),
    ] {
        mesh.cylinder([x, y, z], r, h, [0.25, 0.39, 0.13], SurfaceKind::Grass);
    }
    mesh
}

fn street(lamp: bool) -> Vec<Mesh> {
    let bytes: &[u8] = if lamp {
        include_bytes!("../../assets/props/lamp.glb")
    } else {
        include_bytes!("../../assets/props/bench.glb")
    };
    let glb = gltf::Gltf::from_slice(bytes).expect("bundled prop GLB");
    let binary = glb.blob.as_deref().expect("bundled prop buffer");
    let mut mesh = Mesh::default();
    fn axis(v: Vec3) -> Vec3 {
        Vec3::new(v.x, -v.z, v.y)
    }
    fn node(node: gltf::Node<'_>, parent: Mat4, bytes: &[u8], out: &mut Mesh, lamp: bool) {
        let transform = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        let normals = transform.inverse().transpose();
        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                let reader = primitive.reader(|_| Some(bytes));
                let positions: Vec<_> = reader
                    .read_positions()
                    .expect("bundled prop positions")
                    .collect();
                let normal: Vec<_> = reader
                    .read_normals()
                    .expect("bundled prop normals")
                    .collect();
                let colors: Vec<_> = reader
                    .read_colors(0)
                    .expect("bundled prop colors")
                    .into_rgb_f32()
                    .collect();
                let factor = primitive
                    .material()
                    .pbr_metallic_roughness()
                    .base_color_factor();
                let light = primitive
                    .material()
                    .emissive_factor()
                    .iter()
                    .any(|v| *v > 0.);
                for index in reader
                    .read_indices()
                    .expect("bundled prop indices")
                    .into_u32()
                {
                    let i = index as usize;
                    let mut p = axis(transform.transform_point3(Vec3::from_array(positions[i])));
                    let mut n = axis(normals.transform_vector3(Vec3::from_array(normal[i])));
                    if !lamp {
                        p = Vec3::new(-p.y, p.x, p.z);
                        n = Vec3::new(-n.y, n.x, n.z);
                    }
                    n /= n.length().max(1e-9);
                    out.vertices.push(crate::runtime::Vertex {
                        position: (p / UNITS).to_array(),
                        normal: n.to_array(),
                        color: std::array::from_fn(|c| colors[i][c] * factor[c]),
                        surface: [
                            if light {
                                SurfaceKind::Light
                            } else {
                                SurfaceKind::Prop
                            } as u32 as f32,
                            1.,
                            0.,
                            0.,
                        ],
                    });
                }
            }
        }
        for child in node.children() {
            node_walk(child, transform, bytes, out, lamp);
        }
    }
    fn node_walk(n: gltf::Node<'_>, m: Mat4, b: &[u8], out: &mut Mesh, lamp: bool) {
        node(n, m, b, out, lamp);
    }
    for n in glb
        .default_scene()
        .or_else(|| glb.scenes().next())
        .unwrap()
        .nodes()
    {
        node(n, Mat4::IDENTITY, binary, &mut mesh, lamp);
    }
    let mut lower = Mesh::default();
    let mut upper = Mesh::default();
    for triangle in mesh.vertices.chunks_exact(3) {
        let target = if lamp && triangle.iter().all(|v| v.position[2] > 0.047) {
            &mut upper
        } else {
            &mut lower
        };
        target.vertices.extend_from_slice(triangle);
    }
    [lower, upper]
        .into_iter()
        .filter(|m| !m.vertices.is_empty())
        .collect()
}
