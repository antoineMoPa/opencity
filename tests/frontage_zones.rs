use opencity::{map, parse, project_frontage_zones, BuildingUse, FrontageSide};

const ROAD: &str = "road street: sidewalk carlane carlane sidewalk; end \
    main = street(0,0, 2,0);";
const OPTIONS: &str = "{use: commercial, side: both, start: 0.1, end: 0.9, depth: 0.16, \
    seed: 18446744073709551615}";

#[test]
fn stable_names_exact_seed_and_roundtrip() {
    let program = parse(&format!("{ROAD} market = frontage_zone(main, {OPTIONS});")).unwrap();
    let saved = parse(&program.to_road_source()).unwrap();
    assert_eq!(program, saved);
    let result = project_frontage_zones(&saved);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let zone = &result.zones[0];
    assert_eq!(zone.name, "market");
    assert_eq!(zone.road_placement, "main");
    assert_eq!(zone.seed, u64::MAX);
    assert_eq!(zone.building_use, BuildingUse::Commercial);
    assert_eq!(zone.strip_quads(8).len(), 16);
    let left = zone.frame_at(FrontageSide::Left, 0.5).unwrap();
    let right = zone.frame_at(FrontageSide::Right, 0.5).unwrap();
    let width: f32 = saved
        .statements
        .iter()
        .find_map(|s| match s {
            opencity::Statement::Road(r) => Some(
                r.elements
                    .iter()
                    .map(|e| map::element_world_width(e.kind))
                    .sum(),
            ),
            _ => None,
        })
        .unwrap();
    assert!((left.origin[1] - width / 2.0).abs() < 1e-6);
    assert!((right.origin[1] + width / 2.0).abs() < 1e-6);
    assert_eq!(left.outward, [0.0, 1.0]);
    assert_eq!(right.outward, [0.0, -1.0]);
    assert!(zone.frame_at(FrontageSide::Both, 0.5).is_none());
    assert!(zone.frame_at(FrontageSide::Left, f32::NAN).is_none());
    assert!(zone.frame_at(FrontageSide::Left, 1.0).is_none());
}

#[test]
fn malformed_options_report_errors_without_zones() {
    for options in [
        OPTIONS.replace("commercial", "unspecified"),
        OPTIONS.replace("commercial", "retail"),
        OPTIONS.replace("both", "east"),
        OPTIONS.replace("0.1", "-0.1"),
        OPTIONS.replace("0.9", "0.1"),
        OPTIONS.replace("0.9", "1.1"),
        OPTIONS.replace("0.16", "0"),
        OPTIONS.replace("0.16", "-0.1"),
        OPTIONS.replace("0.16", "999999999999999999999999999999999999999999999999"),
        OPTIONS.replace("18446744073709551615", "18446744073709551616"),
        OPTIONS.replace("18446744073709551615", "1.2"),
        OPTIONS.replace("18446744073709551615", "-1"),
        OPTIONS.replace("side: both,", ""),
        OPTIONS.replace("side: both,", "side: both, side: left,"),
        OPTIONS.replace("depth:", "width:"),
    ] {
        let source = format!("{ROAD} zone = frontage_zone(main, {options});");
        let program = match parse(&source) {
            Ok(program) => program,
            Err(error) => {
                assert!(options.contains("side: both, side: left,"));
                assert!(error.message.contains("duplicate object property"));
                continue;
            }
        };
        let result = project_frontage_zones(&program);
        assert!(result.zones.is_empty(), "{source}");
        assert_eq!(result.errors.len(), 1, "{source}");
    }
}

