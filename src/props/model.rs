use super::*;
use std::sync::OnceLock;
pub(super) fn parts(kind: Kind) -> &'static [Arc<Mesh>] {
    if SignDesign::for_kind(kind).is_some() {
        static SIGNS: OnceLock<[Vec<Arc<Mesh>>; 7]> = OnceLock::new();
        let models = SIGNS.get_or_init(|| {
            [
                Kind::StopSign,
                Kind::SpeedSign,
                Kind::OneWaySign,
                Kind::YieldSign,
                Kind::CurveSign,
                Kind::HighwaySign,
                Kind::ParkingSign,
            ]
            .map(|kind| SignDesign::for_kind(kind).unwrap().parts())
        });
        return &models[index(kind) - 11];
    }
    static MODELS: OnceLock<[Vec<Arc<Mesh>>; 11]> = OnceLock::new();
    &MODELS.get_or_init(|| {
        [
            vec![cone()],
            tree(Kind::Tree, false),
            tree(Kind::TreeMaple2, false),
            tree(Kind::TreeLinden, false),
            tree(Kind::TreeLinden2, false),
            tree(Kind::TreeLocust, false),
            tree(Kind::TreeLocust2, false),
            street(Kind::Bench),
            street(Kind::Lamp),
            street(Kind::TrashCan),
            vec![bus_stop_sign()],
        ]
        .map(|parts| {
            parts
                .into_iter()
                .map(|mut mesh| {
                    // The shared instance shader carries coverage in surface.y.
                    for vertex in &mut mesh.vertices {
                        if vertex.surface[0] < SurfaceKind::TreeLeafMaple as u32 as f32 {
                            vertex.surface[1] = 1.;
                        }
                    }
                    Arc::new(mesh)
                })
                .collect()
        })
    })[index(kind)]
}
pub(super) fn far_parts(kind: Kind) -> &'static [Arc<Mesh>] {
    if !kind.is_tree() {
        return parts(kind);
    }
    static MODELS: OnceLock<[Vec<Arc<Mesh>>; 6]> = OnceLock::new();
    &MODELS.get_or_init(|| {
        [
            Kind::Tree,
            Kind::TreeMaple2,
            Kind::TreeLinden,
            Kind::TreeLinden2,
            Kind::TreeLocust,
            Kind::TreeLocust2,
        ]
        .map(|kind| tree(kind, true).into_iter().map(Arc::new).collect())
    })[index(kind) - 1]
}
pub(super) fn image_parts(kind: Kind) -> &'static [Arc<Mesh>] {
    if !kind.is_tree() {
        return parts(kind);
    }
    static MODELS: OnceLock<[Vec<Arc<Mesh>>; 6]> = OnceLock::new();
    &MODELS.get_or_init(|| {
        [
            SurfaceKind::TreeImageMaple,
            SurfaceKind::TreeImageMaple2,
            SurfaceKind::TreeImageLinden,
            SurfaceKind::TreeImageLinden2,
            SurfaceKind::TreeImageLocust,
            SurfaceKind::TreeImageLocust2,
        ]
        .map(|surface| {
            let mut mesh = Mesh::default();
            let (half, bottom, top) = match surface {
                SurfaceKind::TreeImageLinden | SurfaceKind::TreeImageLinden2 => {
                    (0.08, -0.025, 0.135)
                }
                SurfaceKind::TreeImageLocust | SurfaceKind::TreeImageLocust2 => (0.07, -0.04, 0.10),
                _ => (0.07, -0.025, 0.115),
            };
            let positions = [
                [-half, 0., bottom],
                [half, 0., bottom],
                [half, 0., top],
                [-half, 0., bottom],
                [half, 0., top],
                [-half, 0., top],
            ];
            // Exported GLB images retain their bottom-origin texture convention.
            let uv = [[0., 0.], [1., 0.], [1., 1.], [0., 0.], [1., 1.], [0., 1.]];
            for (position, uv) in positions.into_iter().zip(uv) {
                mesh.vertices.push(crate::runtime::Vertex {
                    position,
                    normal: [0., -1., 0.],
                    color: [1.; 3],
                    surface: [surface as u32 as f32, uv[0], uv[1], 0.],
                });
            }
            // Expand the culling bounds around the camera-facing plane.
            for y in [-half, half, 0.] {
                mesh.vertices.push(crate::runtime::Vertex {
                    position: [0., y, 0.],
                    normal: [0., -1., 0.],
                    color: [1.; 3],
                    surface: [surface as u32 as f32, 0., 0., 0.],
                });
            }
            vec![Arc::new(mesh)]
        })
    })[index(kind) - 1]
}
fn index(kind: Kind) -> usize {
    match kind {
        Kind::Cone => 0,
        Kind::RandomTree => 1,
        Kind::Tree => 1,
        Kind::TreeMaple2 => 2,
        Kind::TreeLinden => 3,
        Kind::TreeLinden2 => 4,
        Kind::TreeLocust => 5,
        Kind::TreeLocust2 => 6,
        Kind::Bench => 7,
        Kind::Lamp => 8,
        Kind::TrashCan => 9,
        Kind::BusStopSign => 10,
        Kind::StopSign => 11,
        Kind::SpeedSign => 12,
        Kind::OneWaySign => 13,
        Kind::YieldSign => 14,
        Kind::CurveSign => 15,
        Kind::HighwaySign => 16,
        Kind::ParkingSign => 17,
    }
}
fn bus_stop_sign() -> Mesh {
    let mut mesh = Mesh::default();
    let pole = [0.32, 0.38, 0.39];
    let green = [0.14, 0.48, 0.41];
    let white = [0.93, 0.96, 0.94];
    // Prop geometry is authored in map units: 1 unit = 100 metres.
    mesh.cuboid(
        [-0.0004, -0.0004, 0.],
        [0.0004, 0.0004, 0.026],
        pole,
        SurfaceKind::Prop,
    );
    mesh.cuboid(
        [-0.003, -0.0006, 0.019],
        [0.003, 0.0006, 0.026],
        green,
        SurfaceKind::Prop,
    );
    mesh.cuboid(
        [-0.0024, -0.0007, 0.020],
        [0.0024, -0.0006, 0.025],
        white,
        SurfaceKind::Prop,
    );
    mesh.cuboid(
        [-0.0021, -0.0008, 0.022],
        [0.0021, -0.0007, 0.024],
        green,
        SurfaceKind::Prop,
    );
    mesh
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
fn tree(kind: Kind, far: bool) -> Vec<Mesh> {
    let bytes: &[u8] = match kind {
        Kind::Tree => crate::game_asset!(
            "../../assets/props/trees/broad.glb",
            "opencity/assets/props/trees/broad.glb"
        ),
        Kind::TreeMaple2 => crate::game_asset!(
            "../../assets/props/trees/broad.glb",
            "opencity/assets/props/trees/broad.glb"
        ),
        Kind::TreeLinden => crate::game_asset!(
            "../../assets/props/trees/upright.glb",
            "opencity/assets/props/trees/upright.glb"
        ),
        Kind::TreeLinden2 => crate::game_asset!(
            "../../assets/props/trees/upright.glb",
            "opencity/assets/props/trees/upright.glb"
        ),
        Kind::TreeLocust => crate::game_asset!(
            "../../assets/props/trees/young.glb",
            "opencity/assets/props/trees/young.glb"
        ),
        Kind::TreeLocust2 => crate::game_asset!(
            "../../assets/props/trees/young.glb",
            "opencity/assets/props/trees/young.glb"
        ),
        _ => unreachable!(),
    };
    let glb = gltf::Gltf::from_slice(bytes).expect("bundled tree GLB");
    let binary = glb.blob.as_deref().expect("bundled tree buffer");
    let prefix = if far { "far_" } else { "near_" };
    let parts = ["bark", "leaves", "grass", "winter"].map(|part| {
        let name = format!("{prefix}{part}");
        let primitive = glb
            .meshes()
            .find(|mesh| mesh.name() == Some(name.as_str()))
            .expect("bundled tree part")
            .primitives()
            .next()
            .unwrap();
        let reader = primitive.reader(|_| Some(binary));
        let positions: Vec<_> = reader.read_positions().unwrap().collect();
        let normals: Vec<_> = reader.read_normals().unwrap().collect();
        let uvs: Vec<_> = reader.read_tex_coords(0).unwrap().into_f32().collect();
        let colors: Vec<_> = reader.read_colors(0).unwrap().into_rgb_f32().collect();
        let surface = match part {
            "leaves" => SurfaceKind::TreeLeafMaple,
            "grass" => SurfaceKind::TreeLeafLinden,
            "winter" => SurfaceKind::TreeWinter,
            _ => SurfaceKind::TreeBark,
        };
        let mut mesh = Mesh::default();
        for index in reader
            .read_indices()
            .map(|i| i.into_u32().collect::<Vec<_>>())
            .unwrap_or_else(|| (0..positions.len() as u32).collect())
        {
            let i = index as usize;
            mesh.vertices.push(crate::runtime::Vertex {
                position: [
                    positions[i][0] / UNITS,
                    -positions[i][2] / UNITS,
                    positions[i][1] / UNITS,
                ],
                normal: [normals[i][0], -normals[i][2], normals[i][1]],
                color: colors[i],
                surface: [surface as u32 as f32, uvs[i][0], uvs[i][1], 0.],
            });
        }
        mesh
    });
    let mut merged = Mesh::default();
    for part in parts {
        merged.vertices.extend(part.vertices);
    }
    vec![merged]
}

fn street(kind: Kind) -> Vec<Mesh> {
    let bytes: &[u8] = match kind {
        Kind::Bench => crate::game_asset!(
            "../../assets/props/bench.glb",
            "opencity/assets/props/bench.glb"
        ),
        Kind::Lamp => crate::game_asset!(
            "../../assets/props/lamp.glb",
            "opencity/assets/props/lamp.glb"
        ),
        Kind::TrashCan => crate::game_asset!(
            "../../assets/props/bin.glb",
            "opencity/assets/props/bin.glb"
        ),
        _ => unreachable!("only street furniture uses GLB street models"),
    };
    let glb = gltf::Gltf::from_slice(bytes).expect("bundled prop GLB");
    let binary = glb.blob.as_deref().expect("bundled prop buffer");
    let mut mesh = Mesh::default();
    fn axis(v: Vec3) -> Vec3 {
        Vec3::new(v.x, -v.z, v.y)
    }
    fn node(node: gltf::Node<'_>, parent: Mat4, bytes: &[u8], out: &mut Mesh, kind: Kind) {
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
                    if kind == Kind::Bench {
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
            node_walk(child, transform, bytes, out, kind);
        }
    }
    fn node_walk(n: gltf::Node<'_>, m: Mat4, b: &[u8], out: &mut Mesh, kind: Kind) {
        node(n, m, b, out, kind);
    }
    for n in glb
        .default_scene()
        .or_else(|| glb.scenes().next())
        .unwrap()
        .nodes()
    {
        node(n, Mat4::IDENTITY, binary, &mut mesh, kind);
    }
    let mut lower = Mesh::default();
    let mut upper = Mesh::default();
    for triangle in mesh.vertices.chunks_exact(3) {
        let target = if kind == Kind::Lamp && triangle.iter().all(|v| v.position[2] > 0.047) {
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
