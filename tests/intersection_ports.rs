use opencity::{map, parse, road_geometry::RoadGeometry, road_profiles};
#[test]
fn road_meshes_stop_at_junction_boundaries_instead_of_overlapping_the_interior() {
    let program = parse(include_str!("../examples/default_intersection.road")).unwrap();
    let projection = map::project(&program);
    let geometry = RoadGeometry::compile(&program, &projection);
    let profiles = road_profiles::road_profiles(&program, &projection);
    let half = 0.077;
    for (index, segment) in projection.segments.iter().enumerate() {
        let rows = geometry.cross_sections(index);
        assert!(rows.len() >= 2);
        for row in rows {
            let a = row.boundaries.first().unwrap().point;
            let b = row.boundaries.last().unwrap().point;
            let x = (a.x + b.x) / 2.;
            let y = (a.y + b.y) / 2.;
            assert!(
                x.abs() >= half - 0.00001 || y.abs() >= half - 0.00001,
                "road {index} continues inside intersection at {x},{y}"
            );
        }
        if segment.start_ref.as_deref() == Some("center") {
            assert!(profiles[index].start_clip.unwrap() > 0.);
            assert!(rows[0].t > 0.);
        }
        if segment.end_ref.as_deref() == Some("center") {
            assert!(profiles[index].end_clip.unwrap() < 1.);
            assert!(rows.last().unwrap().t < 1.);
        }
    }
}
#[test]
fn graded_approach_port_height_uses_the_actual_boundary_parameter() {
    let mut program = parse(include_str!("../examples/default_intersection.road")).unwrap();
    assert!(opencity::set_grade(&mut program, "north_approach", 0., 1.));
    let projection = map::project(&program);
    let geometry = RoadGeometry::compile(&program, &projection);
    let index = projection
        .segments
        .iter()
        .position(|s| s.name.as_deref() == Some("north_approach"))
        .unwrap();
    let last = geometry.cross_sections(index).last().unwrap();
    assert!(last.t < 1.);
    assert!(
        (last.boundaries[0].height - projection.segments[index].height_at(last.t)).abs() < 0.000001
    );
}

#[test]
fn short_spline_pieces_inside_a_junction_do_not_leave_following_road_paint_inside_it() {
    let source=include_str!("../examples/default_intersection.road").replace("north_approach = street(north, 0, 0.7, 0, 0.3, center);","near=map_point(0,0.02); north_approach=street(north,0,0.7,0,0.3,near); short=street(near,0,0.01,0,0.005,center);");
    let program = parse(&source).unwrap();
    let projection = map::project(&program);
    let geometry = RoadGeometry::compile(&program, &projection);
    let junction = map::intersections(&program, &projection).remove(0);
    let half = map::intersection_footprint_half(&junction, 0.077);
    let north = projection
        .segments
        .iter()
        .position(|s| s.name.as_deref() == Some("north_approach"))
        .unwrap();
    let short = projection
        .segments
        .iter()
        .position(|s| s.name.as_deref() == Some("short"))
        .unwrap();
    assert!(geometry.cross_sections(short).is_empty());
    let last = geometry.cross_sections(north).last().unwrap();
    assert!(last.t < 1.);
    let point = projection.segments[north].point_at(last.t);
    assert!((point.y - half).abs() < 1e-5);
}
