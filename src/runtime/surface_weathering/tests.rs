use super::*;
use crate::{
    parse,
    runtime::{City, Mesh, Region},
};
#[test]
fn pavement_contacts_skip_flush_lanes_and_find_raised_sidewalks() {
    let source = "road street: sidewalk carlane carlane sidewalk; end street(0,0,1,0);";
    let city = City::from_program(&parse(source).unwrap(), None).unwrap();
    let mesh = city.mesh(Region::WholeMap, 0.04).unwrap();
    let mut bands = std::collections::HashSet::new();
    for triangle in mesh.vertices.chunks_exact(3) {
        let v = &triangle[0];
        if v.surface[0] != SurfaceKind::Asphalt as u32 as f32 {
            continue;
        }
        let bits = v.surface[3] as u32;
        assert_eq!(bits & 3, 3);
        let left = (bits >> 14) & 31;
        let right = (bits >> 19) & 31;
        assert!((left > 1 && right == 0) || (right > 1 && left == 0));
        let width = ((bits >> 2) & 4095) as f32 * 0.0001;
        assert!(width > 0.02 && width < 0.05);
        assert!(triangle
            .iter()
            .all(|v| v.surface[2] >= 0. && v.surface[2] <= width + 0.0001));
        bands.insert((left, right));
    }
    assert_eq!(bands.len(), 2);
}
#[test]
fn sloped_concrete_preserves_positions_and_records_height_above_its_foot() {
    let mut mesh = Mesh::default();
    mesh.quad(
        [[0., 0., 0.], [1., 0., 0.1], [1., 0., 0.2], [0., 0., 0.1]],
        [0.4; 3],
        SurfaceKind::Prop,
    );
    let original = mesh.clone();
    concrete_mesh(&mut mesh.vertices, |p| base([0., 0., 0.], [1., 0., 0.1], p));
    for (v, old) in mesh.vertices.iter().zip(original.vertices) {
        assert_eq!(v.position, old.position);
        assert_eq!(v.color, old.color);
        assert_eq!(v.surface[3], -2.);
        assert!((v.surface[2] - (v.position[2] - v.position[0] * 0.1)).abs() < 1e-6);
    }
}
#[test]
fn concrete_soffit_normals_face_downward() {
    let mut mesh = Mesh::default();
    mesh.quad(
        [[0., 0., 0.], [0., 1., 0.], [1., 1., 0.], [1., 0., 0.]],
        [0.4; 3],
        SurfaceKind::Prop,
    );
    concrete_mesh(&mut mesh.vertices, |_| 0.);
    assert!(mesh.vertices.iter().all(|v| v.normal[2] < -0.99));
}
