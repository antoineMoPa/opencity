#![cfg(feature = "game-runtime")]
use opencity::{
    parse,
    runtime::{scaffold, City},
    BuildingForm, BuildingUse,
};
fn sample(use_kind: &str, options: &str) -> opencity::Program {
    parse(&format!(
        "road local: sidewalk carlane sidewalk; end
        street=local(-2,0,2,0);
        ground=land(-3,-2,3,-2,3,2,-3,2);elevation(ground,0.02);
        zone=frontage_zone(street,{{use:{use_kind},side:both,start:0.1,end:0.9,
        depth:0.2,seed:42{options}}});
        protected=building(0.4,-0.1,0.1,0.1,0.1,0.1,0.4,-0.1,0.4);"
    ))
    .unwrap()
}
fn expand(p: &opencity::Program) -> opencity::Program {
    scaffold::expand(p, |p| City::from_program(p, None))
        .unwrap()
        .0
}
#[test]
fn program_only_expansion_matches_full_expansion_with_one_compilation() {
    let input = sample("mixed_use", "");
    let mut compilations = 0;
    let program = scaffold::expand_program(&input, |p| {
        compilations += 1;
        City::from_program(p, None)
    })
    .unwrap();
    assert_eq!(compilations, 1);
    let (full, city) = scaffold::expand(&input, |p| City::from_program(p, None)).unwrap();
    assert_eq!(program, full);
    assert_eq!(
        scaffold::buildings(&program).len() + 1,
        city.query(opencity::runtime::Region::WholeMap)
            .buildings
            .len()
    );
}

#[test]
fn program_only_expansion_preserves_legacy_residential_obstacles() {
    let mut input = sample("mixed_use", "");
    input.statements.extend(
        parse("residential_area(42,-2,-1,2,1,0,0,0);")
            .unwrap()
            .statements,
    );
    let program = scaffold::expand_program(&input, |p| City::from_program(p, None)).unwrap();
    assert_eq!(program, expand(&input));
}

#[test]
fn unzoned_expansion_compiles_only_once() {
    let input = parse("road local: carlane; end; local(0,0,1,0);").unwrap();
    let mut compilations = 0;
    let (program, _) = scaffold::expand(&input, |p| {
        compilations += 1;
        City::from_program(p, None)
    })
    .unwrap();
    assert_eq!(compilations, 1);
    assert_eq!(program, input);
}
#[test]
fn deterministic_shared_layout_is_idempotent_and_preserves_protected_volumes() {
    let input = sample("mixed_use", "");
    let a = expand(&input);
    assert_eq!(a, expand(&input));
    assert_eq!(a, expand(&a));
    let volumes = scaffold::buildings(&a);
    assert!(volumes.len() > 10);
    let authored = opencity::project_landscape_features(&a)
        .features
        .into_iter()
        .find(|f| f.name.as_deref() == Some("protected"))
        .unwrap();
    for volume in &volumes {
        assert!(!scaffold::overlap(&volume.feature.points, &authored.points));
    }
    let mut reordered = input.clone();
    reordered.statements.reverse();
    let mut b = scaffold::buildings(&expand(&reordered));
    b.sort_by(|a, b| a.id.cmp(&b.id));
    let mut a = volumes;
    a.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(a, b);
}
#[test]
fn density_removes_stable_slots_and_city_centre_form_accepts_every_use() {
    let full = scaffold::buildings(&expand(&sample("commercial", "")));
    let sparse = scaffold::buildings(&expand(&sample("commercial", ",density:0.5")));
    assert!(!sparse.is_empty() && sparse.len() < full.len());
    assert!(sparse.iter().all(|b| full.contains(b)));
    for use_kind in ["residential", "commercial", "mixed_use"] {
        let p = sample(
            use_kind,
            ",form:city_centre,height_min:0.3,height_max:0.5,\
            frontage_min:0.17,frontage_max:0.34,setback:0.02",
        );
        let zones = opencity::project_frontage_zones(&p);
        assert_eq!(zones.zones[0].form.form, BuildingForm::CityCentre);
        let buildings = scaffold::buildings(&expand(&p));
        assert!(buildings.len() > 5);
        for b in buildings {
            assert!((0.3..=0.5).contains(&b.feature.height.unwrap()));
            assert_eq!(
                b.feature.building_use,
                BuildingUse::from_keyword(use_kind).unwrap()
            );
        }
        assert_eq!(p, parse(&p.to_road_source()).unwrap());
    }
    assert!(scaffold::buildings(&expand(&sample("residential", ",density:0"))).is_empty());
}
#[test]
fn invalid_form_ranges_are_diagnostics() {
    for option in [
        ",density:1.1",
        ",height_min:0.4,height_max:0.1",
        ",frontage_min:0",
        ",setback:0.3",
        ",form:residential",
    ] {
        assert!(
            !opencity::project_frontage_zones(&sample("residential", option))
                .errors
                .is_empty(),
            "{option}"
        );
    }
}

#[test]
fn neutral_roof_meshes_are_closed_and_stay_inside_allocated_height() {
    let source = expand(&sample("mixed_use", ""));
    let mut roofs = std::collections::HashSet::new();
    for building in scaffold::buildings(&source) {
        roofs.insert(format!("{:?}", building.roof));
        let mesh = building.mesh();
        assert!(!mesh.vertices.is_empty());
        let mut edges = std::collections::BTreeMap::new();
        for triangle in mesh.vertices.chunks_exact(3) {
            for i in 0..3 {
                let point = |i: usize| triangle[i].position.map(f32::to_bits);
                let (a, b) = (point(i), point((i + 1) % 3));
                let key = if a < b { (a, b) } else { (b, a) };
                *edges.entry(key).or_insert(0) += 1;
            }
        }
        assert!(edges.values().all(|n| *n == 2), "open mesh {}", building.id);
        assert!(mesh
            .vertices
            .iter()
            .all(|v| v.position[2] >= building.feature.elevation
                && v.position[2]
                    <= building.feature.elevation + building.feature.height.unwrap() + 1e-6));
    }
    assert_eq!(roofs.len(), 3);
}

#[test]
fn residential_scaffolds_respect_retail_reservations_and_thin_water_obstacles() {
    let mut p = sample("commercial", "");
    p.statements.extend(
        parse(
            "road cross: sidewalk carlane sidewalk; end
        crossroad=cross(0,-1,0,1);
        a_housing=frontage_zone(crossroad,{use:residential,side:both,start:0.1,end:0.9,
            depth:0.2,seed:24,setback:0.04});
        pond=lake(-1.2,0.15,-1.19,0.15,-1.19,0.19,-1.2,0.19);elevation(pond,0.02);",
        )
        .unwrap()
        .statements,
    );
    let strips = scaffold::strip_footprints(&opencity::project_frontage_zones(&p).zones[0]);
    let out = expand(&p);
    let water = opencity::project_landscape_features(&out)
        .features
        .into_iter()
        .find(|f| f.name.as_deref() == Some("pond"))
        .unwrap();
    let buildings = scaffold::buildings(&out);
    assert!(buildings
        .iter()
        .any(|b| b.feature.building_use == BuildingUse::Residential));
    for b in buildings {
        assert!(!scaffold::overlap(&b.feature.points, &water.points));
        if b.feature.building_use == BuildingUse::Residential {
            assert!(strips
                .iter()
                .all(|p| !scaffold::overlap(&b.feature.points, p)));
        }
    }
}
