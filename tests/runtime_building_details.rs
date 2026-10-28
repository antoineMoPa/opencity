#![cfg(feature = "game-runtime")]

use opencity::{
    parse,
    runtime::{
        BuildingDetailConfig, BuildingPattern, City, FacadeKind, FencePattern, GardenEdge,
        GardenProp, Region, RoofKind, StairKind, SurfaceKind,
    },
};

fn config() -> BuildingDetailConfig {
    BuildingDetailConfig {
        seed: 12,
        patterns: vec![BuildingPattern {
            floors: 2,
            weight: 1,
            facade: FacadeKind::Stacked,
            roof: RoofKind::Cornice,
            stair: StairKind::Curved,
            trim: [0.7, 0.7, 0.6],
            metal: [0.05, 0.05, 0.05],
            door: [0.2, 0.1, 0.1],
        }],
        maximum_height: 0.12,
        minimum_frontage: 0.04,
        road_reach: 0.5,
        unit: 0.01,
        garden_depth: 0.02,
        garden_chance: 1.,
        garden_edges: vec![(GardenEdge::IronFence, 1)],
        garden_props: vec![(GardenProp::Planter, 1)],
    }
}

fn city() -> City {
    let source = "road street: sidewalk carlane sidewalk; end street(-1,0,1,0);\
        building(0.07, {material: brick, floor_height: 0.035}, -0.04,0.1, 0.04,0.1, 0.04,0.21, -0.04,0.21);";
    City::from_program(&parse(source).unwrap(), None).unwrap()
}

#[test]
fn fence_sections_use_two_triangles_with_local_pattern_coordinates_at_any_width() {
    for width in [0.08, 0.3] {
        let source = format!(
            "road street: sidewalk carlane sidewalk; end street(-1,0,1,0); \
             building(0.07, {{floor_height: 0.035}}, \
             0,0.1, {width},0.1, {width},0.21, 0,0.21);"
        );
        let city = City::from_program(&parse(&source).unwrap(), None).unwrap();
        for (edge, pattern) in [
            (GardenEdge::IronFence, FencePattern::IronPickets),
            (GardenEdge::WireFence, FencePattern::ChainLink),
        ] {
            let mut choices = config();
            choices.garden_edges = vec![(edge, 1)];
            choices.garden_props = vec![(GardenProp::None, 1)];
            let (_, objects) = city.building_detail_parts(Region::WholeMap, &choices);
            let panels: Vec<_> = objects
                .iter()
                .filter(|object| {
                    object
                        .vertices
                        .iter()
                        .all(|v| v.surface[0] == SurfaceKind::Fence as u32 as f32)
                })
                .collect();
            assert_eq!(panels.len(), 2, "two garden fence sections");
            for panel in panels {
                assert_eq!(
                    panel.vertices.len(),
                    6,
                    "fence cost must not grow with width"
                );
                assert!(panel.vertices.iter().all(|v| {
                    v.surface[1] >= 0.
                        && (0. ..=1.).contains(&v.surface[2])
                        && v.surface[3] == pattern as u32 as f32
                }));
                assert!(panel.vertices.iter().any(|v| v.surface[1] > 0.));
                assert!(panel.vertices.iter().any(|v| v.surface[2] == 1.));
            }
        }
    }
}

#[test]
fn four_storeys_receive_details_and_stairs_avoid_pavement() {
    let road = City::from_program(
        &parse("road street: sidewalk carlane sidewalk; end street(-1,0,1,0);").unwrap(),
        None,
    )
    .unwrap();
    let sidewalk_edge = (0..200)
        .map(|n| n as f32 * 0.001)
        .filter(|&y| {
            road.surface_at([0., y], 0.01)
                .is_some_and(|s| s.kind == SurfaceKind::Sidewalk)
        })
        .last()
        .expect("street has a sidewalk");
    let front = sidewalk_edge + 0.001;
    let source = format!(
        "road street: sidewalk carlane sidewalk; end street(-1,0,1,0); \
         building(0.14, {{material: brick, floor_height: 0.035}}, \
         -0.04,{front}, 0.04,{front}, 0.04,{}, -0.04,{});",
        front + 0.12,
        front + 0.12
    );
    let city = City::from_program(&parse(&source).unwrap(), None).unwrap();
    let mut options = config();
    options.maximum_height = 0.15;
    options.patterns[0].floors = 4;
    options.garden_chance = 0.;
    let (facade, props) = city.building_detail_parts(Region::WholeMap, &options);
    let windows = facade
        .vertices
        .iter()
        .filter(|v| v.surface[0] == SurfaceKind::Window as u32 as f32)
        .count();
    assert!(windows > 0, "four-storey facades should receive windows");
    assert!(
        props.is_empty(),
        "a flush building should not put stairs on the sidewalk"
    );
}

