use opencity::{map, parse, project_landscape_features, Argument, BlockParameterType, Statement};

const EXAMPLE: &str = include_str!("../examples/parameterized_blocks.road");

#[test]
fn typed_objects_round_trip_and_keep_definitions_reusable() {
    let program = parse(EXAMPLE).unwrap();
    let Statement::Block(block) = &program.statements[1] else {
        panic!("expected block")
    };
    let parameters = block.parameters.as_ref().unwrap();
    assert_eq!(parameters[0].name, "length");
    assert_eq!(parameters[0].kind, BlockParameterType::Number);
    assert_eq!(
        block.calls[0].args[2],
        Argument::Identifier("length".into())
    );
    let saved = program.to_road_source();
    assert!(!saved.contains("end;"));
    assert!(saved.contains("({ length, height }: { length: number, height: number })"));
    assert_eq!(parse(&saved).unwrap(), program);

    let roads = map::project(&program);
    assert_eq!(roads.unresolved, 0);
    assert_eq!(roads.segments.len(), 2);
    assert_eq!(
        (roads.segments[0].start.x, roads.segments[0].end.x),
        (10.0, 15.0)
    );
    assert_eq!(
        (roads.segments[1].start.x, roads.segments[1].end.x),
        (20.0, 28.0)
    );
    assert_eq!(
        (roads.segments[0].start_z, roads.segments[0].end_z),
        (0.0, 0.5)
    );
    assert_eq!(
        (roads.segments[1].start_z, roads.segments[1].end_z),
        (1.0, 2.0)
    );
    assert_ne!(roads.segments[0].source, roads.segments[1].source);

    let landscape = project_landscape_features(&program);
    assert!(landscape.errors.is_empty());
    assert_eq!(landscape.features.len(), 2);
    assert_eq!(
        landscape.features[0].points,
        [[10.0, 20.0], [15.0, 20.0], [15.0, 21.0]]
    );
    assert_eq!(
        landscape.features[1].points,
        [[20.0, 20.0], [28.0, 20.0], [28.0, 21.0]]
    );
    assert_eq!(landscape.features[1].elevation, 1.0);
}

#[test]
fn nested_objects_forward_values_with_lexical_scope_and_stacked_heights() {
    let program = parse(
        r#"
        road street: carlane; end;
        block leaf({ length, height }: { height: number, length: number }):
            lane = street(0, 0, length, 0);
            grade(lane, 0, height);
            water = river(0, 0, length, 0);
            elevation(water, height);
        end;
        block group({ length }: { length: number }):
            a = leaf(1, 2, 0.25, { height: 0.5, length });
            elevation(a, 0.5);
            leaf(3, 4, { length: 2, height: 1 });
        end;
        group(10, 20, 1, { length: 6 });
        group(-10, -20, { length: 9 });
    "#,
    )
    .unwrap();
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    let roads = map::project(&program);
    assert_eq!(roads.unresolved, 0);
    assert_eq!(roads.segments.len(), 4);
    let a = &roads.segments[0];
    assert_eq!(
        (a.start.x, a.start.y, a.end.x, a.end.y),
        (11.0, 22.0, 17.0, 22.0)
    );
    assert_eq!((a.start_z, a.end_z), (1.75, 2.25));
    assert_eq!(roads.segments[1].end.x, 15.0);
    assert_eq!(roads.segments[2].end.x, 0.0);
    assert_eq!(roads.segments[3].end.x, -5.0);
    let landscape = project_landscape_features(&program);
    assert!(landscape.errors.is_empty());
    assert_eq!(landscape.features.len(), 4);
    assert_eq!(landscape.features[0].elevation, 2.25);
}

#[test]
fn string_and_identifier_parameters_keep_their_argument_kinds() {
    let program = parse(
        r#"
        road street: carlane; end;
        block piece({ profile, asset }: { profile: identifier, asset: string }):
            profile(0, 0, 2, 0);
            building(0.4, asset, 0, 0, 2, 0, 2, 1);
        end;
        piece(1, 2, { asset: "models/tree.glb", profile: street });
    "#,
    )
    .unwrap();
    let Statement::Block(block) = &program.statements[1] else {
        panic!("expected block")
    };
    let Statement::Call(call) = &program.statements[2] else {
        panic!("expected call")
    };
    let bound = block.instantiate(&call.args).unwrap();
    assert_eq!(bound.calls[0].name, "street");
    assert_eq!(
        bound.calls[1].args[1],
        Argument::String("\"models/tree.glb\"".into())
    );
    assert_eq!(map::project(&program).segments.len(), 1);
    let landscape = project_landscape_features(&program);
    assert!(landscape.errors.is_empty());
    assert_eq!(
        landscape.features[0].model.as_deref(),
        Some("models/tree.glb")
    );
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn invalid_parameter_declarations_have_source_locations() {
    for declaration in [
        "{ length, length }: { length: number }",
        "{ length }: { length: number, length: number }",
        "{ length }: { other: number }",
        "{ length }: {}",
        "{ length }: { length: boolean }",
        "{ length }",
    ] {
        let error = parse(&format!("\nblock piece({declaration}): end;")).unwrap_err();
        assert_eq!(error.span.line, 2, "{declaration}");
    }
    let error = parse("block piece({ length }: { length: number }): length = river(0,0,1,0); end;")
        .unwrap_err();
    assert!(error.message.contains("shadow"));
    let error = parse("piece(0, 0, { length: 1, length: 2 });").unwrap_err();
    assert!(error.message.contains("duplicate object property"));
}

#[test]
fn invalid_bindings_report_errors_without_hiding_valid_instances() {
    for (args, expected) in [
        ("0, 0", "parameter object"),
        ("0, 0, {}", "missing block parameter 'length'"),
        (
            "0, 0, { length: 1, other: 2 }",
            "unknown block parameter 'other'",
        ),
        ("0, 0, { length: \"long\" }", "must be number"),
        ("0, 0, { length: nope }", "must be number"),
        ("0, 0, { length: {} }", "must be number"),
        ("0, { length: 1 }", "block placement needs"),
        ("nope, 0, { length: 1 }", "block x must"),
    ] {
        let source = format!(
            "road street: carlane; end;
            block piece({{ length }}: {{ length: number }}):
                street(0, 0, length, 0); river(0, 0, length, 0);
            end;
            piece({args}); piece(0, 0, {{ length: 2 }});"
        );
        let program = parse(&source).unwrap();
        let roads = map::project(&program);
        assert_eq!(roads.unresolved, 1, "{args}");
        assert_eq!(roads.segments.len(), 1, "{args}");
        let landscape = project_landscape_features(&program);
        assert_eq!(landscape.features.len(), 1, "{args}");
        assert_eq!(landscape.errors.len(), 1, "{args}");
        assert!(
            landscape.errors[0].message.contains(expected),
            "{args}: {:?}",
            landscape.errors
        );
    }
}

#[test]
fn empty_objects_trailing_commas_and_recursive_blocks() {
    let program = parse("block empty({}: {}): end; empty(0,0,{});").unwrap();
    assert_eq!(map::project(&program).unresolved, 0);
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    let program = parse("road street: carlane; end; block loop({ length, }: { length: number, }): street(0,0,length,0); loop(0,0,{length,}); end; loop(0,0,{length: 2,});").unwrap();
    let roads = map::project(&program);
    assert_eq!(roads.segments.len(), 1);
    assert_eq!(roads.unresolved, 1);
    assert!(project_landscape_features(&program).errors[0]
        .message
        .contains("recursive"));
}
