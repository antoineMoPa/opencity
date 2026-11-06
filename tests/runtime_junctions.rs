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

#[test]
fn subdivided_sloped_approaches_meet_junction_without_a_height_step_or_wall() {
    let program = parse(
        "road street: sidewalk carlane carlane sidewalk; end
        p=map_point(0,0); n0=map_point(0,0.03); n1=map_point(0,0.06); n=map_point(0,1);
        e=map_point(1,0); s=map_point(0,-1); w=map_point(-1,0);
        a=street(p,0,0.01,0,0.02,n0); b=street(n0,0,0.04,0,0.05,n1);
        north=street(n1,0,0.3,0,0.7,n);
        east=street(p,0.3,0,0.7,0,e); south=street(p,0,-0.3,0,-0.7,s);
        west=street(p,-0.3,0,-0.7,0,w);
        grade(a,0.3,0.33); grade(b,0.33,0.36); grade(north,0.36,0.8);
        grade(east,0.3,0.4); grade(south,0.3,0.4); grade(west,0.3,0.4);
        intersection4(p,street);",
    )
    .unwrap();
    let projection = opencity::map::project(&program);
    assert_eq!(projection.unresolved, 0);
    let junction = opencity::map::intersections(&program, &projection).remove(0);
    let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
    let seams = geometry.junction_seams(&projection, &junction);
    assert_eq!(seams.len(), 4);
    assert!(seams
        .iter()
        .any(|(i, _, _)| projection.segments[*i].name.as_deref() == Some("north")));
    let city = City::from_program(&program, None).unwrap();
    let half = junction.footprint_half;
    for x in [-0.02, 0.02] {
        let inner = city.surface_at([x, half - 0.00001], 2.).unwrap();
        let outer = city.surface_at([x, half + 0.00001], 2.).unwrap();
        assert_eq!(inner.kind, SurfaceKind::Asphalt);
        assert!(
            (inner.height - outer.height).abs() < 0.0001,
            "{inner:?} / {outer:?}"
        );
        assert!(city
            .road_barrier_contact([x, half], 0.01, outer.height + 0.0001, outer.height + 0.015)
            .is_none());
    }
}

#[test]
fn elevated_curved_approach_does_not_get_a_barrier_across_its_mouth() {
    let program = parse(
        "road street: sidewalk carlane carlane sidewalk; end
        p=map_point(0,0); n=map_point(0.2,1); e=map_point(1,0); s=map_point(0,-1);
        north=street(p,0,0.3,0.2,0.7,n); east=street(p,0.3,0,0.7,0,e);
        south=street(p,0,-0.3,0,-0.7,s);
        elevation(north,0.5); elevation(east,0.5); elevation(south,0.5);
        intersection3(p,street);",
    )
    .unwrap();
    let projection = opencity::map::project(&program);
    let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
    let city = City::from_program(&program, None).unwrap();
    let road = &city.roads[0];
    let t = geometry.cross_sections(0)[0].t;
    for lane in [1, 2] {
        let p = opencity::map::element_position(&road.segment, &road.definition, lane, t).unwrap();
        assert!(city
            .road_barrier_contact([p.x, p.y], 0.01, 0.5001, 0.515)
            .is_none());
    }
    // The closed western side must still protect the elevated junction.
    let half = road
        .definition
        .elements
        .iter()
        .map(|e| opencity::map::element_world_width(e.kind))
        .sum::<f32>()
        / 2.;
    assert!(city
        .road_barrier_contact([-half, 0.], 0.01, 0.5001, 0.515)
        .is_some());
}

#[test]
fn nearby_junctions_share_turn_room_across_subdivided_connecting_roads() {
    let program = parse(
        "road street: sidewalk carlane carlane sidewalk; end
        p=map_point(0,0); m=map_point(0,0.15); q=map_point(0,0.3);
        e=map_point(1,0); w=map_point(-1,0); ne=map_point(1,0.3); nw=map_point(-1,0.3);
        a=street(p,0,0.05,0,0.1,m); b=street(m,0,0.2,0,0.25,q);
        east=street(p,0.3,0,0.7,0,e); west=street(p,-0.3,0,-0.7,0,w);
        northeast=street(q,0.3,0.3,0.7,0.3,ne); northwest=street(q,-0.3,0.3,-0.7,0.3,nw);
        intersection3(p,street); intersection3(q,street);",
    )
    .unwrap();
    let projection = opencity::map::project(&program);
    let junctions = opencity::map::intersections(&program, &projection);
    assert_eq!(junctions.len(), 2);
    assert!(junctions.iter().map(|j| j.footprint_half).sum::<f32>() < 0.3);
    let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
    for j in &junctions {
        assert_eq!(geometry.junction_seams(&projection, j).len(), 3);
    }
}
