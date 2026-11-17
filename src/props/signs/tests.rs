use super::*;
#[test]
fn coincident_signs_and_building_overlaps_are_filtered_before_rendering() {
    let program=crate::parse("road r: carlane; end r(-1,0,1,0); land(-2,-2,2,-2,2,2,-2,2); house=building(0.06,{collision:none},0.018,-0.03,0.025,-0.03,0.025,0.03,0.018,0.03); a=prop(highway_sign,0,0); b=prop(highway_sign,0,0,{heading:90}); c=prop(highway_sign,0,0,{elevation:0.1}); d=prop(stop_sign,0.1,0); e=prop(stop_sign,0.1,0,{heading:360}); f=prop(speed_sign,0.1,0);").unwrap();
    let city = City::from_program(&program, None).unwrap();
    let entries = super::super::read(&program).unwrap();
    let visible = super::super::visible_placements(&entries, &city);
    let names: Vec<_> = visible
        .iter()
        .map(|(e, _, _)| entries[*e].name.as_str())
        .collect();
    assert_eq!(
        names,
        ["b", "c", "d"],
        "panel bounds, heading, vertical separation and duplicate poses must all count"
    );
    assert!(
        city.building_bounds_overlap([0.019, -0.01, 0.01], [0.02, 0.01, 0.02]),
        "visual filtering includes non-collidable buildings"
    );
}
#[test]
fn stop_face_background_and_borders_do_not_overlap() {
    let parts = SignDesign::Stop { cluster: false }.parts();
    let panel = &parts[1];
    let area: f32 = panel
        .vertices
        .chunks_exact(3)
        .filter(|t| {
            t.iter()
                .all(|v| (v.position[1] - (-0.07 - 0.008) / UNITS).abs() < 1e-9)
        })
        .map(|t| {
            let a = Vec2::new(t[0].position[0], t[0].position[2]);
            let b = Vec2::new(t[1].position[0], t[1].position[2]);
            let c = Vec2::new(t[2].position[0], t[2].position[2]);
            ((b - a).perp_dot(c - a)).abs() * 0.5
        })
        .sum();
    let expected = 0.0075_f32.powi(2) * 2. * (std::f32::consts::SQRT_2 - 1.);
    assert!(
        (area - expected).abs() < 1e-10,
        "border strips must partition a single octagon face, {area} vs {expected}"
    );
}
#[test]
fn stop_outline_has_eight_equal_edges_and_angles() {
    let p = geometry::octagon(0.75);
    let sides: Vec<_> = (0..8)
        .map(|i| Vec2::from_array(p[i]).distance(Vec2::from_array(p[(i + 1) % 8])))
        .collect();
    for i in 0..8 {
        assert!((sides[i] - sides[0]).abs() < 1e-6);
        let a = Vec2::from_array(p[(i + 7) % 8]) - Vec2::from_array(p[i]);
        let b = Vec2::from_array(p[(i + 1) % 8]) - Vec2::from_array(p[i]);
        assert!(
            (a.dot(b) / (a.length() * b.length()) + std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6
        );
    }
}
#[test]
fn sign_faces_are_in_front_of_their_supports() {
    for kind in Kind::ALL.into_iter().filter(|k| k.is_road_sign()) {
        let design = SignDesign::for_kind(kind).unwrap();
        let profiles = design.profiles();
        for panel in profiles.iter().filter(|p| !p.pole) {
            for pole in profiles.iter().filter(|p| p.pole) {
                assert!(
                    panel.center.y + 0.008 < pole.center.y - pole.half.y,
                    "{:?}: supports must be behind the metal back",
                    kind
                );
            }
        }
    }
}
#[test]
fn all_sign_options_round_trip_including_rows_accents_and_quotes() {
    let source = r#"
road r: carlane; end r(-1,0,1,0);
stop=prop(stop_sign,0,0,{cluster:true});
speed=prop(speed_sign,0.1,0,{limit:90});
one=prop(one_way_sign,0.2,0,{arrow:left});
yield_a=prop(yield_sign,0.3,0);
curve=prop(curve_sign,0.4,0,{arrow:left});
exit_a=prop(highway_sign,0.5,0,{destination:"Québec",secondary:"L'Île \"verte\"",exit:"12-E",arrow:exit_right});
row_a=prop_row(highway_sign,0,0,0.5,0,1,0,{spacing:20,destination:"Montréal",secondary:"Centre-ville",exit:"",arrow:straight});
"#;
    let p = crate::parse(source).unwrap();
    let entries = super::super::read(&p).unwrap();
    let after = crate::Program {
        statements: entries
            .iter()
            .map(|e| crate::Statement::Call(super::super::write(e)))
            .collect(),
    };
    assert_eq!(
        super::super::read(&crate::parse(&after.to_road_source()).unwrap()).unwrap(),
        entries
    );
    let mut changed = p.clone();
    super::super::apply(&mut changed, &entries).unwrap();
    assert_eq!(changed.statements[..2], p.statements[..2]);
}
#[test]
fn sign_validation_rejects_incompatible_or_unbounded_options() {
    for source in [
        "a=prop(speed_sign,0,0,{limit:35});",
        "a=prop(speed_sign,0,0,{limit:200});",
        "a=prop(curve_sign,0,0,{arrow:straight});",
        "a=prop(one_way_sign,0,0,{arrow:exit_right});",
        "a=prop(yield_sign,0,0,{cluster:true});",
        "a=prop(stop_sign,0,0,{cluster:yes});",
        "a=prop(highway_sign,0,0,{destination:\"\"});",
        "a=prop(highway_sign,0,0,{destination:\"abcdefghijklmnopqrstuvwxyz0123456789\"});",
        "a=prop(stop_sign,0,0,{cluster:true,cluster:false});",
    ] {
        assert!(
            crate::parse(source)
                .map_err(|e| e.to_string())
                .and_then(|p| super::super::read(&p))
                .is_err(),
            "{source}"
        );
    }
}
#[test]
fn sign_meshes_share_geometry_and_keep_designs_distinct() {
    let a = SignDesign::Highway {
        destination: "Québec".into(),
        secondary: "Centre-ville".into(),
        exit: "12".into(),
        arrow: SignArrow::ExitRight,
    };
    let mut b = a.clone();
    if let SignDesign::Highway { destination, .. } = &mut b {
        *destination = "Montréal".into();
    }
    let first = a.parts();
    let second = a.parts();
    let other = b.parts();
    assert_eq!(first.len(), 3);
    for (x, y) in first.iter().zip(second.iter()) {
        assert!(Arc::ptr_eq(x, y));
    }
    assert!(!Arc::ptr_eq(&first[2], &other[2]));
    assert_ne!(first[2].vertices.len(), other[2].vertices.len());
    for kind in [
        Kind::StopSign,
        Kind::SpeedSign,
        Kind::OneWaySign,
        Kind::YieldSign,
        Kind::CurveSign,
        Kind::HighwaySign,
    ] {
        let design = SignDesign::for_kind(kind).unwrap();
        design.validate().unwrap();
        let parts = design.parts();
        let profiles = design.profiles();
        assert_eq!(parts.len(), profiles.len());
        for (part, profile) in parts.iter().zip(profiles) {
            assert!(!part.vertices.is_empty());
            assert!(part
                .vertices
                .iter()
                .all(|v| v.surface[1] == 1. && v.position.iter().all(|p| p.is_finite())));
            let center = profile.center / UNITS;
            let half = profile.half / UNITS;
            for vertex in &part.vertices {
                let delta = (Vec3::from_array(vertex.position) - center).abs();
                assert!(
                    delta.cmple(half + Vec3::splat(0.00001)).all(),
                    "{:?}: outside physical bounds {:?}",
                    kind,
                    delta
                );
            }
        }
    }
    assert_eq!(SignDesign::Stop { cluster: true }.parts().len(), 4);
}
