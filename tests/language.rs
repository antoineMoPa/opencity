use opencity::{
    parse, parse_road_file, project_landscape_features, tokenize, Argument, LandscapeKind,
    RoadDirection, RoadElementKind, Statement, TokenKind,
};

const EXAMPLE: &str = include_str!("../examples/city.road");

#[test]
fn parses_readme_language_and_round_trips() {
    let program = parse(EXAMPLE).unwrap();
    assert_eq!(program.statements.len(), 3);
    let Statement::Road(road) = &program.statements[0] else {
        panic!("expected road")
    };
    assert_eq!(road.name, "normal_road");
    assert_eq!(road.elements.len(), 10);
    assert_eq!(road.elements[0].kind, RoadElementKind::BuildingEdge);
    assert_eq!(road.elements[0].catch, Some(1));
    assert_eq!(road.elements[9].catch, Some(2));

    let Statement::Block(block) = &program.statements[1] else {
        panic!("expected block")
    };
    assert_eq!(block.calls.len(), 3);
    assert_eq!(block.calls[0].assigns, ["a", "b"]);
    assert_eq!(block.calls[0].args[0], Argument::Number("0.0".into()));
    assert_eq!(block.calls[2].args[0], Argument::Identifier("b".into()));

    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn reads_and_writes_nested_road_file() {
    let root = std::env::temp_dir().join(format!(
        "opencity-test-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let path = root.join("definitions").join("city.road");
    let program = parse(EXAMPLE).unwrap();
    program.write_road_file(&path).unwrap();
    assert_eq!(parse_road_file(&path).unwrap(), program);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn keywords_match_complete_identifiers() {
    let tokens = tokenize("roadway block1 endcap lane_2").unwrap();
    assert_eq!(
        tokens.iter().map(|token| token.kind).collect::<Vec<_>>(),
        [
            TokenKind::Identifier,
            TokenKind::Identifier,
            TokenKind::Identifier,
            TokenKind::Identifier,
            TokenKind::Eof
        ]
    );
}

#[test]
fn rejects_bad_syntax_with_location() {
    let error = parse("road lane:\n  sidewalk-nope; end;").unwrap_err();
    assert_eq!((error.span.line, error.span.column), (2, 12));
    assert!(error.message.contains("unknown road element"));
    assert!(parse("road lane: sidewalk; ").is_err());
    assert!(parse("a, = lane(0);").is_err());
    assert!(parse("lane(0,);").is_err());
    assert!(parse("lane($x);").is_err());
}

#[test]
fn parses_negative_map_coordinates_without_confusing_road_separators() {
    let source = "road lane: verge-carlane-verge; end; lane(-1.25, 2, 3, -4.5);";
    let program = parse(source).unwrap();
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn road_direction_round_trips_and_exposes_lane_travel() {
    let source = "road boulevard: sidewalk-carlane-middle-carlane-sidewalk; direction both; end; \
                  road ramp: verge-carlane-verge; direction forward; end; \
                  road return: carlane; direction reverse; end;";
    let program = parse(source).unwrap();
    let roads: Vec<_> = program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Road(road) => Some(road),
            _ => None,
        })
        .collect();
    assert_eq!(roads[0].lane_direction(0), None);
    assert_eq!(roads[0].lane_direction(1), Some(RoadDirection::Reverse));
    assert_eq!(roads[0].lane_direction(3), Some(RoadDirection::Forward));
    assert_eq!(roads[1].lane_direction(1), Some(RoadDirection::Forward));
    assert_eq!(roads[2].lane_direction(0), Some(RoadDirection::Reverse));
    assert!(RoadDirection::Both.permits(false));
    assert!(!RoadDirection::Forward.permits(false));
    assert!(RoadDirection::Reverse.permits(false));
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    let error = parse("road bad: carlane; direction sideways; end;").unwrap_err();
    assert!(error.message.contains("unknown road direction"));
}

#[test]
fn target_map_ramp_paths_follow_their_entry_and_merge() {
    let program = parse(include_str!("../examples/target_map.road")).unwrap();
    let projection = opencity::map::project(&program);
    assert_eq!(projection.unresolved, 0);
    let segment = |name: &str| {
        projection
            .segments
            .iter()
            .find(|segment| segment.name.as_deref() == Some(name))
            .unwrap()
    };
    for (path, entry, merge) in [
        ("nw_loop", "nw_loop_entry", "nw_loop_merge"),
        ("ne_loop", "ne_loop_entry", "ne_loop_merge"),
        ("se_loop", "se_loop_entry", "se_loop_merge"),
        ("sw_loop", "sw_loop_entry", "sw_loop_merge"),
        ("nw_direct", "nw_direct_from_95", "nw_direct_to_10"),
        ("ne_direct", "ne_direct_from_10", "ne_direct_to_95"),
        ("sw_direct", "sw_direct_from_10", "sw_direct_to_95"),
        ("se_direct", "se_direct_from_95", "se_direct_to_10"),
    ] {
        let pieces: Vec<_> = projection
            .segments
            .iter()
            .filter(|segment| {
                segment.road_name == "ramp"
                    && segment
                        .name
                        .as_deref()
                        .is_some_and(|name| name.starts_with(&format!("{path}_")))
            })
            .collect();
        let first = pieces.first().unwrap();
        let last = pieces.last().unwrap();
        let entry = segment(entry);
        let merge = segment(merge);
        let close = |a: opencity::map::WorldPoint, b: opencity::map::WorldPoint| {
            (a.x - b.x).abs() < 0.001 && (a.y - b.y).abs() < 0.001
        };
        assert!(close(entry.end, first.start), "{path} entry is reversed");
        assert!(close(last.end, merge.start), "{path} merge is reversed");
        assert!(pieces
            .iter()
            .all(|piece| piece.direction == RoadDirection::Forward));
        assert!(entry.permits_travel(true) && !entry.permits_travel(false));
    }
}

#[test]
fn every_target_ramp_mouth_joins_a_lane_in_its_travel_direction() {
    let program = parse(include_str!("../examples/target_map.road")).unwrap();
    let projection = opencity::map::project(&program);
    let attachments = [
        (
            "nw_direct_from_95",
            "nw_direct",
            true,
            RoadDirection::Forward,
        ),
        (
            "nw_direct_to_10",
            "nw_direct",
            false,
            RoadDirection::Reverse,
        ),
        (
            "ne_direct_from_10",
            "ne_direct",
            true,
            RoadDirection::Reverse,
        ),
        (
            "ne_direct_to_95",
            "ne_direct",
            false,
            RoadDirection::Reverse,
        ),
        (
            "sw_direct_from_10",
            "sw_direct",
            true,
            RoadDirection::Forward,
        ),
        (
            "sw_direct_to_95",
            "sw_direct",
            false,
            RoadDirection::Forward,
        ),
        (
            "se_direct_from_95",
            "se_direct",
            true,
            RoadDirection::Reverse,
        ),
        (
            "se_direct_to_10",
            "se_direct",
            false,
            RoadDirection::Forward,
        ),
        ("nw_loop_entry", "nw_loop", true, RoadDirection::Reverse),
        ("nw_loop_merge", "nw_loop", false, RoadDirection::Forward),
        ("ne_loop_entry", "ne_loop", true, RoadDirection::Reverse),
        ("ne_loop_merge", "ne_loop", false, RoadDirection::Reverse),
        ("se_loop_entry", "se_loop", true, RoadDirection::Forward),
        ("se_loop_merge", "se_loop", false, RoadDirection::Reverse),
        ("sw_loop_entry", "sw_loop", true, RoadDirection::Forward),
        ("sw_loop_merge", "sw_loop", false, RoadDirection::Forward),
        (
            "west_depart_10",
            "west_ramp_s",
            true,
            RoadDirection::Forward,
        ),
        (
            "west_merge_10",
            "west_ramp_e",
            false,
            RoadDirection::Forward,
        ),
        (
            "west_grid_merge_10",
            "west_grid_to_freeway",
            false,
            RoadDirection::Forward,
        ),
        (
            "east_depart_10_n",
            "east_ramp_n",
            true,
            RoadDirection::Forward,
        ),
        (
            "east_depart_10_s",
            "east_ramp_s",
            true,
            RoadDirection::Forward,
        ),
        (
            "north_depart_95_w",
            "i95_north_west",
            true,
            RoadDirection::Reverse,
        ),
        (
            "north_depart_95_e",
            "i95_north_east",
            true,
            RoadDirection::Forward,
        ),
        (
            "south_depart_95_w",
            "i95_south_west",
            true,
            RoadDirection::Forward,
        ),
        (
            "south_depart_95_e",
            "i95_south_east",
            true,
            RoadDirection::Forward,
        ),
    ];
    assert_eq!(
        projection
            .segments
            .iter()
            .filter(|segment| segment.road_name == "ramp_mouth")
            .count(),
        attachments.len()
    );
    for (mouth_name, path, is_entry, expected_direction) in attachments {
        let mouth = projection
            .segments
            .iter()
            .find(|segment| segment.name.as_deref() == Some(mouth_name))
            .unwrap();
        assert_eq!(mouth.direction, RoadDirection::Forward);
        let path_pieces: Vec<_> = projection
            .segments
            .iter()
            .filter(|segment| {
                segment.road_name == "ramp"
                    && segment
                        .name
                        .as_deref()
                        .is_some_and(|name| name.starts_with(&format!("{path}_")))
            })
            .collect();
        let path_end = if is_entry {
            path_pieces.first().unwrap().start_ref.as_deref()
        } else {
            path_pieces.last().unwrap().end_ref.as_deref()
        };
        let mouth_end = if is_entry {
            mouth.end_ref.as_deref()
        } else {
            mouth.start_ref.as_deref()
        };
        assert_eq!(mouth_end, path_end, "{mouth_name} points against {path}");
        let lane_ref = mouth
            .start_ref
            .as_deref()
            .filter(|reference| reference.ends_with("_lane"))
            .or_else(|| {
                mouth
                    .end_ref
                    .as_deref()
                    .filter(|reference| reference.ends_with("_lane"))
            })
            .unwrap();
        let lane = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Call(call)
                    if call.name == "lane_point"
                        && call.assigns.first().is_some_and(|name| name == lane_ref) =>
                {
                    Some(call)
                }
                _ => None,
            })
            .unwrap();
        let [Argument::Identifier(parent_name), Argument::Number(index), Argument::Number(fraction)] =
            lane.args.as_slice()
        else {
            panic!("{lane_ref} has an unexpected lane_point form");
        };
        let parent = projection
            .segments
            .iter()
            .find(|segment| segment.name.as_deref() == Some(parent_name))
            .unwrap();
        let road = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == parent.road_name => Some(road),
                _ => None,
            })
            .unwrap();
        let lane_index: usize = index.parse().unwrap();
        let fraction: f32 = fraction.parse().unwrap();
        let direction = road.lane_direction(lane_index).unwrap();
        assert_eq!(
            direction, expected_direction,
            "{mouth_name} uses the wrong carriageway"
        );
        if mouth_name.contains("_loop_") {
            let tangent = parent.tangent_at(fraction);
            let mouth_tangent = mouth.tangent_at(if is_entry { 0.0 } else { 1.0 });
            let longitudinal = tangent.x * mouth_tangent.x + tangent.y * mouth_tangent.y;
            let along_travel = if direction == RoadDirection::Forward {
                longitudinal
            } else {
                -longitudinal
            };
            assert!(
                along_travel > 0.0,
                "{mouth_name} reverses at its freeway lane"
            );
        }
    }
}

