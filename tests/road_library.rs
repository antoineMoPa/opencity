use opencity::{map, parse, RoadDirection, RoadElementKind, Statement};

fn asset(source: &str) -> (opencity::Program, map::Projection) {
    let program = parse(source).expect("library asset parses");
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    let projection = map::project(&program);
    assert_eq!(projection.unresolved, 0, "all library calls project");
    (program, projection)
}

#[test]
fn four_way_has_four_matching_external_ports() {
    let (program, projection) = asset(include_str!("../road_library/four_way.road"));
    let junctions = map::intersections(&program, &projection);
    assert_eq!(junctions.len(), 1);
    assert_eq!(junctions[0].ports.len(), 4);
    assert_eq!(projection.segments.len(), 4);
    for name in ["port_north", "port_east", "port_south", "port_west"] {
        assert_eq!(
            projection
                .segments
                .iter()
                .filter(|segment| {
                    segment.start_ref.as_deref() == Some(name)
                        || segment.end_ref.as_deref() == Some(name)
                })
                .count(),
            1,
            "{name} is one free plug endpoint"
        );
    }
}

#[test]
fn three_way_has_three_matching_external_ports() {
    let (program, projection) = asset(include_str!("../road_library/three_way.road"));
    let junctions = map::intersections(&program, &projection);
    assert_eq!(junctions.len(), 1);
    assert_eq!(junctions[0].ports.len(), 3);
    assert_eq!(projection.segments.len(), 3);
}

#[test]
fn composed_patterns_share_a_real_junction_reference() {
    let source = include_str!("../road_library/composition.example.road");
    let (_, projection) = asset(source);
    let from_file = opencity::parse_road_file(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("road_library/composition.example.road"),
    )
    .unwrap();
    assert_eq!(from_file, parse(source).unwrap());
    let arms = map::intersection_arms(&projection, "cross_port_east");
    assert_eq!(arms.len(), 2);
    assert!(projection
        .points
        .iter()
        .all(|point| point.name != "tee_port_west"));
}

#[test]
fn qa_gallery_projects_all_five_patterns() {
    let (program, projection) = asset(include_str!("../road_library/qa_gallery.road"));
    assert_eq!(projection.segments.len(), 83);
    assert_eq!(map::intersections(&program, &projection).len(), 2);
    for prefix in [
        "four_way_sample_",
        "three_way_sample_",
        "flat_ramp_sample_",
        "elevated_ramp_sample_",
        "cloverleaf_sample_",
    ] {
        assert!(projection.segments.iter().any(|segment| segment
            .name
            .as_deref()
            .is_some_and(|name| name.starts_with(prefix))));
    }
}

#[test]
fn native_pattern_placement_rotates_junction_and_ports() {
    let (program, projection) = asset("place(three_way, turned, 5, 2, 1);");
    let north = projection
        .points
        .iter()
        .find(|point| point.name == "turned_port_north")
        .unwrap();
    assert!((north.position.x - 3.8).abs() < 0.0001);
    assert!((north.position.y - 2.0).abs() < 0.0001);
    let junction = map::intersections(&program, &projection);
    assert_eq!(junction.len(), 1);
    assert!(junction[0]
        .ports
        .iter()
        .any(|port| { port.point_ref == "turned_tee_n" && port.direction.x < -0.9 }));
}

#[test]
fn native_cloverleaf_placement_keeps_lane_attachments_and_grades() {
    let (_, projection) = asset("place(cloverleaf, interchange, 10, -3, 1);");
    let east_west = projection
        .segments
        .iter()
        .find(|segment| segment.name.as_deref() == Some("interchange_i_ew_04"))
        .unwrap();
    let north_south = projection
        .segments
        .iter()
        .find(|segment| segment.name.as_deref() == Some("interchange_i_ns_03"))
        .unwrap();
    assert_eq!(east_west.end_z, 0.0);
    assert_eq!(north_south.end_z, 0.3);
    assert_eq!(projection.segments.len(), 66);
}

#[test]
fn native_pattern_joins_reject_bad_geometry_and_travel() {
    for (source, message) in [
        (
            "place(four_way, cross, 0, 0, 0); place(three_way, tee, 0, 0, 0); \
                join(cross, port_east, tee, port_west);",
            "do not meet",
        ),
        (
            "place(street_to_interstate_flat, flat, 0, 0, 0); \
                place(street_to_interstate_elevated, raised, 6, 0, 0); join(flat, \
                highway_e, raised, highway_w);",
            "different heights",
        ),
        (
            "place(street_to_interstate_flat, first, 0, 0, 0); \
                place(street_to_interstate_flat, second, 6, 0, 2); join(first, \
                highway_e, second, highway_e);",
            "point traffic the same way",
        ),
    ] {
        let error = parse(source).unwrap_err();
        assert!(error.message.contains(message), "{error}");
    }
}

