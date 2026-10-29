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
                assert_eq!(v.surface[3], 2.);
                let along = v.position[0] * direction[0] + v.position[1] * direction[1];
                assert!((v.surface[1] - along).abs() < 1e-5, "{v:?}");
            } else {
                curbs += 1;
                assert_eq!(v.surface[3], 0., "curbs must not carry slab joints");
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
        assert!(a.iter().chain(b).all(|v| v.surface[3] == 2.));
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
            .all(|v| v.surface[3] == 2. && v.surface[1].is_finite()));
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