#[test]
fn all_target_one_way_ramp_paths_are_continuous_and_connected() {
    let program = parse(include_str!("../examples/target_map.road")).unwrap();
    let projection = opencity::map::project(&program);
    let paths = [
        "nw_loop",
        "ne_loop",
        "se_loop",
        "sw_loop",
        "nw_direct",
        "ne_direct",
        "sw_direct",
        "se_direct",
        "west_ramp_n",
        "west_ramp_s",
        "west_ramp_e",
        "east_ramp_n",
        "east_ramp_s",
        "west_ramp_to_grid",
        "west_grid_to_freeway",
        "east_ramp_loop",
        "i95_north_west",
        "i95_north_east",
        "i95_north_return",
        "i95_south_west",
        "i95_south_east",
        "i95_south_turn",
        "south_95_access",
    ];
    let mut checked = 0;
    for path in paths {
        let pieces: Vec<_> = projection
            .segments
            .iter()
            .filter(|segment| {
                segment.road_name == "ramp"
                    && segment
                        .name
                        .as_deref()
                        .is_some_and(|name| name.starts_with(&format!("{path}_")))
            })
            .collect();
        assert!(!pieces.is_empty(), "{path} is missing");
        checked += pieces.len();
        for piece in &pieces {
            assert_eq!(
                piece.direction,
                RoadDirection::Forward,
                "{path} is not one-way"
            );
        }
        for pair in pieces.windows(2) {
            assert_eq!(
                pair[0].end_ref, pair[1].start_ref,
                "{path} reverses between pieces"
            );
        }
        for endpoint in [
            pieces.first().unwrap().start_ref.as_ref(),
            pieces.last().unwrap().end_ref.as_ref(),
        ] {
            let endpoint = endpoint.unwrap();
            let connections = projection
                .segments
                .iter()
                .filter(|segment| {
                    segment.start_ref.as_ref() == Some(endpoint)
                        || segment.end_ref.as_ref() == Some(endpoint)
                })
                .count();
            assert!(
                connections >= 2,
                "{path} has an unconnected endpoint at {endpoint}"
            );
        }
    }
    assert_eq!(
        checked,
        projection
            .segments
            .iter()
            .filter(|segment| segment.road_name == "ramp")
            .count()
    );
}

