use opencity::{
    network::{self, Bound, Point},
    parse, Statement,
};
const EXAMPLE: &str = include_str!("../examples/intersections.road");
fn definition(source: &str) -> network::Definition {
    let program = parse(source).unwrap();
    let Statement::Network(definition) = program.statements.into_iter().next().unwrap() else {
        panic!("expected component")
    };
    definition
}
#[test]
fn proposal_examples_resolve_and_round_trip_without_losing_topology() {
    let program = parse(EXAMPLE).unwrap();
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    let projected = network::project(&program);
    assert!(projected.errors.is_empty(), "{:?}", projected.errors);
    assert_eq!(projected.instances.len(), 3);
    let cross = &projected.instances[0];
    assert_eq!(cross.geometry.ports.len(), 8);
    assert_eq!(cross.geometry.movements.len(), 12);
    assert_eq!(cross.geometry.paths.len(), 14);
    assert!(cross
        .geometry
        .movements
        .iter()
        .all(|m| m.movement.from.split('.').next() != m.movement.to.split('.').next()));
    let highway = &projected.instances[1].geometry;
    assert_eq!(
        highway.anchors[0].1,
        Point {
            x: -35.,
            y: 0.,
            z: 0.
        }
    );
    let exit = &highway.movements[1];
    assert_eq!(exit.movement.via.len(), 2);
    assert_eq!(
        exit.points.first().unwrap(),
        &Point {
            x: -100.,
            y: 0.,
            z: 0.
        }
    );
    assert_eq!(
        exit.points.last().unwrap(),
        &Point {
            x: 30.,
            y: -100.,
            z: 8.
        }
    );
    assert_eq!(exit.movement.via[0].start, Bound::Distance(0.));
    let merge = &projected.instances[2].geometry;
    assert_eq!(merge.anchors[0].1.x, 20.);
    assert_eq!(merge.movements[1].points.last().unwrap().x, 80.);
}
#[test]
fn line_cubic_and_both_arc_sweeps_have_actual_geometry_and_elevation() {
    let d=definition("intersection bends { path cw { move (0,0,1); arc to (1,1,3) radius 1 clockwise; } path ccw { move (0,0); arc to (1,1) radius 1 counterclockwise; } path curve { move (0,0); cubic (0,1), (1,1) to (1,0); } }");
    let resolved = network::resolve(&d).unwrap();
    let cw = &resolved.paths[0];
    let ccw = &resolved.paths[1];
    assert!((cw.points[32].x - 0.70710677).abs() < 0.0001);
    assert!((cw.points[32].y - 0.29289323).abs() < 0.0001);
    assert_eq!(cw.points[32].z, 2.);
    assert!((ccw.points[32].x - cw.points[32].y).abs() < 0.0001);
    assert!((ccw.length() - std::f32::consts::FRAC_PI_2).abs() < 0.001);
    let curve = &resolved.paths[2];
    assert_eq!(
        curve.points[32],
        Point {
            x: 0.5,
            y: 0.75,
            z: 0.
        }
    );
    let slice = network::slice_path(&ccw.points, 0.1, 0.8).unwrap();
    assert!(slice.len() > 2);
    assert!(network::slice_path(&ccw.points, 0.8, 0.1).is_none());
}
#[test]
fn crossings_and_unlisted_turns_do_not_create_implicit_movements() {
    let d=definition("intersection crossing { path low { move (-1,0); line (1,0); } path high { move (0,-1,5); line (0,1,5); } }");
    let resolved = network::resolve(&d).unwrap();
    assert!(resolved.movements.is_empty());
    assert_eq!(resolved.paths.len(), 2);
    let q = network::band_quads(&resolved.paths[0].points, 2., network::Side::Left);
    assert_eq!(q[0][0].y, 0.);
    assert_eq!(q[0][2].y, -2.);
}
#[test]
fn bad_references_geometry_and_connectivity_are_rejected_with_locations() {
    for (body,message) in [
        ("path p { from missing; line (1,0); }","unknown port"),
        ("path p { move (0,0); line (1,0); } path p { move (0,0); line (2,0); }","duplicate"),
        ("path p { move (0,0); line (1,0); } anchor a at p distance 2;","outside path"),
        ("path p { from a; line (1,0); } anchor a at p distance 0;","cyclic"),
        ("path p { move (0,0); arc to (5,0) radius 1; }","arc radius"),
        ("path p { move (0,0); line (1,0); } surface s along p width 0;","positive"),
        ("port N.in at (0,0) heading south; path p { from N.in; line (1,0); } allow vehicle N.in -> N.in via p;","out port"),
        ("port A.in at (0,0) heading east; port B.out at (3,0) heading east; path a { from A.in; line (1,0); } path b { move (2,0); line to B.out; } allow vehicle A.in -> B.out via a then b;","disconnected"),
        ("port A.in at (0,0) heading east; port B.out at (2,0) heading east; path p { from A.in; line to B.out; } allow vehicle A.in -> B.out via p[0..3];","invalid range"),
        ("path p { move (0,0); line (0,0); }","nonzero"),
        ("port A.in at (0,0) heading somewhere;","heading"),
    ] {
        let error=parse(&format!("\nintersection bad {{ {body} }}")).unwrap_err();
        assert!(error.message.contains(message),"{body}: {error}");assert_eq!(error.span.line,2);
    }
}
#[test]
fn component_instances_work_inside_typed_blocks_without_aliasing() {
    let program=parse("intersection node { path lane { move (0,0); line (1,2,3); } } block group({ x }: { x: number }): node(x,0,0.5); end a=group(10,20,1,{x:2}); b=group(-10,-20,{x:4}); elevation(a,0.25);").unwrap();
    let projection = network::project(&program);
    assert!(projection.errors.is_empty());
    assert_eq!(projection.instances.len(), 2);
    let a = &projection.instances[0];
    let b = &projection.instances[1];
    assert_ne!(a.id, b.id);
    assert_eq!(
        a.origin,
        Point {
            x: 12.,
            y: 20.,
            z: 1.75
        }
    );
    assert_eq!(
        b.origin,
        Point {
            x: -6.,
            y: -20.,
            z: 0.5
        }
    );
    assert_eq!(
        a.world_point(a.geometry.paths[0].points[1]),
        Point {
            x: 13.,
            y: 18.,
            z: 4.75
        }
    );
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}
#[test]
fn inline_components_and_contextual_keywords_keep_existing_calls_working() {
    let program=parse("intersection (4,5,2) { # comment\npath p { move (0,0); line (1,0); } } interchange = map_point(0,0); tool(interchange);").unwrap();
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    assert_eq!(
        network::project(&program).instances[0].origin,
        Point {
            x: 4.,
            y: 5.,
            z: 2.
        }
    );
}
#[test]
fn svg_contains_surface_geometry_and_only_explicit_vehicle_routes() {
    let program = parse(EXAMPLE).unwrap();
    let projection = network::project(&program);
    let cross = &projection.instances[0];
    let svg = network::to_svg(&cross.geometry, "A < B & \"C\"", 512, true);
    assert!(svg.contains("A &lt; B &amp; &quot;C&quot;"));
    assert!(svg.contains("<polygon"));
    assert_eq!(svg.matches("data-from=").count(), 12);
    assert_eq!(svg.matches("data-port=").count(), 8);
    assert!(svg.contains("width=\"512\""));
    let clean = network::to_svg(&cross.geometry, "cross", 512, false);
    assert!(!clean.contains("data-from="));
    assert!(clean.contains("data-band="));
}