#[test]
fn street_ramps_enter_forward_interstate_lane_at_matching_grade() {
    for (source, highway_z) in [
        (
            include_str!("../road_library/street_to_interstate_flat.road"),
            0.0,
        ),
        (
            include_str!("../road_library/street_to_interstate_elevated.road"),
            0.3,
        ),
    ] {
        let (_, projection) = asset(source);
        let named = |name: &str| {
            projection
                .segments
                .iter()
                .find(|segment| segment.name.as_deref() == Some(name))
                .unwrap()
        };
        let highway = named("highway");
        let street = named("street");
        let entry_mouth = named("entry_mouth");
        let approach = named("approach");
        let mouth = named("merge_mouth");
        assert_eq!(highway.direction, RoadDirection::Forward);
        assert_eq!(approach.direction, RoadDirection::Forward);
        assert_eq!(mouth.direction, RoadDirection::Forward);
        assert_eq!(entry_mouth.start_ref.as_deref(), Some("entry"));
        assert_eq!(entry_mouth.end_ref, approach.start_ref);
        assert_eq!(approach.end_ref, mouth.start_ref);
        assert_eq!(mouth.end_ref.as_deref(), Some("merge"));
        assert!(approach.start.y < street.start.y);
        assert!(mouth.end.y < highway.start.y);
        assert!((approach.start_z - street.end_z).abs() < 0.0001);
        assert!((approach.end_z - mouth.start_z).abs() < 0.0001);
        assert!((mouth.end_z - highway.end_z).abs() < 0.0001);
        assert!((highway.start_z - highway_z).abs() < 0.0001);
    }
}

#[test]
fn cloverleaf_has_separate_decks_and_eight_directed_connectors() {
    let (program, projection) = asset(include_str!("../road_library/cloverleaf.road"));
    let named = |name: &str| {
        projection
            .segments
            .iter()
            .find(|segment| segment.name.as_deref() == Some(name))
            .unwrap()
    };
    assert_eq!(named("i_ew_04").end_ref.as_deref(), Some("ew_center"));
    assert_eq!(named("i_ns_03").end_ref.as_deref(), Some("ns_center"));
    assert_eq!(named("i_ew_04").end_z, 0.0);
    assert_eq!(named("i_ns_03").end_z, 0.3);
    for path in ["nw_loop", "sw_loop", "ne_loop", "se_loop"] {
        let pieces = [
            format!("{path}_a_mouth"),
            format!("{path}_a"),
            format!("{path}_b"),
            format!("{path}_c"),
            format!("{path}_c_merge"),
        ];
        for pair in pieces.windows(2) {
            let previous = named(&pair[0]);
            let next = named(&pair[1]);
            assert_eq!(previous.end_ref, next.start_ref);
            assert!((previous.end_z - next.start_z).abs() < 0.0001);
            assert!(
                tangent_alignment(previous, next) > 0.98,
                "{path} has a kink"
            );
        }
        assert!(pieces
            .iter()
            .all(|name| named(name).direction == RoadDirection::Forward));
    }
    for path in [
        "nw_connector",
        "sw_connector",
        "ne_connector",
        "se_connector",
    ] {
        let pieces = [
            format!("{path}_mouth"),
            path.into(),
            format!("{path}_down"),
            format!("{path}_merge"),
        ];
        for pair in pieces.windows(2) {
            let previous = named(&pair[0]);
            let next = named(&pair[1]);
            assert_eq!(previous.end_ref, next.start_ref);
            assert!((previous.end_z - next.start_z).abs() < 0.0001);
            assert!(
                tangent_alignment(previous, next) > 0.98,
                "{path} has a kink"
            );
        }
        assert!(pieces
            .iter()
            .all(|name| named(name).direction == RoadDirection::Forward));
    }
    assert_eq!(map::intersections(&program, &projection).len(), 0);

    for (mouth, attachment, is_entry) in [
        ("nw_loop_a_mouth", "nw_exit", true),
        ("nw_loop_c_merge", "nw_merge", false),
        ("sw_loop_a_mouth", "sw_exit", true),
        ("sw_loop_c_merge", "sw_merge", false),
        ("ne_loop_a_mouth", "ne_exit", true),
        ("ne_loop_c_merge", "ne_merge", false),
        ("se_loop_a_mouth", "se_exit", true),
        ("se_loop_c_merge", "se_merge", false),
        ("nw_connector_mouth", "nw_direct_exit", true),
        ("nw_connector_merge", "nw_direct_merge", false),
        ("sw_connector_mouth", "sw_direct_exit", true),
        ("sw_connector_merge", "sw_direct_merge", false),
        ("ne_connector_mouth", "ne_direct_exit", true),
        ("ne_connector_merge", "ne_direct_merge", false),
        ("se_connector_mouth", "se_direct_exit", true),
        ("se_connector_merge", "se_direct_merge", false),
    ] {
        let (parent_name, lane, t) = map::lane_attachment(&program, attachment).unwrap();
        let parent = named(&parent_name);
        let profile = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                opencity::Statement::Road(road) if road.name == parent.road_name => Some(road),
                _ => None,
            })
            .unwrap();
        let lane_direction = profile.lane_direction(lane).unwrap();
        let lane_tangent = parent.tangent_at(t);
        let ramp = named(mouth);
        let ramp_tangent = ramp.tangent_at(if is_entry { 0.0 } else { 1.0 });
        let dot = lane_tangent.x * ramp_tangent.x + lane_tangent.y * ramp_tangent.y;
        let travel_dot = if lane_direction == RoadDirection::Reverse {
            -dot
        } else {
            dot
        };
        assert!(travel_dot > 0.0, "{mouth} points against its parent lane");
        let road_z = parent.height_at(t);
        let ramp_z = if is_entry { ramp.start_z } else { ramp.end_z };
        assert!((road_z - ramp_z).abs() < 0.0001, "{mouth} has a grade step");
    }
}

