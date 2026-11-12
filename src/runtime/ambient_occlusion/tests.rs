use super::*;
use crate::runtime::SurfaceKind;
fn floor() -> Mesh {
    let mut mesh = Mesh::default();
    mesh.quad(
        [[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]],
        [0.5; 3],
        SurfaceKind::Prop,
    );
    mesh
}
#[test]
fn exposed_plane_does_not_shadow_itself_and_bake_preserves_geometry() {
    let mut mesh = floor();
    let original = mesh.clone();
    bake(&mut mesh, 0.3, None);
    for (a, b) in mesh.vertices.iter().zip(&original.vertices) {
        assert_eq!(a.position, b.position);
        assert_eq!(a.normal, b.normal);
        assert_eq!(a.surface, b.surface);
        assert_eq!(a.color, b.color);
    }
}
#[test]
fn contact_corner_is_occluded_while_distant_floor_stays_open() {
    let mut mesh = floor();
    mesh.quad(
        [[0., 0., 0.], [0., 1., 0.], [0., 1., 1.], [0., 0., 1.]],
        [0.5; 3],
        SurfaceKind::Prop,
    );
    let mut duplicate = mesh.clone();
    bake(&mut mesh, 0.3, None);
    bake(&mut duplicate, 0.3, None);
    let near = mesh
        .vertices
        .iter()
        .find(|v| v.position == [0., 0., 0.] && v.normal[2] > 0.9)
        .unwrap();
    let far = mesh
        .vertices
        .iter()
        .find(|v| v.position == [1., 0., 0.] && v.normal[2] > 0.9)
        .unwrap();
    assert!(
        near.color[0] < far.color[0] * 0.9,
        "near={:?}, far={:?}",
        near.color,
        far.color
    );
    for (a, b) in mesh.vertices.iter().zip(&duplicate.vertices) {
        assert_eq!(a.color, b.color);
        assert!(a.color.iter().all(|&c| (0.26..=0.5).contains(&c)));
    }
}
#[test]
fn ground_only_shades_low_walls_and_bad_radius_is_a_noop() {
    let mut mesh = Mesh::default();
    mesh.quad(
        [[0., 0., 0.], [0., 1., 0.], [0., 1., 1.], [0., 0., 1.]],
        [0.5; 3],
        SurfaceKind::Prop,
    );
    let original = mesh.clone();
    bake(&mut mesh, f32::NAN, Some(0.));
    assert_eq!(mesh.vertices[0].color, original.vertices[0].color);
    bake(&mut mesh, 0.25, Some(0.));
    assert!(mesh.vertices[0].color[0] < 0.45);
    assert_eq!(mesh.vertices[2].color, [0.5; 3]);
}

#[test]
fn acceleration_tree_finds_the_nearest_box_face_from_every_axis() {
    let mut mesh = Mesh::default();
    for x in 0..12 {
        mesh.cuboid(
            [x as f32 * 3., 0., 0.],
            [x as f32 * 3. + 1., 1., 1.],
            [0.5; 3],
            SurfaceKind::Prop,
        );
    }
    mesh.cuboid([10.; 3], [11.; 3], [0.5; 3], SurfaceKind::Prop);
    let tree = Tree::new(&mesh);
    assert!(tree.nodes.len() > 3);
    for axis in 0..3 {
        for sign in [-1., 1.] {
            let mut origin = [10.5; 3];
            origin[axis] = if sign > 0. { 9. } else { 12. };
            let mut direction = [0.; 3];
            direction[axis] = sign;
            let mut distance = 2.;
            tree.hit(0, origin, direction, 1e-5, &mut distance);
            assert!(
                (distance - 1.).abs() < 1e-6,
                "axis {axis}, sign {sign}: {distance}"
            );
        }
    }
}

#[test]
fn solid_box_bake_matches_triangle_bake_on_exposed_surfaces() {
    let mut mesh = Mesh::default();
    for (min, max) in [
        ([0., 0., 0.], [1., 1., 0.1]),
        ([0.3, 0.3, 0.1], [0.6, 0.6, 0.5]),
        ([0., 0., 0.62], [0.9, 0.9, 0.72]),
    ] {
        let start = mesh.vertices.len();
        mesh.cuboid(min, max, [0.5; 3], SurfaceKind::Prop);
        for v in &mut mesh.vertices[start..] {
            if v.position[2] == min[2] && v.normal[2] > 0.9 {
                v.normal[2] = -1.;
            }
        }
    }
    let mut reference = mesh.clone();
    bake(&mut reference, 0.25, Some(0.));
    bake_cuboids(&mut mesh, 0.25, Some(0.));
    for (a, b) in mesh.vertices.iter().zip(&reference.vertices) {
        // Touching undersides are buried inside the supporting solid. The roof
        // samples remain outside all solids, including the gap below the overhang.
        if a.normal[2] < 0.9 {
            continue;
        }
        assert!(
            (a.color[0] - b.color[0]).abs() < 0.015,
            "{:?}: {:?} / {:?}",
            a.position,
            a.color,
            b.color
        );
    }
}