#[test]
fn external_roads_resolve_component_port_positions_and_heights() {
    let program=parse("road street: carlane; end intersection node { port A.in at (0,0,2) heading east; port B.out at (2,0,3) heading east; path p { from A.in; line to B.out; } allow vehicle A.in -> B.out via p; } cross=node(10,20,5); far=map_point(15,20); approach=street(cross.B.out,13,20,14,20,far);").unwrap();
    let map = opencity::map::project(&program);
    assert_eq!(map.unresolved, 0);
    let road = &map.segments[0];
    assert_eq!((road.start.x, road.start.y, road.start_z), (12., 20., 8.));
    assert_eq!(road.start_ref.as_deref(), Some("cross.B.out"));
    assert_eq!(road.end.x, 15.);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}
#[test]
fn port_references_inside_repeated_blocks_are_qualified_per_instance() {
    let program=parse("road street: carlane; end intersection node { port A.in at (0,0,2) heading east; port B.out at (2,0,3) heading east; path p { from A.in; line to B.out; } } block group: local=node(1,2,3); link=street(local.A.in,1.5,2,2.5,2,local.B.out); end a=group(10,20,4); b=group(-10,-20);").unwrap();
    let map = opencity::map::project(&program);
    assert_eq!(map.unresolved, 0);
    assert_eq!(map.segments.len(), 2);
    let a = &map.segments[0];
    assert_eq!(
        (a.start.x, a.start.y, a.start_z, a.end_z),
        (11., 22., 9., 10.)
    );
    assert_eq!(a.start_ref.as_deref(), Some("a.local.A.in"));
    assert_eq!(map.segments[1].start_ref.as_deref(), Some("b.local.A.in"));
    assert!(map
        .points
        .iter()
        .any(|point| Some(point.name.as_str()) == a.start_ref.as_deref()));
}
#[test]
fn component_names_cannot_collide_with_existing_definitions() {
    for source in [
        "road node: carlane; end intersection node {}",
        "block node: end intersection node {}",
        "intersection node {} interchange node {}",
    ] {
        assert!(parse(source)
            .unwrap_err()
            .message
            .contains("duplicate component"));
    }
}

#[test]
fn default_intersection_is_authored_with_four_curved_sidewalks_and_stop_bars() {
    let program = parse(include_str!("../road_library/default_four_way.road")).unwrap();
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    let geometry = network::default_four_way();
    assert_eq!(geometry.movements.len(), 12);
    assert!(
        geometry
            .bands
            .iter()
            .all(|band| band.material != network::Material::Verge),
        "the default junction must not paint decorative grass patches"
    );
    let sidewalks: Vec<_> = geometry
        .bands
        .iter()
        .filter(|band| band.name.starts_with("sidewalk_"))
        .collect();
    assert_eq!(sidewalks.len(), 4);
    for band in sidewalks {
        assert!(geometry.path(&band.path).unwrap().points.len() > 60);
        assert_eq!(band.material, network::Material::Sidewalk);
    }
    let stops: Vec<_> = geometry
        .bands
        .iter()
        .filter(|band| band.kind == network::BandKind::Marking)
        .collect();
    assert_eq!(stops.len(), 4);
    for stop in stops {
        assert_eq!(stop.material, network::Material::White);
        assert_eq!(geometry.path(&stop.path).unwrap().points.len(), 2);
    }
    let svg = network::to_svg_with_options(geometry, "default", 512, false, false);
    assert!(!svg.contains("data-from="));
    assert!(!svg.contains("data-port="));
    for side in ["N", "E", "S", "W"] {
        assert!(svg.contains(&format!("data-band=\"stop_{side}\"")));
    }
}