#[test]
fn cloverleaf_core_keeps_shoulders_at_every_ramp_port() {
    let (program, projection) = asset(include_str!("../road_library/cloverleaf.road"));
    let core: Vec<_> = projection
        .segments
        .iter()
        .filter(|segment| segment.road_name == "interstate_core")
        .collect();
    assert_eq!(core.len(), 25);
    for segment in &core {
        let dx = segment.end.x - segment.start.x;
        let dy = segment.end.y - segment.start.y;
        assert!(
            (dx * dx + dy * dy).sqrt() <= 0.701,
            "{} spans too much of the interchange",
            segment.name.as_deref().unwrap()
        );
    }
    for statement in &program.statements {
        let Statement::Call(call) = statement else {
            continue;
        };
        if call.name != "lane_point" {
            continue;
        }
        let attachment = call.assigns.first().unwrap();
        let (parent_name, lane, _) = map::lane_attachment(&program, attachment).unwrap();
        let parent = projection
            .segments
            .iter()
            .find(|segment| segment.name.as_deref() == Some(&parent_name))
            .unwrap();
        assert_eq!(
            parent.road_name, "interstate_core",
            "{attachment} attaches outside a short core patch"
        );
        let road = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == parent.road_name => Some(road),
                _ => None,
            })
            .unwrap();
        assert_eq!(road.elements[lane].kind, RoadElementKind::CarLane);
        assert_eq!(road.elements.first().unwrap().kind, RoadElementKind::Verge);
        assert_eq!(road.elements.last().unwrap().kind, RoadElementKind::Verge);
    }
}

fn tangent_alignment(a: &map::Segment, b: &map::Segment) -> f32 {
    let left = a.tangent_at(1.0);
    let right = b.tangent_at(0.0);
    let lengths =
        (left.x * left.x + left.y * left.y).sqrt() * (right.x * right.x + right.y * right.y).sqrt();
    (left.x * right.x + left.y * right.y) / lengths
}

