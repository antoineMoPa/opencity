use opencity::{parse, project_landscape_features, BuildingMaterial, BuildingStyle, LandscapeKind};

#[test]
fn material_keywords_and_shared_colors_are_explicit() {
    for (material, keyword, shader_id) in [
        (BuildingMaterial::Concrete, "concrete", 2),
        (BuildingMaterial::Glass, "glass", 1),
        (BuildingMaterial::Brick, "brick", 3),
        (BuildingMaterial::RedBrick, "red_brick", 11),
        (BuildingMaterial::BrownBrick, "brown_brick", 12),
        (BuildingMaterial::BuffBrick, "buff_brick", 13),
        (BuildingMaterial::PaleBrick, "pale_brick", 14),
        (BuildingMaterial::Limestone, "limestone", 15),
    ] {
        assert_eq!(BuildingMaterial::from_keyword(keyword), Some(material));
        assert_eq!(material.keyword(), keyword);
        assert_eq!(material.shader_id(), shader_id);
        assert_ne!(material.wall_rgb(), material.roof_rgb());
    }
    assert_eq!(BuildingMaterial::from_keyword("Glass"), None);
    assert_eq!(BuildingMaterial::Concrete.roof_rgb(), [192, 198, 194]);
}

#[test]
fn building_style_survives_source_roundtrip_and_projection() {
    let program = parse(
        "tower = building(0.3, {material: glass, floor_height: 0.05}, 0,0, 1,0, 1,1);\
         brick_house = building(0.1, {material: brick}, 2,0, 3,0, 3,1);",
    )
    .unwrap();
    let rewritten = parse(&program.to_road_source()).unwrap();
    assert_eq!(program, rewritten);
    let projection = project_landscape_features(&rewritten);
    assert!(projection.errors.is_empty(), "{:?}", projection.errors);
    assert_eq!(projection.features.len(), 2);
    assert_eq!(
        projection.features[0].building_style,
        Some(BuildingStyle {
            material: BuildingMaterial::Glass,
            floor_height: 0.05,
        })
    );
    assert_eq!(
        projection.features[1].building_style,
        Some(BuildingStyle {
            material: BuildingMaterial::Brick,
            ..BuildingStyle::default()
        })
    );
}

#[test]
fn legacy_buildings_keep_default_style_and_optional_models() {
    let program = parse(
        "building(0.1, 0,0, 1,0, 1,1);\
         building(0.2, \"assets/hall.glb\", 2,0, 3,0, 3,1);\
         land(0,0, 1,0, 1,1);\
         building(0.2, {}, 4,0, 5,0, 5,1);",
    )
    .unwrap();
    let projection = project_landscape_features(&program);
    assert!(projection.errors.is_empty(), "{:?}", projection.errors);
    for feature in &projection.features {
        assert_eq!(
            feature.building_style,
            (feature.kind == LandscapeKind::Building).then(BuildingStyle::default)
        );
    }
    assert_eq!(
        projection.features[1].model.as_deref(),
        Some("assets/hall.glb")
    );
}

#[test]
fn glb_path_can_precede_building_options() {
    let program = parse(
        "building(0.2, \"assets/hall.glb\", {material: brick, floor_height: 0.03}, 0,0, 1,0, 1,1);",
    )
    .unwrap();
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    let projection = project_landscape_features(&program);
    assert!(projection.errors.is_empty(), "{:?}", projection.errors);
    assert_eq!(
        projection.features[0].model.as_deref(),
        Some("assets/hall.glb")
    );
    assert_eq!(
        projection.features[0].building_style,
        Some(BuildingStyle {
            material: BuildingMaterial::Brick,
            floor_height: 0.03,
        })
    );
}

#[test]
fn building_options_bind_independently_in_typed_blocks() {
    let program = parse(
        "block tower({height, facade, floor}: {height: number, facade: identifier, floor: number}):\
           building(height, {material: facade, floor_height: floor}, 0,0, 1,0, 1,1);\
         end \
         tower(10,20,0.4,{height:0.3, facade:glass, floor:0.05});\
         tower(-10,-20,{height:0.1, facade:brick, floor:0.025});",
    )
    .unwrap();
    let projection = project_landscape_features(&program);
    assert!(projection.errors.is_empty(), "{:?}", projection.errors);
    assert_eq!(projection.features.len(), 2);
    let first = &projection.features[0];
    assert_eq!(first.points[0], [10.0, 20.0]);
    assert_eq!(first.elevation, 0.4);
    assert_eq!(first.height, Some(0.3));
    assert_eq!(
        first.building_style.unwrap().material,
        BuildingMaterial::Glass
    );
    let second = &projection.features[1];
    assert_eq!(second.points[0], [-10.0, -20.0]);
    assert_eq!(second.height, Some(0.1));
    assert_eq!(
        second.building_style.unwrap().material,
        BuildingMaterial::Brick
    );
    assert_eq!(second.building_style.unwrap().floor_height, 0.025);
}

#[test]
fn invalid_building_options_are_reported_without_geometry() {
    for options in [
        "{material: Glass}",
        "{material: stone}",
        "{material: \"glass\"}",
        "{material: 1}",
        "{material: {}}",
        "{floor_height: 0}",
        "{floor_height: -0.01}",
        "{floor_height: \"0.04\"}",
        "{floor_height: huge}",
        "{floor_height: {}}",
        "{floor_height: 999999999999999999999999999999999999999999}",
        "{colour: brick}",
    ] {
        let program = parse(&format!("building(0.2, {options}, 0,0, 1,0, 1,1);")).unwrap();
        let projection = project_landscape_features(&program);
        assert_eq!(projection.errors.len(), 1, "{options}");
        assert!(projection.features.is_empty(), "{options}");
    }
    for options in [
        "{material: glass, material: brick}",
        "{floor_height: 0.04, floor_height: 0.03}",
    ] {
        assert!(parse(&format!("building(0.2, {options}, 0,0, 1,0, 1,1);")).is_err());
    }
    for height in ["0", "-1", "999999999999999999999999999999999999999999"] {
        let program = parse(&format!(
            "building({height}, {{material: glass}}, 0,0, 1,0, 1,1);"
        ))
        .unwrap();
        let projection = project_landscape_features(&program);
        assert_eq!(projection.errors.len(), 1, "{height}");
        assert!(projection.features.is_empty(), "{height}");
    }
}