#[test]
fn missing_ambiguous_and_unsupported_references_are_not_guessed() {
    for source in [
        format!("{ROAD} z = frontage_zone(missing, {OPTIONS});"),
        format!("{ROAD} z = frontage_zone(street, {OPTIONS});"),
        format!("{ROAD} main = street(0,1,2,1); z = frontage_zone(main, {OPTIONS});"),
        format!("{ROAD} z = frontage_zone(main, {OPTIONS}); z = frontage_zone(main, {OPTIONS});"),
        format!("{ROAD} frontage_zone(main, {OPTIONS});"),
        format!("{ROAD} a,b = frontage_zone(main, {OPTIONS});"),
        format!("{ROAD} z = frontage_zone(main);"),
        format!(
            "road street: carlane; end block part: main=street(0,0,1,0); end \
            part(0,0); z = frontage_zone(main, {OPTIONS});"
        ),
        format!("{ROAD} block part: z = frontage_zone(main, {OPTIONS}); end"),
        format!(
            "road street: carlane; end main=street(0,0,0,0); \
            z = frontage_zone(main, {OPTIONS});"
        ),
    ] {
        let result = project_frontage_zones(&parse(&source).unwrap());
        assert!(result.zones.is_empty(), "{source}");
        assert!(!result.errors.is_empty(), "{source}");
    }
}

#[test]
fn road_rename_updates_zone_reference_and_removal_diagnoses_it() {
    let mut program = parse(&format!("{ROAD} z = frontage_zone(main, {OPTIONS});")).unwrap();
    let source = map::project(&program).segments[0].source;
    assert!(map::set_name(&mut program, source, "renamed".into()));
    let result = project_frontage_zones(&program);
    assert!(result.errors.is_empty());
    assert_eq!(result.zones[0].road_placement, "renamed");
    program.statements.retain(|s| {
        !matches!(s, opencity::Statement::Call(c)
        if c.assigns.iter().any(|name| name == "renamed"))
    });
    let result = project_frontage_zones(&program);
    assert!(result.zones.is_empty());
    assert_eq!(result.errors.len(), 1);
}

#[test]
fn samples_cover_curves_and_both_authored_orientations() {
    for road in [
        "main=street(0,0,2,0);",
        "main=street(2,0,0,0);",
        "main=street(0,0,1,0,2,1,2,2);",
    ] {
        let source = format!(
            "road street: sidewalk carlane sidewalk; end \
            {road} z=frontage_zone(main,{OPTIONS});"
        );
        let result = project_frontage_zones(&parse(&source).unwrap());
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let zone = &result.zones[0];
        for side in [FrontageSide::Left, FrontageSide::Right] {
            for t in [0.1, 0.5, 0.9] {
                let frame = zone.frame_at(side, t).unwrap();
                let cross = frame.along[0] * frame.outward[1] - frame.along[1] * frame.outward[0];
                assert!(
                    (cross
                        - if side == FrontageSide::Left {
                            1.0
                        } else {
                            -1.0
                        })
                    .abs()
                        < 1e-5
                );
            }
        }
        assert_eq!(zone.strip_quads(32).len(), 64);
    }
}

#[test]
fn examples_project_and_retain_stable_zone_ids() {
    for (source, count) in [
        (include_str!("../examples/default_map.road"), 383),
        (include_str!("../examples/frontage_zones.road"), 3),
    ] {
        let program = parse(source).unwrap();
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
        let result = project_frontage_zones(&program);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.zones.len(), count);
        assert!(result.zones.iter().all(|z| !z.strip_quads(24).is_empty()));
    }
}