#[test]
fn southern_island_loop_stays_outside_neighborhood_blocks() {
    let program = parse(include_str!("../examples/target_map.road")).unwrap();
    let projection = opencity::map::project(&program);
    let segment = |name_prefix: &str| {
        projection
            .segments
            .iter()
            .find(|segment| {
                segment
                    .name
                    .as_deref()
                    .is_some_and(|name| name.starts_with(name_prefix))
            })
            .unwrap()
    };
    let loop_pieces: Vec<_> = projection
        .segments
        .iter()
        .filter(|segment| {
            segment
                .name
                .as_deref()
                .is_some_and(|name| name.starts_with("island_loop_"))
        })
        .collect();
    assert_eq!(loop_pieces.len(), 8);
    assert_eq!(
        loop_pieces.first().unwrap().start_ref,
        segment("island_v0_0_").start_ref
    );
    assert_eq!(
        loop_pieces.last().unwrap().end_ref,
        segment("island_v3_0_").start_ref
    );
    let top_grid_row = (512.0 - 874.0) / 60.0;
    for piece in loop_pieces {
        for sample in 1..10 {
            let point = piece.point_at(sample as f32 / 10.0);
            assert!(
                point.y > top_grid_row + 0.001,
                "island loop crosses the neighborhood grid"
            );
        }
    }
}

