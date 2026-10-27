use opencity::{map, network::ComponentTemplateBody, parse, Argument, Statement};

const TEMPLATE: &str = r#"
intersection tee({ profile, min, max }: { profile: identifier, min: number, max: number }) {
    center = map_point(0,0);
    north = map_point(0,max);
    east = map_point(max,0);
    west = map_point(min,0);
    intersection3(center,profile,north,north,east,east,west,west);
}
"#;
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-5, "{a} != {b}");
}

#[test]
fn default_map_reuses_three_layouts_and_rotates_all_three_way_orientations() {
    let program = parse(include_str!("../examples/default_map.road")).unwrap();
    let definitions: Vec<_> = program
        .statements
        .iter()
        .filter_map(|s| match s {
            Statement::Network(d) => d.template.as_ref(),
            _ => None,
        })
        .collect();
    assert_eq!(definitions.len(), 3);
    assert!(definitions
        .iter()
        .all(|t| matches!(t.body, ComponentTemplateBody::Junction(_))));
    let mut rotations = std::collections::BTreeSet::new();
    let mut counts = [0; 3];
    for s in &program.statements {
        let Statement::Call(call) = s else {
            continue;
        };
        assert!(!matches!(
            call.name.as_str(),
            "intersection2" | "intersection3" | "intersection4"
        ));
        match call.name.as_str() {
            "town_corner" => counts[0] += 1,
            "town_three_way" => {
                counts[1] += 1;
                let Argument::Number(rotation) = &call.args[3] else {
                    panic!()
                };
                rotations.insert(rotation.parse::<i32>().unwrap());
            }
            "town_four_way" => counts[2] += 1,
            _ => {}
        }
    }
    assert_eq!(counts, [9, 93, 166]);
    assert_eq!(rotations, [0, 90, 180, 270].into_iter().collect());
    let projected = map::project(&program);
    assert_eq!(projected.unresolved, 0);
    assert_eq!(map::intersections(&program, &projected).len(), 268);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn cardinal_rotation_preserves_named_ports_profile_and_world_geometry() {
    let program=parse(&format!("{TEMPLATE} road street: sidewalk carlane sidewalk; end a=tee(10,20,2,90,{{profile:street,min:-1,max:1}}); road_a=street(a.east,10,22,10,23,a.east);")).unwrap();
    let before = program.clone();
    let projection = map::project(&program);
    assert_eq!(projection.unresolved, 0);
    let junction = map::intersections(&program, &projection).remove(0);
    assert_eq!(junction.point_ref, "a.center");
    assert_eq!(junction.road_name, "street");
    let port = junction
        .ports
        .iter()
        .find(|p| p.point_ref == "a.east")
        .unwrap();
    close(port.position.x, 10.);
    close(port.position.y, 21.);
    assert_eq!(port.direction, map::WorldPoint { x: 0., y: 1. });
    assert_eq!(
        map::intersection_port_direction(&program, "a.east"),
        Some(port.direction)
    );
    close(projection.segments[0].start_z, 2.);
    assert_eq!(program, before);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn nested_block_rotations_keep_junctions_and_road_endpoints_aligned() {
    let program = parse(&format!(
        r#"{TEMPLATE}
        road street: carlane; end
        block group({{ size }}: {{ size: number }}):
            node=tee(2,0,1,90,{{profile:street,min:-1,max:size}});
            lane=street(node.north,0,0,0,1,node.east);
        end
        placed=group(10,20,3,90,{{size:1}});
    "#
    ))
    .unwrap();
    let projection = map::project(&program);
    assert_eq!(projection.unresolved, 0);
    let junction = map::intersections(&program, &projection).remove(0);
    assert_eq!(junction.point_ref, "placed.node.center");
    let port = junction
        .ports
        .iter()
        .find(|p| p.point_ref == "placed.node.north")
        .unwrap();
    close(port.position.x, 10.);
    close(port.position.y, 21.);
    let road = &projection.segments[0];
    assert_eq!(road.start_ref.as_deref(), Some("placed.node.north"));
    close(road.start.x, port.position.x);
    close(road.start.y, port.position.y);
    close(road.start_z, 4.);
}

#[test]
fn invalid_layouts_and_placements_report_errors() {
    for body in [
        "center=map_point(0,0);",
        "river(0,0,1,0);",
        "intersection3(center,street,north,missing,east,missing,west,missing);",
    ] {
        assert!(
            parse(&format!("intersection bad({{}}:{{}}){{{body}}}")).is_err(),
            "{body}"
        );
    }
    for args in [
        "0,0",
        "0,0,{}",
        "0,0,0,45,{profile:street,min:-1,max:1}",
        "0,0,{profile:5,min:-1,max:1}",
        "0,0,{profile:street,min:bad,max:1}",
    ] {
        let program=parse(&format!("{TEMPLATE} road street:carlane;end bad=tee({args}); good=tee(0,0,{{profile:street,min:-1,max:1}});")).unwrap();
        let projection = map::project(&program);
        assert_eq!(projection.unresolved, 1, "{args}");
        assert_eq!(map::intersections(&program, &projection).len(), 1, "{args}");
    }
}

#[test]
fn editor_moves_and_rotates_the_instance_while_retaining_its_shared_definition() {
    let mut program=parse(&format!("{TEMPLATE} road street:carlane;end a=tee(10,20,{{profile:street,min:-1,max:1}}); b=tee(0,0,{{profile:street,min:-1,max:1}});")).unwrap();
    let definition = program.statements[0].clone();
    assert!(map::translate_intersection(
        &mut program,
        "a.center",
        map::WorldPoint { x: 2., y: 3. }
    ));
    assert!(map::rotate_intersection(&mut program, "a.center"));
    let projection = map::project(&program);
    assert_eq!(projection.unresolved, 0);
    let junction = map::intersections(&program, &projection)
        .into_iter()
        .find(|j| j.point_ref == "a.center")
        .unwrap();
    close(junction.position.x, 12.);
    close(junction.position.y, 23.);
    let port = junction
        .ports
        .iter()
        .find(|p| p.point_ref == "a.north")
        .unwrap();
    close(port.position.x, 13.);
    close(port.position.y, 23.);
    assert_eq!(program.statements[0], definition);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    assert!(map::remove_placed_intersection(&mut program, "b.center"));
    assert_eq!(
        map::intersections(&program, &map::project(&program)).len(),
        1
    );
}