#[test]
fn town_corridors_cover_complete_streets_every_third_block() {
    let program = parse(include_str!("../examples/default_map.road")).unwrap();
    let result = project_frontage_zones(&program);
    let rows = [
        (
            "west",
            vec![(-4.8, 6), (-2.7, 6), (-0.6, 9), (1.5, 15), (3.6, 15)],
        ),
        ("east", vec![(-1.2, 9), (0.6, 9), (2.4, 15)]),
    ];
    for (town, expected) in rows {
        for (y, count) in &expected {
            let zones: Vec<_> = result
                .zones
                .iter()
                .filter(|z| {
                    z.road_placement.starts_with(town)
                        && !z.name.starts_with("housing_")
                        && !z.name.starts_with("centre_")
                        && (z.segment().start.y - y).abs() < 1e-5
                })
                .collect();
            assert_eq!(zones.len(), *count, "{town} row {y}");
            let mut intervals: Vec<_> = zones
                .iter()
                .map(|z| {
                    assert_eq!(z.side, FrontageSide::Both);
                    assert_eq!((z.start, z.end), (0., 1.));
                    let s = z.segment();
                    assert!((s.end.y - y).abs() < 1e-5);
                    let junction = |r: &Option<String>| {
                        r.as_ref().unwrap().split('.').next().unwrap().to_owned()
                    };
                    if s.start.x < s.end.x {
                        (s.start.x, junction(&s.start_ref), junction(&s.end_ref))
                    } else {
                        (s.end.x, junction(&s.end_ref), junction(&s.start_ref))
                    }
                })
                .collect();
            intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
            for pair in intervals.windows(2) {
                // Road segments stop at junction ports, so connectivity is
                // defined by the shared junction rather than coincident ends.
                assert_eq!(pair[0].2, pair[1].1, "gap in {town} row {y}");
            }
        }
        assert!(result
            .zones
            .iter()
            .filter(|z| z.road_placement.starts_with(town)
                && !z.name.starts_with("housing_")
                && !z.name.starts_with("centre_"))
            .all(|z| expected
                .iter()
                .any(|(y, _)| (z.segment().start.y - y).abs() < 1e-5)));
    }
}

#[cfg(feature = "game-runtime")]
#[test]
fn city_exposes_resolved_roads_and_rejects_invalid_zones() {
    use opencity::runtime::City;
    let program = parse(&format!("{ROAD} z=frontage_zone(main,{OPTIONS});")).unwrap();
    let city = City::from_program(&program, None).unwrap();
    assert_eq!(city.frontage_zones.len(), 1);
    let zone = &city.frontage_zones[0];
    assert_eq!(
        city.frontage_road(zone).unwrap().segment.name.as_deref(),
        Some("main")
    );
    let invalid = parse(&format!("{ROAD} z=frontage_zone(missing,{OPTIONS});")).unwrap();
    assert!(City::from_program(&invalid, None).is_err());
}

#[test]
fn direct_ast_mutations_reject_nonfinite_and_duplicate_options() {
    for value in ["NaN", "inf", "-inf"] {
        let mut program = parse(&format!("{ROAD} z=frontage_zone(main,{OPTIONS});")).unwrap();
        let opencity::Statement::Call(call) = program.statements.last_mut().unwrap() else {
            unreachable!();
        };
        let opencity::Argument::Object(options) = &mut call.args[1] else {
            unreachable!();
        };
        options
            .iter_mut()
            .find(|(key, _)| key == "depth")
            .unwrap()
            .1 = opencity::Argument::Number(value.into());
        let projection = project_frontage_zones(&program);
        assert!(projection.zones.is_empty());
        assert_eq!(projection.errors.len(), 1);
    }
    let mut program = parse(&format!("{ROAD} z=frontage_zone(main,{OPTIONS});")).unwrap();
    let opencity::Statement::Call(call) = program.statements.last_mut().unwrap() else {
        unreachable!();
    };
    let opencity::Argument::Object(options) = &mut call.args[1] else {
        unreachable!();
    };
    options.push(options[0].clone());
    let projection = project_frontage_zones(&program);
    assert!(projection.zones.is_empty());
    assert!(projection.errors[0]
        .message
        .contains("duplicate frontage option"));
}

#[test]
fn permitted_travel_does_not_flip_authored_sides() {
    let frames: Vec<_> = ["forward", "reverse", "both"]
        .into_iter()
        .map(|direction| {
            let source = format!(
                "road street: sidewalk carlane sidewalk; direction {direction}; end \
            main=street(0,0,2,0); z=frontage_zone(main,{OPTIONS});"
            );
            let projection = project_frontage_zones(&parse(&source).unwrap());
            assert!(projection.errors.is_empty());
            projection.zones[0]
                .frame_at(FrontageSide::Left, 0.5)
                .unwrap()
        })
        .collect();
    assert!(frames.windows(2).all(|pair| pair[0] == pair[1]));
}
