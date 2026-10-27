#![cfg(feature = "game-runtime")]
use opencity::{
    parse,
    runtime::{City, Mesh, Region, SurfaceKind},
};
fn layers(mesh: &Mesh, p: [f32; 2]) -> Vec<(f32, f32)> {
    let mut result = Vec::new();
    for tri in mesh.vertices.chunks_exact(3) {
        if tri[0].surface[0] == SurfaceKind::Marking as u32 as f32 {
            continue;
        }
        let [a, b, c] = [tri[0].position, tri[1].position, tri[2].position];
        let d = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if d.abs() < 1e-14 {
            continue;
        }
        let u = ((p[0] - a[0]) * (c[1] - a[1]) - (p[1] - a[1]) * (c[0] - a[0])) / d;
        let v = ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])) / d;
        if u > 0.00001 && v > 0.00001 && u + v < 0.99999 {
            result.push((
                a[2] + u * (b[2] - a[2]) + v * (c[2] - a[2]),
                tri[0].surface[0],
            ));
        }
    }
    result
}
#[test]
fn junction_and_approach_meshes_have_no_coplanar_double_surfaces() {
    for profile in [
        "sidewalk carlane carlane sidewalk",
        "sidewalk carlane middle carlane sidewalk",
    ] {
        let source = include_str!("../examples/default_intersection.road")
            .replace("sidewalk carlane carlane sidewalk", profile);
        let c = City::from_program(&parse(&source).unwrap(), None).unwrap();
        let mesh = c.mesh(Region::WholeMap, 0.04).unwrap();
        for x in -35..=35 {
            for y in -35..=35 {
                let p = [x as f32 * 0.007 + 0.00137, y as f32 * 0.007 + 0.00219];
                let hits = layers(&mesh, p);
                for i in 0..hits.len() {
                    for j in i + 1..hits.len() {
                        assert!(
                            (hits[i].0 - hits[j].0).abs() > 1e-6,
                            "coplanar pavement at {p:?}: {hits:?}, profile={profile}"
                        );
                    }
                }
            }
        }
        // Approach lane support stays continuous through the actual junction face.
        for y in -60..=60 {
            let p = [0.02837, y as f32 * 0.005 + 0.00119];
            assert!(c.surface_at(p, 0.01).is_some(), "missing lane at {p:?}");
        }
    }
}

#[test]
fn three_way_runtime_and_minimap_use_curved_profile_components() {
    let source = include_str!("../examples/default_intersection.road")
        .lines()
        .filter(|line| !line.starts_with("south_approach"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace(
            "intersection4(center, street);",
            "intersection3(center, street);",
        );
    let city = City::from_program(&parse(&source).unwrap(), None).unwrap();
    assert_eq!(city.junctions.len(), 1);
    let component = city
        .components
        .iter()
        .find(|c| c.id == "profile:center")
        .unwrap();
    assert_eq!(
        component
            .geometry
            .bands
            .iter()
            .filter(|b| b.path.starts_with("curb_"))
            .count(),
        2
    );
    let svg = city.minimap_svg().unwrap();
    assert!(svg.contains("<path fill=\"#a7afa1\""));
    assert!(city.surface_at([0.001, 0.001], 0.01).is_some());
}
