use opencity::{
    parse, parse_road_file, project_landscape_features, tokenize, Argument, LandscapeKind,
    RoadElementKind, Statement, TokenKind,
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
