use opencity::{map, parse, road_geometry::RoadGeometry, RoadRole, Statement};

#[test]
fn road_roles_round_trip_and_are_independent_of_names() {
    let source = "road arbitrary: carlane; role merge; direction forward; end;";
    let program = parse(source).unwrap();
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    let Statement::Road(road) = &program.statements[0] else {
        panic!()
    };
    assert_eq!(road.role, RoadRole::Merge);
    let program = parse("road ramp_mouth: carlane; end;").unwrap();
    let Statement::Road(road) = &program.statements[0] else {
        panic!()
    };
    assert_eq!(road.role, RoadRole::Ordinary);
    assert!(parse("road a: carlane; role unknown; end;").is_err());
    assert!(parse("road a: carlane; role merge; role ramp; end;").is_err());
}

#[test]
fn unrelated_wall_is_reported_instead_of_opened() {
    let program = parse(
        "road pavement: carlane; end;\n        a = pavement(-1,0,1,0); b = \
            pavement(0,-1,0,1); elevation(a,0.3); elevation(b,0.3);",
    )
    .unwrap();
    let projection = map::project(&program);
    let geometry = RoadGeometry::compile(&program, &projection);
    assert!(!geometry.obstructions().is_empty());
}

#[test]
fn separated_overpass_keeps_walls_without_blocking_lower_lane() {
    let program = parse(
        "road pavement: carlane; end;
        a = pavement(-1,0,1,0); b = pavement(0,-1,0,1); elevation(a,0.4);",
    )
    .unwrap();
    let projection = map::project(&program);
    let geometry = RoadGeometry::compile(&program, &projection);
    assert!(!geometry.walls.is_empty());
    assert!(geometry.obstructions().is_empty());
}

#[test]
fn merge_openings_survive_renaming_and_all_quarter_turns() {
    for turn in 0..4 {
        let program = parse(&format!(
            "place(street_to_interstate_elevated, sample, 5, -3, {turn});"
        ))
        .unwrap();
        let renamed = program
            .to_road_source()
            .replace("ramp_mouth", "unrelated_label");
        let program = parse(&renamed).unwrap();
        let projection = map::project(&program);
        let geometry = RoadGeometry::compile(&program, &projection);
        assert!(geometry.unresolved.is_empty());
        assert!(
            geometry.obstructions().is_empty(),
            "turn {turn}: {:?}",
            geometry.obstructions()
        );
        // Losing the explicit merge declaration must expose the obstruction.
        let broken = parse(&renamed.replace("role merge;", "role ordinary;")).unwrap();
        assert!(!RoadGeometry::compile(&broken, &map::project(&broken))
            .obstructions()
            .is_empty());
    }
}

#[test]
fn connector_shoulders_continue_at_full_width_into_the_merge() {
    let program = parse(include_str!("../road_library/cloverleaf.road")).unwrap();
    let projection = map::project(&program);
    let geometry = RoadGeometry::compile(&program, &projection);
    let index = |name: &str| {
        projection
            .segments
            .iter()
            .position(|s| s.name.as_deref() == Some(name))
            .unwrap()
    };
    for prefix in ["nw", "ne", "sw", "se"] {
        let ramp = index(&format!("{prefix}_connector_down"));
        let mouth = index(&format!("{prefix}_connector_merge"));
        let ramp_rows = geometry.cross_sections(ramp);
        let mouth_rows = geometry.cross_sections(mouth);
        let last = ramp_rows.last().unwrap();
        let first = &mouth_rows[0];
        for (a, b) in [
            (
                last.boundaries.first().unwrap(),
                first.boundaries.first().unwrap(),
            ),
            (
                last.boundaries.last().unwrap(),
                first.boundaries.last().unwrap(),
            ),
        ] {
            assert!(
                (a.point.x - b.point.x).hypot(a.point.y - b.point.y) < 0.00001,
                "{prefix} has a detached wall at its merge seam"
            );
            assert!((a.height - b.height).abs() < 0.00001);
        }
        let width = |row: &opencity::road_geometry::CrossSection| {
            let a = row.boundaries.first().unwrap();
            let b = row.boundaries.last().unwrap();
            (a.point.x - b.point.x).hypot(a.point.y - b.point.y)
        };
        assert!(
            (width(&ramp_rows[0]) - width(last)).abs() < 0.00001,
            "ramp shoulders must stay full width through the merge"
        );
        for wall in geometry.walls.iter().filter(|w| w.segment == ramp) {
            for end in [wall.start, wall.end] {
                assert!(
                    ramp_rows.iter().any(|row| [
                        row.boundaries.first().unwrap(),
                        row.boundaries.last().unwrap()
                    ]
                    .iter()
                    .any(
                        |v| (v.point.x - end.point.x).hypot(v.point.y - end.point.y) < 0.00001
                            && (v.height - end.height).abs() < 0.00001
                    )),
                    "wall must sit on an actual pavement edge"
                );
            }
        }
    }
}