#[test]
fn facade_details_are_seeded_region_stable_and_face_the_road() {
    let city = city();
    let all = city.building_detail_mesh(Region::WholeMap, &config());
    let nearby = city.building_detail_mesh(Region::radius([0., 0.15], 0.15).unwrap(), &config());
    assert!(!all.vertices.is_empty());
    assert_eq!(all.vertices.len(), nearby.vertices.len());
    for (a, b) in all.vertices.iter().zip(&nearby.vertices) {
        assert_eq!(a.position, b.position);
        assert_eq!(a.color, b.color);
        assert!(a.position.iter().chain(&a.normal).all(|x| x.is_finite()));
    }
    let windows: Vec<_> = all
        .vertices
        .iter()
        .filter(|v| v.surface[0] == SurfaceKind::Window as u32 as f32)
        .collect();
    assert!(!windows.is_empty());
    assert!(windows.iter().all(|v| v.position[1] < 0.1));
    assert!(
        windows.iter().all(|v| v.normal[1] < -0.99),
        "window faces point toward the road"
    );
    assert!(
        all.vertices.iter().any(|v| v.position[1] < 0.09),
        "stairs should extend towards the road"
    );
    assert!(
        all.vertices.iter().any(|v| v.position[2] > 0.07),
        "cornice should extend above the roof"
    );
}

#[test]
fn patterns_control_geometry_and_do_not_change_collision_queries() {
    let city = city();
    let collision_before = city.building_collision([0., 0.15], 0., 0., 0.07);
    let mut choices = config();
    let curved = city.building_detail_mesh(Region::WholeMap, &choices);
    choices.patterns[0].stair = StairKind::Straight;
    let straight = city.building_detail_mesh(Region::WholeMap, &choices);
    assert_ne!(
        curved
            .vertices
            .iter()
            .map(|v| v.position)
            .collect::<Vec<_>>(),
        straight
            .vertices
            .iter()
            .map(|v| v.position)
            .collect::<Vec<_>>()
    );
    choices.patterns[0].stair = StairKind::QuarterTurn;
    let turning = city.building_detail_mesh(Region::WholeMap, &choices);
    assert_ne!(
        turning
            .vertices
            .iter()
            .map(|v| v.position)
            .collect::<Vec<_>>(),
        straight
            .vertices
            .iter()
            .map(|v| v.position)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        city.building_collision([0., 0.15], 0., 0., 0.07),
        collision_before
    );
    assert!(
        !city.building_collision([0., 0.07], 0., 0., 0.07),
        "frontage details do not become colliders"
    );
    choices.patterns.clear();
    assert!(city
        .building_detail_mesh(Region::WholeMap, &choices)
        .vertices
        .is_empty());
    choices.unit = f32::NAN;
    assert!(city
        .building_detail_mesh(Region::WholeMap, &choices)
        .vertices
        .is_empty());
}

#[test]
fn roof_and_garden_choices_have_distinct_visual_geometry() {
    let city = city();
    let mut options = config();
    options.garden_chance = 0.;
    options.patterns[0].roof = RoofKind::Coping;
    let plain = city.building_detail_mesh(Region::WholeMap, &options);
    options.patterns[0].roof = RoofKind::Mansard;
    let mansard = city.building_detail_mesh(Region::WholeMap, &options);
    assert!(mansard
        .vertices
        .iter()
        .any(|v| v.surface[0] == SurfaceKind::Roof as u32 as f32 && v.surface[1] == options.unit));
    assert!(
        mansard
            .vertices
            .iter()
            .any(|v| v.surface[0] == SurfaceKind::Window as u32 as f32 && v.position[2] > 0.07),
        "mansard includes dormer windows"
    );
    assert_ne!(plain.vertices.len(), mansard.vertices.len());

    options.patterns[0].roof = RoofKind::Coping;
    options.garden_chance = 1.;
    options.garden_props = vec![(GardenProp::None, 1)];
    for edge in [
        GardenEdge::StoneWall,
        GardenEdge::BrickWall,
        GardenEdge::IronFence,
        GardenEdge::ConcreteEdging,
        GardenEdge::WireFence,
        GardenEdge::Hedge,
    ] {
        options.garden_edges = vec![(edge, 1)];
        let garden = city.building_detail_mesh(Region::WholeMap, &options);
        assert!(garden.vertices.len() > plain.vertices.len(), "{edge:?}");
    }
    options.garden_edges = vec![(GardenEdge::Open, 1)];
    for prop in [
        GardenProp::Planter,
        GardenProp::Bench,
        GardenProp::Bin,
        GardenProp::Bicycle,
    ] {
        options.garden_props = vec![(prop, 1)];
        let garden = city.building_detail_mesh(Region::WholeMap, &options);
        assert!(garden.vertices.len() > plain.vertices.len(), "{prop:?}");
    }
}

