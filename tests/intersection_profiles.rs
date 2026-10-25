use opencity::{
    map,
    network::{self, BandKind, Material, Point},
    parse,
    road_geometry::RoadGeometry,
    road_profiles, RoadElementKind as K,
};
const SOURCE: &str = include_str!("../examples/intersection_profiles.road");
fn triangles(quad: [Point; 4]) -> [[Point; 3]; 2] {
    [[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]]
}
fn inside(point: Point, triangle: [Point; 3]) -> bool {
    let mut positive = false;
    let mut negative = false;
    let mut area = 0.;
    for i in 0..3 {
        let a = triangle[i];
        let b = triangle[(i + 1) % 3];
        let cross = (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x);
        positive |= cross > 1e-9;
        negative |= cross < -1e-9;
        area += a.x * b.y - a.y * b.x;
    }
    area.abs() > 1e-12 && !(positive && negative)
}
#[test]
fn every_profile_band_reaches_the_same_material_on_the_junction_side_of_its_port() {
    check_seams(SOURCE);
}

#[test]
fn curved_junctions_leave_the_outside_corners_open_instead_of_painting_grass() {
    let program = parse(SOURCE).unwrap();
    let projection = map::project(&program);
    for junction in map::intersections(&program, &projection)
        .into_iter()
        .filter(|j| j.kind == map::IntersectionKind::FourWay)
    {
        let instance =
            network::default_four_way_instance(&program, &projection, &junction).unwrap();
        assert!(!instance
            .geometry
            .bands
            .iter()
            .any(|band| band.name.starts_with("ground_")));
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                let corner = Point {
                    x: sx * junction.footprint_half * 0.99,
                    y: sy * junction.footprint_half * 0.99,
                    z: 0.0,
                };
                assert!(
                    !instance
                        .geometry
                        .bands
                        .iter()
                        .filter(|band| band.kind == BandKind::Surface)
                        .flat_map(|band| instance.geometry.band_quads(band))
                        .flat_map(triangles)
                        .any(|triangle| inside(corner, triangle)),
                    "{} still fills outside its rounded corner",
                    junction.point_ref
                );
            }
        }
    }
}

fn check_seams(source: &str) {
    let program = parse(source).unwrap();
    let projection = map::project(&program);
    let geometry = RoadGeometry::compile(&program, &projection);
    let profiles = road_profiles::road_profiles(&program, &projection);
    for junction in map::intersections(&program, &projection)
        .into_iter()
        .filter(|junction| junction.kind == map::IntersectionKind::FourWay)
    {
        let instance =
            network::default_four_way_instance(&program, &projection, &junction).unwrap();
        for (index, end) in map::intersection_arms(&projection, &junction.point_ref) {
            let segment = &projection.segments[index];
            let road = road_profiles::road_for_segment(&program, segment).unwrap();
            // Short spline pieces fully inside the footprint have no external seam.
            if geometry.cross_sections(index).is_empty() {
                continue;
            }
            let row = if end {
                geometry.cross_sections(index).last().unwrap()
            } else {
                &geometry.cross_sections(index)[0]
            };
            let inward = Point {
                x: junction.position.x - segment.point_at(row.t).x,
                y: segment.point_at(row.t).y - junction.position.y,
                z: 0.,
            };
            for (band, element) in road.elements.iter().enumerate() {
                // Medians terminate at the stop bar; the interior remains traversable pavement.
                if element.kind == K::Middle {
                    continue;
                }
                let a = row.boundaries[band].point;
                let b = row.boundaries[band + 1].point;
                let point = Point {
                    x: (a.x + b.x) / 2. - junction.position.x + inward.x * 0.00005,
                    y: junction.position.y - (a.y + b.y) / 2. + inward.y * 0.00005,
                    z: 0.,
                };
                let expected = network::road_color(element.kind, false);
                let mut actual = None;
                for surface in instance
                    .geometry
                    .bands
                    .iter()
                    .filter(|band| band.kind == BandKind::Surface)
                {
                    if instance
                        .geometry
                        .band_quads(surface)
                        .into_iter()
                        .flat_map(triangles)
                        .any(|triangle| inside(point, triangle))
                    {
                        actual = Some(surface.material.rgb());
                    }
                }
                assert_eq!(
                    actual,
                    Some(expected),
                    "{} / {} / {:?}, point {:?}, clips {:?}/{:?}",
                    junction.point_ref,
                    segment.name.as_deref().unwrap(),
                    element.kind,
                    point,
                    profiles[index].start_clip,
                    profiles[index].end_clip
                );
            }
        }
    }
}
#[test]
fn local_stays_uncurbed_and_mixed_roads_keep_distinct_widths_and_sidewalks() {
    let program = parse(SOURCE).unwrap();
    let projection = map::project(&program);
    let junctions = map::intersections(&program, &projection);
    let local = junctions
        .iter()
        .find(|j| j.point_ref == "local_center")
        .unwrap();
    let local = network::default_four_way_instance(&program, &projection, local).unwrap();
    assert!(!local
        .geometry
        .bands
        .iter()
        .any(|band| band.material == Material::RoadElement(K::Sidewalk)));
    let mixed = junctions
        .iter()
        .find(|j| j.point_ref == "mixed_center")
        .unwrap();
    let mixed = network::default_four_way_instance(&program, &projection, mixed).unwrap();
    let n = mixed.geometry.ports.iter().find(|p| p.name == "N").unwrap();
    let e = mixed.geometry.ports.iter().find(|p| p.name == "E").unwrap();
    assert!(n.width.unwrap() < e.width.unwrap());
    for movement in &mixed.geometry.movements {
        let m = &movement.movement;
        let from = mixed
            .geometry
            .ports
            .iter()
            .find(|p| format!("{}.{}", p.name, p.role.keyword()) == m.from)
            .unwrap();
        let to = mixed
            .geometry
            .ports
            .iter()
            .find(|p| format!("{}.{}", p.name, p.role.keyword()) == m.to)
            .unwrap();
        assert!(movement.points.first().unwrap().distance(from.position) < 1e-6);
        assert!(movement.points.last().unwrap().distance(to.position) < 1e-6);
    }
}
#[test]
fn stop_bars_span_incoming_vehicle_lanes_instead_of_sidewalk_or_parking() {
    let program = parse(SOURCE).unwrap();
    let projection = map::project(&program);
    for junction in map::intersections(&program, &projection) {
        let instance =
            network::default_four_way_instance(&program, &projection, &junction).unwrap();
        let stops: Vec<_> = instance
            .geometry
            .bands
            .iter()
            .filter(|band| band.kind == BandKind::Marking)
            .collect();
        assert_eq!(stops.len(), 4);
        for stop in stops {
            let quads = instance.geometry.band_quads(stop);
            assert_eq!(quads.len(), 1);
            assert_eq!(stop.material, Material::White);
        }
    }
}
