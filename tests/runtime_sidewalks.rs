#![cfg(feature = "game-runtime")]
use opencity::{
    parse,
    runtime::{City, Mesh, Region, SurfaceKind},
};
fn mesh(source: &str) -> Mesh {
    City::from_program(&parse(source).unwrap(), None)
        .unwrap()
        .mesh(Region::WholeMap, 0.04)
        .unwrap()
}
#[test]
fn straight_sidewalk_joints_cross_the_whole_width_in_any_direction() {
    for (end, direction) in [
        ("1,0", [1., 0.]),
        ("-1,0", [-1., 0.]),
        ("0,1", [0., 1.]),
        ("0,-1", [0., -1.]),
        ("0.6,0.8", [0.6, 0.8]),
    ] {
        let m = mesh(&format!(
            "road street: sidewalk carlane sidewalk; end street(0,0,{end});"
        ));
        let mut tops = 0;
        let mut curbs = 0;
        for v in &m.vertices {
            if v.surface[0] != SurfaceKind::Sidewalk as u32 as f32 {
                continue;
            }
            if v.normal[2] > 0.9 {
                tops += 1;
                assert_eq!((v.surface[3] as u32) & 3, 2);
                let along = v.position[0] * direction[0] + v.position[1] * direction[1];
                assert!((v.surface[1] - along).abs() < 1e-5, "{v:?}");
            } else {
                curbs += 1;
                assert_eq!(v.surface[3], -2., "curbs must not carry slab joints");
            }
        }
        assert!(tops > 0 && curbs > 0);
    }
}
#[test]
fn curved_sidewalks_keep_equal_distance_across_each_section_and_shared_vertices() {
    let m = mesh("road street: sidewalk carlane sidewalk; end street(0,0,0.2,0.8,0.8,0.2,1,1);");
    let tops: Vec<_> = m
        .vertices
        .chunks_exact(3)
        .filter(|t| t[0].surface[0] == SurfaceKind::Sidewalk as u32 as f32 && t[0].normal[2] > 0.9)
        .collect();
    assert!(tops.len() > 8);
    for pair in tops.chunks_exact(2) {
        let [a, b] = [pair[0], pair[1]];
        assert!(a.iter().chain(b).all(|v| (v.surface[3] as u32) & 3 == 2));
        // Quad order is [start left, end left, end right, start right].
        assert!((a[1].surface[1] - a[2].surface[1]).abs() < 1e-6);
        assert!((b[0].surface[1] - b[2].surface[1]).abs() < 1e-6);
        assert!(a[1].surface[1] > a[0].surface[1]);
    }
    let mut shared = std::collections::HashMap::new();
    for v in tops.into_iter().flatten() {
        let key = v.position.map(f32::to_bits);
        if let Some(previous) = shared.insert(key, v.surface[1]) {
            assert!((v.surface[1] - previous).abs() < 1e-6);
        }
    }
}
#[test]
fn clipped_junction_sidewalks_keep_band_coordinates() {
    let m = mesh(include_str!("../examples/default_intersection.road"));
    let mut corners = 0;
    for t in m.vertices.chunks_exact(3) {
        if t[0].surface[0] != SurfaceKind::Sidewalk as u32 as f32 || t[0].normal[2] < 0.9 {
            continue;
        }
        assert!(t
            .iter()
            .all(|v| (v.surface[3] as u32) & 3 == 2 && v.surface[1].is_finite()));
        if t.iter()
            .all(|v| v.position[0].abs() < 0.12 && v.position[1].abs() < 0.12)
        {
            corners += 1;
        }
    }
    assert!(
        corners > 8,
        "curved junction sidewalks must carry transverse joints"
    );
}
#[test]
fn split_and_reversed_straight_segments_share_world_anchored_joint_phase() {
    let m = mesh(
        "road street: sidewalk carlane sidewalk; end \
         street(0.003,0,0.123,0); street(0.223,0,0.123,0);",
    );
    let mut count = 0;
    for v in &m.vertices {
        if v.surface[0] == SurfaceKind::Sidewalk as u32 as f32 && v.normal[2] > 0.9 {
            count += 1;
            assert!((v.surface[1].abs() - v.position[0]).abs() < 1e-6);
        }
    }
    assert!(count > 12);
}

