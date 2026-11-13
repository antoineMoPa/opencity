use super::*;
fn specimen(curb_height: f32, road_height: f32) -> Mesh {
    let mut mesh = Mesh::default();
    mesh.quad(
        [
            [0., 0., curb_height],
            [0., 0.02, curb_height],
            [0.1, 0.02, curb_height],
            [0.1, 0., curb_height],
        ],
        [0.6; 3],
        SurfaceKind::Sidewalk,
    );
    mesh.quad(
        [
            [-0.1, -0.1, road_height],
            [0.2, -0.1, road_height],
            [0.2, 0., road_height],
            [-0.1, 0., road_height],
        ],
        [0.4; 3],
        SurfaceKind::Asphalt,
    );
    mesh
}
fn area(mesh: &Mesh) -> f32 {
    mesh.vertices
        .chunks_exact(3)
        .map(|t| {
            let [a, b, c] = [t[0].position, t[1].position, t[2].position];
            ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
        })
        .sum()
}
#[test]
fn finite_curb_contact_preserves_pavement_area_height_and_color() {
    let mut mesh = specimen(0.0015, 0.);
    let before = area(&mesh);
    prepare_mesh(&mut mesh);
    assert!((area(&mesh) - before).abs() < 1e-7);
    let contacts: Vec<_> = mesh
        .vertices
        .iter()
        .filter(|v| v.surface[3] < -4.)
        .collect();
    assert!(!contacts.is_empty());
    for v in contacts {
        assert_eq!(v.position[2], 0.);
        assert_eq!(v.color, [0.4; 3]);
        assert!(v.position[0] >= -1e-6 && v.position[0] <= 0.100001);
        assert!(v.position[1] >= -SPREAD - 1e-6 && v.position[1] <= 1e-6);
    }
    // A second preparation is harmless and does not add more subdivisions.
    let vertices = mesh.vertices.len();
    prepare_mesh(&mut mesh);
    assert_eq!(mesh.vertices.len(), vertices);
}
#[test]
fn flush_seams_and_pavement_on_another_deck_keep_original_geometry() {
    for (curb, road) in [(0., 0.), (0.0015, -0.1)] {
        let mut mesh = specimen(curb, road);
        let original = mesh.clone();
        prepare_mesh(&mut mesh);
        assert_eq!(mesh.vertices.len(), original.vertices.len());
        assert!(mesh.vertices.iter().all(|v| v.surface[3] >= 0.));
    }
}
#[test]
fn authored_junction_mesh_has_contacts_without_changing_support_samples() {
    use crate::runtime::{City, Region};
    let program = crate::parse(include_str!(
        "../../../../tests/fixtures/contact_debris.road"
    ))
    .unwrap();
    let city = City::from_program(&program, None).unwrap();
    let probes = [[0.45, 0.], [0., 0.], [0., 0.2]];
    let before = probes.map(|p| city.surface_at(p, 1.).map(|s| (s.height, s.kind)));
    let mesh = city.mesh(Region::WholeMap, 0.04).unwrap();
    assert!(mesh.vertices.iter().any(|v| v.surface[3] < -4.));
    let after = probes.map(|p| city.surface_at(p, 1.).map(|s| (s.height, s.kind)));
    assert_eq!(before, after);
}
