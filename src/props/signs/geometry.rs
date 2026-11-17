use super::*;
const WHITE: [f32; 3] = [0.94, 0.95, 0.90];
const BLACK: [f32; 3] = [0.018, 0.025, 0.023];
const RED: [f32; 3] = [0.62, 0.025, 0.02];
const METAL: [f32; 3] = [0.34, 0.39, 0.38];
fn profile(x: f32, z: f32, w: f32, h: f32, pole: bool, mass: f32) -> SignPartProfile {
    SignPartProfile {
        center: Vec3::new(x, if pole { 0. } else { -0.07 }, z),
        half: Vec3::new(w * 0.5, if pole { w * 0.5 } else { 0.035 }, h * 0.5),
        pole,
        mass,
    }
}
pub(super) fn profiles(design: &SignDesign) -> Vec<SignPartProfile> {
    match design {
        SignDesign::Highway { .. } => vec![
            profile(-1.7, 2.55, 0.12, 5.1, true, 24.),
            profile(1.7, 2.55, 0.12, 5.1, true, 24.),
            profile(0., 4., 4.8, 2.2, false, 42.),
        ],
        SignDesign::Stop { cluster: true } => vec![
            profile(0., 1.8, 0.07, 3.6, true, 12.),
            profile(0., 3.2, 0.75, 0.75, false, 2.2),
            profile(0., 2.5, 0.42, 0.56, false, 1.2),
            profile(0., 1.84, 0.42, 0.56, false, 1.2),
        ],
        SignDesign::Stop { .. } => vec![
            profile(0., 1.4, 0.07, 2.8, true, 9.),
            profile(0., 2.4, 0.75, 0.75, false, 2.2),
        ],
        SignDesign::Speed { .. } => vec![
            profile(0., 1.5, 0.07, 3., true, 10.),
            profile(0., 2.52, 0.6, 0.9, false, 2.4),
        ],
        SignDesign::OneWay { .. } => vec![
            profile(0., 1.4, 0.07, 2.8, true, 9.),
            profile(0., 2.5, 0.9, 0.4, false, 1.7),
        ],
        SignDesign::Yield => vec![
            profile(0., 1.4, 0.07, 2.8, true, 9.),
            profile(0., 2.5, 0.9, 0.78, false, 2.),
        ],
        SignDesign::Curve { .. } => vec![
            profile(0., 1.5, 0.07, 3., true, 10.),
            profile(0., 2.5, 0.9, 0.9, false, 2.1),
        ],
    }
}
/// A regular octagon, flat on top; the corner cut is exact, not hand tuned.
pub(super) fn octagon(width: f32) -> Vec<[f32; 2]> {
    let r = width * 0.5;
    let a = r * (std::f32::consts::SQRT_2 - 1.);
    vec![
        [-a, -r],
        [a, -r],
        [r, -a],
        [r, a],
        [a, r],
        [-a, r],
        [-r, a],
        [-r, -a],
    ]
}
fn rectangle(w: f32, h: f32) -> Vec<[f32; 2]> {
    vec![
        [-w * 0.5, -h * 0.5],
        [w * 0.5, -h * 0.5],
        [w * 0.5, h * 0.5],
        [-w * 0.5, h * 0.5],
    ]
}
fn flat(mesh: &mut Mesh, points: &[[f32; 2]], x: f32, z: f32, y: f32, color: [f32; 3]) {
    let coords: Vec<f64> = points.iter().flatten().map(|v| *v as f64).collect();
    let area: f32 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum();
    for indices in earcutr::earcut(&coords, &[], 2)
        .expect("sign polygon")
        .chunks_exact(3)
    {
        let mut p = [points[indices[0]], points[indices[1]], points[indices[2]]];
        let cross =
            (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]);
        if cross * area < 0. {
            p.swap(1, 2);
        }
        mesh.triangle(
            p.map(|p| [(p[0] + x) / UNITS, y / UNITS, (p[1] + z) / UNITS]),
            color,
            SurfaceKind::Prop,
            [1., 0., 0.],
        );
    }
}
fn panel(points: &[[f32; 2]], p: SignPartProfile, _color: [f32; 3]) -> Mesh {
    let mut mesh = Mesh::default();
    let mut back = points.to_vec();
    back.reverse();
    flat(&mut mesh, &back, p.center.x, p.center.z, 0.008, METAL);
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        mesh.quad(
            [
                [a[0] + p.center.x, -0.008, a[1] + p.center.z],
                [b[0] + p.center.x, -0.008, b[1] + p.center.z],
                [b[0] + p.center.x, 0.008, b[1] + p.center.z],
                [a[0] + p.center.x, 0.008, a[1] + p.center.z],
            ]
            .map(|v| v.map(|n| n / UNITS)),
            METAL,
            SurfaceKind::Prop,
        );
    }
    mesh
}
// Border strips and face fill form a partition rather than stacked polygons.
fn framed(mesh: &mut Mesh, points: &[[f32; 2]], z: f32, scales: [f32; 2], colors: [[f32; 3]; 3]) {
    for (outer, inner, color) in [
        (1., scales[0], colors[0]),
        (scales[0], scales[1], colors[1]),
    ] {
        if outer == inner {
            continue;
        }
        for (a, b) in points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
        {
            flat(
                mesh,
                &[
                    [a[0] * outer, a[1] * outer],
                    [b[0] * outer, b[1] * outer],
                    [b[0] * inner, b[1] * inner],
                    [a[0] * inner, a[1] * inner],
                ],
                0.,
                z,
                -0.008,
                color,
            );
        }
    }
    let inner: Vec<_> = points
        .iter()
        .map(|p| [p[0] * scales[1], p[1] * scales[1]])
        .collect();
    flat(mesh, &inner, 0., z, -0.008, colors[2]);
}
fn arrow(mesh: &mut Mesh, x: f32, z: f32, length: f32, direction: SignArrow, color: [f32; 3]) {
    let points = [
        [-0.5, -0.065],
        [0.16, -0.065],
        [0.16, -0.2],
        [0.5, 0.],
        [0.16, 0.2],
        [0.16, 0.065],
        [-0.5, 0.065],
    ];
    let transformed: Vec<_> = points
        .iter()
        .map(|p| {
            let (a, b) = (p[0] * length, p[1] * length);
            match direction {
                SignArrow::Right => [a, b],
                SignArrow::Left => [-a, b],
                SignArrow::Straight => [-b, a],
                SignArrow::ExitRight => [
                    (a - b) * std::f32::consts::FRAC_1_SQRT_2,
                    (a + b) * std::f32::consts::FRAC_1_SQRT_2,
                ],
            }
        })
        .collect();
    flat(mesh, &transformed, x, z, -0.018, color);
}
fn parking(p: SignPartProfile, permit: bool) -> Mesh {
    let shape = rectangle(p.half.x * 2., p.half.z * 2.);
    let mut mesh = panel(&shape, p, WHITE);
    framed(
        &mut mesh,
        &shape,
        p.center.z,
        [0.96, 0.94],
        [WHITE, BLACK, WHITE],
    );
    let z = p.center.z + 0.14;
    let r = 0.075;
    for i in 0..32 {
        let a = i as f32 * std::f32::consts::TAU / 32.;
        let b = (i + 1) as f32 * std::f32::consts::TAU / 32.;
        flat(
            &mut mesh,
            &[
                [r * a.cos(), r * a.sin()],
                [r * b.cos(), r * b.sin()],
                [(r - 0.012) * b.cos(), (r - 0.012) * b.sin()],
                [(r - 0.012) * a.cos(), (r - 0.012) * a.sin()],
            ],
            0.,
            z,
            -0.018,
            RED,
        );
    }
    text::draw(&mut mesh, "P", [0., z], 0.11, 0.1, -0.020, BLACK);
    flat(
        &mut mesh,
        &[
            [-0.06, -0.071],
            [0.071, 0.06],
            [0.06, 0.071],
            [-0.071, -0.06],
        ],
        0.,
        z,
        -0.022,
        RED,
    );
    text::draw(
        &mut mesh,
        if permit { "9h - 21h" } else { "13h - 14h" },
        [0., p.center.z],
        0.045,
        0.35,
        -0.018,
        BLACK,
    );
    text::draw(
        &mut mesh,
        if permit { "LUN. AU VEN." } else { "VEN." },
        [0., p.center.z - 0.065],
        0.032,
        0.35,
        -0.018,
        BLACK,
    );
    text::draw(
        &mut mesh,
        if permit {
            "EXCEPTÉ 54"
        } else {
            "1 AVR. AU 1 DÉC."
        },
        [0., p.center.z - 0.12],
        0.027,
        0.35,
        -0.018,
        BLACK,
    );
    arrow(
        &mut mesh,
        0.,
        p.center.z - 0.205,
        0.25,
        if permit {
            SignArrow::Left
        } else {
            SignArrow::Right
        },
        BLACK,
    );
    mesh
}
pub(super) fn build(design: &SignDesign) -> Vec<Mesh> {
    let layout = profiles(design);
    let mut parts = Vec::new();
    for (i, p) in layout.into_iter().enumerate() {
        if p.pole {
            let mut mesh = Mesh::default();
            mesh.cuboid(
                ((p.center - p.half) / UNITS).to_array(),
                ((p.center + p.half) / UNITS).to_array(),
                METAL,
                SurfaceKind::Prop,
            );
            parts.push(mesh);
            continue;
        }
        if matches!(design, SignDesign::Stop { cluster: true }) && i >= 2 {
            parts.push(parking(p, i == 3));
            continue;
        }
        let w = p.half.x * 2.;
        let h = p.half.z * 2.;
        let z = p.center.z;
        let (shape, color) = match design {
            SignDesign::Stop { .. } => (octagon(w), RED),
            SignDesign::Yield => (
                vec![[-w * 0.5, h * 0.5], [0., -h * 0.5], [w * 0.5, h * 0.5]],
                RED,
            ),
            SignDesign::Curve { .. } => (
                vec![[0., -h * 0.5], [w * 0.5, 0.], [0., h * 0.5], [-w * 0.5, 0.]],
                [0.95, 0.68, 0.025],
            ),
            SignDesign::OneWay { .. } => (rectangle(w, h), BLACK),
            SignDesign::Highway { .. } => (rectangle(w, h), [0.018, 0.22, 0.09]),
            _ => (rectangle(w, h), WHITE),
        };
        let mut mesh = panel(&shape, p, color);
        match design {
            SignDesign::Stop { .. } => {
                framed(&mut mesh, &shape, z, [0.95, 0.88], [RED, WHITE, RED]);
                text::draw(&mut mesh, "ARRÊT", [0., z], 0.18, 0.62, -0.018, WHITE);
            }
            SignDesign::Speed { limit } => {
                framed(&mut mesh, &shape, z, [0.95, 0.92], [WHITE, BLACK, WHITE]);
                text::draw(
                    &mut mesh,
                    "MAXIMUM",
                    [0., z + 0.27],
                    0.07,
                    0.51,
                    -0.018,
                    BLACK,
                );
                text::draw(
                    &mut mesh,
                    &limit.to_string(),
                    [0., z - 0.08],
                    0.37,
                    0.49,
                    -0.018,
                    BLACK,
                );
            }
            SignDesign::OneWay { arrow: direction } => {
                framed(&mut mesh, &shape, z, [0.95, 0.92], [BLACK, WHITE, BLACK]);
                arrow(&mut mesh, 0., z, 0.73, *direction, WHITE);
            }
            SignDesign::Yield => {
                framed(&mut mesh, &shape, z, [1., 0.7], [RED, RED, WHITE]);
            }
            SignDesign::Curve { arrow: direction } => {
                framed(&mut mesh, &shape, z, [0.92, 0.88], [color, BLACK, color]);
                let d = if *direction == SignArrow::Right {
                    1.
                } else {
                    -1.
                };
                let points = [
                    [-0.065, -0.24],
                    [0.055, -0.24],
                    [0.055, -0.02],
                    [0.13, 0.08],
                    [0.13, -0.015],
                    [0.32, 0.14],
                    [0.13, 0.29],
                    [0.13, 0.19],
                    [-0.065, 0.04],
                ];
                let points: Vec<_> = points.iter().map(|p| [p[0] * d, p[1]]).collect();
                flat(&mut mesh, &points, 0., z, -0.018, BLACK);
            }
            SignDesign::Highway {
                destination,
                secondary,
                exit,
                arrow: direction,
            } => {
                framed(&mut mesh, &shape, z, [0.975, 0.955], [color, WHITE, color]);
                if !exit.is_empty() {
                    text::draw(
                        &mut mesh,
                        &format!("SORTIE {exit}"),
                        [1.05, z + 0.72],
                        0.22,
                        2.25,
                        -0.018,
                        WHITE,
                    );
                }
                text::draw(
                    &mut mesh,
                    destination,
                    [-0.35, z + 0.2],
                    0.42,
                    3.65,
                    -0.018,
                    WHITE,
                );
                text::draw(
                    &mut mesh,
                    secondary,
                    [-0.35, z - 0.48],
                    0.27,
                    3.65,
                    -0.018,
                    WHITE,
                );
                arrow(&mut mesh, 1.9, z - 0.42, 0.58, *direction, WHITE);
            }
        }
        parts.push(mesh);
    }
    for (mesh, profile) in parts.iter_mut().zip(profiles(design)) {
        if !profile.pole {
            for vertex in &mut mesh.vertices {
                vertex.position[1] += profile.center.y / UNITS;
            }
        }
    }
    parts
}