fn on_triangle(p: [f32; 3], tri: &[opencity::runtime::Vertex]) -> bool {
    let sub = |a: [f32; 3], b: [f32; 3]| std::array::from_fn::<_, 3, _>(|i| a[i] - b[i]);
    let dot = |a: [f32; 3], b: [f32; 3]| (0..3).map(|i| a[i] * b[i]).sum::<f32>();
    let a = tri[0].position;
    let u = sub(tri[1].position, a);
    let v = sub(tri[2].position, a);
    let w = sub(p, a);
    let determinant = dot(u, u) * dot(v, v) - dot(u, v).powi(2);
    if determinant <= 1e-20 {
        return false;
    }
    let s = (dot(w, u) * dot(v, v) - dot(w, v) * dot(u, v)) / determinant;
    let t = (dot(w, v) * dot(u, u) - dot(w, u) * dot(u, v)) / determinant;
    s >= -0.001
        && t >= -0.001
        && s + t <= 1.001
        && (0..3).all(|i| (a[i] + s * u[i] + t * v[i] - p[i]).abs() < 1e-6)
}

#[test]
fn junction_sidewalk_edges_are_closed_to_street_level_without_internal_curbs() {
    let four = include_str!("../examples/default_intersection.road");
    let three = four
        .lines()
        .filter(|line| !line.starts_with("south_approach"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace(
            "intersection4(center, street);",
            "intersection3(center, street);",
        );
    for source in [four.to_owned(), three] {
        for elevated in [false, true] {
            let mut source = source.clone();
            if elevated {
                source.push_str("\ngrade(north_approach,0.3,0.2); grade(east_approach,0.2,0.4); grade(west_approach,0.4,0.2);");
                if source.contains("south_approach =") {
                    source.push_str("grade(south_approach,0.2,0.3);");
                }
            }
            let city = City::from_program(&parse(&source).unwrap(), None).unwrap();
            let m = city.mesh(Region::WholeMap, 0.04).unwrap();
            let half = city.junctions[0].footprint_half;
            let curbs: Vec<_> = m
                .vertices
                .chunks_exact(3)
                .filter(|tri| {
                    tri[0].surface[0] == SurfaceKind::Sidewalk as u32 as f32
                        && tri[0].normal[2].abs() < 0.01
                        && tri
                            .iter()
                            .all(|v| v.position[0].abs().max(v.position[1].abs()) < half + 1e-6)
                })
                .collect();
            assert!(!curbs.is_empty(), "junction lacks vertical sidewalk faces");
            // Every emitted face has concrete coordinates and is on an exposed
            // edge, never an internal tessellation cut or a road mouth.
            for tri in &curbs {
                assert!(tri.iter().all(|v| v.surface[3] == -2.));
                let p: [f32; 3] =
                    std::array::from_fn(|i| tri.iter().map(|v| v.position[i]).sum::<f32>() / 3.);
                let sides = [-1., 1.].map(|sign| {
                    city.surface_at(
                        [
                            p[0] + sign * tri[0].normal[0] * 1e-5,
                            p[1] + sign * tri[0].normal[1] * 1e-5,
                        ],
                        1.,
                    )
                    .is_some_and(|s| s.kind == SurfaceKind::Sidewalk)
                });
                assert_ne!(sides[0], sides[1], "internal curb at {p:?}");
            }
            let mut checked = 0;
            for tri in m.vertices.chunks_exact(3).filter(|t| {
                t[0].surface[0] == SurfaceKind::Sidewalk as u32 as f32 && t[0].normal[2] > 0.9
            }) {
                for [a, b] in [[0, 1], [1, 2], [2, 0]] {
                    let a = tri[a].position;
                    let b = tri[b].position;
                    let p: [f32; 3] = std::array::from_fn(|i| (a[i] + b[i]) * 0.5);
                    if p[0].abs().max(p[1].abs()) >= half - 1e-5 {
                        continue;
                    }
                    let d = [b[0] - a[0], b[1] - a[1]];
                    let length = d[0].hypot(d[1]);
                    if length < 1e-6 {
                        continue;
                    }
                    let sides = [-1., 1.].map(|sign| {
                        city.surface_at(
                            [
                                p[0] - sign * d[1] / length * 1e-5,
                                p[1] + sign * d[0] / length * 1e-5,
                            ],
                            1.,
                        )
                    });
                    if !sides
                        .iter()
                        .any(|s| s.is_some_and(|s| s.kind == SurfaceKind::Sidewalk))
                        || !sides
                            .iter()
                            .any(|s| s.is_some_and(|s| s.kind == SurfaceKind::Asphalt))
                    {
                        continue;
                    }
                    let middle = [p[0], p[1], p[2] - 0.00075];
                    assert!(
                        curbs.iter().any(|t| on_triangle(middle, t)),
                        "open curb at {middle:?}, elevated={elevated}"
                    );
                    checked += 1;
                }
            }
            assert!(checked > 8, "did not exercise curved sidewalk edges");
        }
    }
}