#[test]
fn projects_land_and_water_with_block_translation_and_elevation() {
    let source = "\
        shore = land(0, 0, 4, 0, 4, 3);\n\
        elevation(shore, 2);\n\
        block watershed:\n\
          pool = lake(0, 0, 2, 0, 1, 2);\n\
          elevation(pool, -1);\n\
          river(1, 2, 4, 3);\n\
        end;\n\
        basin = watershed(10, -5, -2);\n\
        elevation(basin, 3);";
    let program = parse(source).unwrap();
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    let projection = project_landscape_features(&program);
    assert!(projection.errors.is_empty(), "{:?}", projection.errors);
    assert_eq!(projection.features.len(), 3);
    assert_eq!(projection.features[0].kind, LandscapeKind::Land);
    assert_eq!(projection.features[0].name.as_deref(), Some("shore"));
    assert_eq!(projection.features[0].elevation, 2.0);
    assert_eq!(
        projection.features[0].points,
        [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0]]
    );
    assert_eq!(projection.features[1].kind, LandscapeKind::Lake);
    assert_eq!(
        projection.features[1].points,
        [[10.0, -5.0], [12.0, -5.0], [11.0, -3.0]]
    );
    assert_eq!(projection.features[1].elevation, 0.0);
    assert_eq!(projection.features[2].kind, LandscapeKind::River);
    assert_eq!(projection.features[2].points, [[11.0, -3.0], [14.0, -2.0]]);
    assert_eq!(projection.features[2].elevation, 1.0);
}

#[test]
fn invalid_landscape_calls_do_not_hide_valid_shapes() {
    let program = parse(
        "\
        land(0, 0, 1, 0);\n\
        river(0, 0, 1, 1);\n\
        lake(0, 0, x, 1, 2, 2);\n\
        block recursive: recursive(0, 0); end;\n\
        recursive(1, 1);",
    )
    .unwrap();
    let projection = project_landscape_features(&program);
    assert_eq!(projection.features.len(), 1);
    assert_eq!(projection.features[0].kind, LandscapeKind::River);
    assert_eq!(projection.errors.len(), 3);
}