#[test]
fn sloped_roofs_do_not_overhang_irregular_footprints() {
    let source = "road street: sidewalk carlane sidewalk; end street(-1,0,1,0);\
        building(0.07, {floor_height: 0.035}, -0.04,0.1, 0.04,0.1, 0.07,0.16, 0.02,0.21, -0.04,0.21);";
    let city = City::from_program(&parse(source).unwrap(), None).unwrap();
    let mut options = config();
    options.patterns[0].roof = RoofKind::Mansard;
    options.garden_chance = 0.;
    let mesh = city.building_detail_mesh(Region::WholeMap, &options);
    assert!(!mesh.vertices.is_empty());
    assert!(mesh
        .vertices
        .iter()
        .all(|v| v.surface[0] != SurfaceKind::Roof as u32 as f32));
}

#[test]
fn stairs_fences_and_garden_furniture_are_separate_removable_objects() {
    let city = city();
    let mut choices = config();
    choices.garden_edges = vec![(GardenEdge::Open, 1)];
    choices.garden_props = vec![(GardenProp::None, 1)];
    let (facade, building_objects) = city.building_detail_parts(Region::WholeMap, &choices);
    assert_eq!(building_objects.len(), 2, "one stair and one balcony");
    assert!(building_objects
        .iter()
        .all(|object| !object.vertices.is_empty()));
    choices.garden_edges = vec![(GardenEdge::IronFence, 1)];
    choices.garden_props = vec![(GardenProp::Bench, 1)];
    let (with_garden, objects) = city.building_detail_parts(Region::WholeMap, &choices);
    assert_eq!(facade.vertices.len(), with_garden.vertices.len());
    assert!(
        objects.len() >= 4,
        "stair, balcony, fence sections and bench"
    );
    let combined = city.building_detail_mesh(Region::WholeMap, &choices);
    assert_eq!(
        combined.vertices.len(),
        with_garden.vertices.len() + objects.iter().map(|o| o.vertices.len()).sum::<usize>()
    );

    for edge in [
        GardenEdge::StoneWall,
        GardenEdge::BrickWall,
        GardenEdge::IronFence,
        GardenEdge::ConcreteEdging,
        GardenEdge::WireFence,
        GardenEdge::Hedge,
    ] {
        choices.garden_edges = vec![(edge, 1)];
        choices.garden_props = vec![(GardenProp::None, 1)];
        let (_, objects) = city.building_detail_parts(Region::WholeMap, &choices);
        assert!(
            objects.len() > building_objects.len(),
            "{edge:?} must have a removable boundary"
        );
    }
    choices.garden_edges = vec![(GardenEdge::Open, 1)];
    for prop in [
        GardenProp::Planter,
        GardenProp::Bench,
        GardenProp::Bin,
        GardenProp::Bicycle,
    ] {
        choices.garden_props = vec![(prop, 1)];
        let (_, objects) = city.building_detail_parts(Region::WholeMap, &choices);
        assert_eq!(
            objects.len(),
            building_objects.len() + 1,
            "{prop:?} must be separate"
        );
    }
    choices.garden_chance = 0.;
    choices.patterns[0].facade = FacadeKind::Cottage;
    let (_, gallery) = city.building_detail_parts(Region::WholeMap, &choices);
    assert_eq!(gallery.len(), 4, "canopy, two posts and stair are separate");
}