#[test]
fn cloverleaf_ramp_centerlines_do_not_cross_at_grade() {
    let (_, projection) = asset(include_str!("../road_library/cloverleaf.road"));
    let ramps: Vec<_> = projection
        .segments
        .iter()
        .filter(|segment| matches!(segment.road_name.as_str(), "exit_ramp" | "ramp_mouth"))
        .collect();
    for (index, a) in ramps.iter().enumerate() {
        for b in ramps.iter().skip(index + 1) {
            if a.start_ref == b.end_ref
                || a.end_ref == b.start_ref
                || a.start_ref == b.start_ref
                || a.end_ref == b.end_ref
            {
                continue;
            }
            for ai in 0..32 {
                let a0 = a.point_at(ai as f32 / 32.0);
                let a1 = a.point_at((ai + 1) as f32 / 32.0);
                for bi in 0..32 {
                    let b0 = b.point_at(bi as f32 / 32.0);
                    let b1 = b.point_at((bi + 1) as f32 / 32.0);
                    let adx = a1.x - a0.x;
                    let ady = a1.y - a0.y;
                    let bdx = b1.x - b0.x;
                    let bdy = b1.y - b0.y;
                    let det = adx * bdy - ady * bdx;
                    if det.abs() < 0.000001 {
                        continue;
                    }
                    let dx = b0.x - a0.x;
                    let dy = b0.y - a0.y;
                    let ta = (dx * bdy - dy * bdx) / det;
                    let tb = (dx * ady - dy * adx) / det;
                    if !(0.0..=1.0).contains(&ta) || !(0.0..=1.0).contains(&tb) {
                        continue;
                    }
                    let at_a = (ai as f32 + ta) / 32.0;
                    let at_b = (bi as f32 + tb) / 32.0;
                    if (a.height_at(at_a) - b.height_at(at_b)).abs() < 0.04 {
                        panic!(
                            "{} crosses {} at grade",
                            a.name.as_deref().unwrap(),
                            b.name.as_deref().unwrap()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn diagonal_surface_junctions_share_the_approach_frame_and_clip_outside_the_pavement() {
    for angle in [23.0_f32, -32.0, 41.0] {
        let mut program = parse(
            "road street: sidewalk carlane middle carlane sidewalk; end
            center=map_point(0,0); n=map_point(0,2); e=map_point(2,0);
            s=map_point(0,-2); w=map_point(-2,0);
            north=street(center,0,0.6,0,1.4,n);
            east=street(center,0.6,0,1.4,0,e);
            south=street(s,0,-1.4,0,-0.6,center);
            west=street(w,-1.4,0,-0.6,0,center);
            intersection4(center,street);",
        )
        .unwrap();
        for statement in &mut program.statements {
            let Statement::Call(call) = statement else {
                continue;
            };
            let pairs: &[usize] = if call.name == "map_point" {
                &[0]
            } else if call.name == "street" {
                &[1, 3]
            } else {
                &[]
            };
            for &i in pairs {
                let read = |arg: &opencity::Argument| match arg {
                    opencity::Argument::Number(n) => n.parse::<f32>().unwrap(),
                    _ => panic!("coordinate expected"),
                };
                let p = opencity::blocks::rotate_vector(
                    [read(&call.args[i]), read(&call.args[i + 1])],
                    angle,
                );
                call.args[i] = opencity::Argument::Number(p[0].to_string());
                call.args[i + 1] = opencity::Argument::Number(p[1].to_string());
            }
        }
        let projection = map::project(&program);
        let junction = map::intersections(&program, &projection).remove(0);
        assert!((map::intersection_rotation(&projection, &junction) - angle).abs() < 0.001);
        let instance =
            opencity::network::default_four_way_instance(&program, &projection, &junction).unwrap();
        assert!((instance.rotation - angle).abs() < 0.001);
        let profiles = opencity::road_profiles::road_profiles(&program, &projection);
        for (road, profile) in projection.segments.iter().zip(profiles) {
            let t = profile.start_clip.or(profile.end_clip).unwrap();
            let p = road.point_at(t);
            let local = map::junction_local_direction(p, angle);
            let half = map::intersection_footprint_half(&junction, 0.1);
            assert!((local.x.abs().max(local.y.abs()) - half).abs() < 0.0001);
        }
    }
}

#[test]
fn three_way_surface_junction_reserves_curved_corner_pavement() {
    let program = parse(
        "road street: sidewalk carlane carlane sidewalk; end
        p=map_point(0,0); n=map_point(0.2,2); e=map_point(2,0.2); s=map_point(0,-2);
        north=street(p,0,0.6,0.2,1.4,n);
        east=street(p,0.6,0,1.4,0.2,e);
        south=street(s,0,-1.4,0,-0.6,p);
        grade(north,0.4,0.4); grade(east,0.4,0.4); grade(south,0.4,0.4);
        intersection3(p,street);",
    )
    .unwrap();
    let projection = map::project(&program);
    let junction = map::intersections(&program, &projection).remove(0);
    assert!(junction.footprint_half > 0.077);
    let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
    assert!(geometry.obstructions().is_empty());
}

#[test]
fn wide_junction_checks_only_pavement_remaining_after_approach_clipping() {
    let program = parse(
        "road wide: sidewalk carlane carlane carlane carlane carlane carlane sidewalk; end
        p=map_point(0,0); n=map_point(0,2); e=map_point(2,0); s=map_point(0,-2); w=map_point(-2,0);
        north=wide(p,0,0.6,0,1.4,n); east=wide(p,0.6,0,1.4,0,e);
        south=wide(s,0,-1.4,0,-0.6,p); west=wide(w,-1.4,0,-0.6,0,p);
        intersection4(p,wide);",
    )
    .unwrap();
    let projection = map::project(&program);
    let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
    assert!(geometry.band_intrusions(&program, &projection).is_empty());
}
