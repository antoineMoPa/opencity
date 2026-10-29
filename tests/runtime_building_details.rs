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
        building(0.07, {material: brick, floor_height: 0.035}, -0.04,0.1, \
        0.04,0.1, 0.04,0.21, -0.04,0.21);";

    City::from_program(&parse(source).unwrap(), None).unwrap()
}

#[test]
fn entrance_paving_is_one_supported_quad_aligned_to_ground_door() {
    for angle in [0_f32, 0.7, 1.57, 3.14] {
        let (sin, cos) = angle.sin_cos();
        let point = |x: f32, y: f32| (cos * x - sin * y, sin * x + cos * y);
        let road = [point(-1., 0.), point(1., 0.)];
        let corners = [
            point(-0.04, 0.08),
            point(0.04, 0.08),
            point(0.04, 0.19),
            point(-0.04, 0.19),
        ];
        let source = format!(
            "road street: sidewalk carlane sidewalk; end street({},{},{},{}); \
            ground=land(-2,-2,2,-2,2,2,-2,2); elevation(ground,0.02); \
            home=building(0.07,{{floor_height:0.035,front_edge:0}},{}); elevation(home,0.002);",
            road[0].0,
            road[0].1,
            road[1].0,
            road[1].1,
            corners
                .iter()
                .map(|p| format!("{},{}", p.0, p.1))
                .collect::<Vec<_>>()
                .join(",")
        );
        let city = City::from_program(&parse(&source).unwrap(), None).unwrap();
        let paths = |mesh: opencity::runtime::Mesh| {
            mesh.vertices
                .into_iter()
                .filter(|v| {
                    v.surface[0] == SurfaceKind::Sidewalk as u32 as f32 && v.surface[3] == 1.
                })
                .collect::<Vec<_>>()
        };
        let (mesh, props) = city.building_detail_parts(Region::WholeMap, &config());
        let path = paths(mesh);
        assert_eq!(path.len(), 6, "one quad at angle {angle}");
        assert!(
            props.into_iter().all(|p| paths(p).is_empty()),
            "path is static"
        );
        let feature = city.query(Region::WholeMap).buildings[0].1;
        let frame = city.building_frontage(feature, 0.5).unwrap();
        let expected = frame.point(frame.width * 0.46 + 0.0095 * 0.5, 0., 0.00015);
        let start = [
            (path[0].position[0] + path[1].position[0]) * 0.5,
            (path[0].position[1] + path[1].position[1]) * 0.5,
        ];
        assert!((start[0] - expected[0]).hypot(start[1] - expected[1]) < 1e-6);
        let end = path[2].position;
        assert_eq!(
            city.surface_at([end[0], end[1]], 0.01).unwrap().kind,
            SurfaceKind::Sidewalk
        );
        assert!(path.iter().all(|v| v.normal[2] > 0.99));
        assert!(
            (path[0].position[2] - 0.00015).abs() < 1e-6,
            "paving starts at ground, not the raised home base"
        );
        assert!(
            (end[2] - 0.00165).abs() < 1e-6,
            "paving meets the raised sidewalk"
        );
        for replacement in ["lake(-2,-2,2,-2,2,2,-2,2)", "concrete(-2,-2,2,-2,2,2,-2,2)"] {
            let blocked = source.replace("land(-2,-2,2,-2,2,2,-2,2)", replacement);
            let city = City::from_program(&parse(&blocked).unwrap(), None).unwrap();
            assert!(paths(city.building_detail_mesh(Region::WholeMap, &config())).is_empty());
        }
    }
}

#[test]
fn room_panes_keep_uvs_aspect_and_tangent_through_facade_rotation() {
    for (dx, dy) in [(1., 0.), (0., 1.), (0.6, 0.8), (-1., 0.)] {
        let point = |x: f32, y: f32| (x * dx - y * dy, x * dy + y * dx);
        let a = point(-1., 0.);
        let b = point(1., 0.);
        let corners = [
            point(-0.04, 0.1),
            point(0.04, 0.1),
            point(0.04, 0.21),
            point(-0.04, 0.21),
        ];
        let source = format!(
            "road street: sidewalk carlane sidewalk; end street({},{},{},{}); \
                building(0.07, {{floor_height: 0.035}}, {});",
            a.0,
            a.1,
            b.0,
            b.1,
            corners
                .iter()
                .map(|p| format!("{},{}", p.0, p.1))
                .collect::<Vec<_>>()
                .join(",")
        );
        let city = City::from_program(&parse(&source).unwrap(), None).unwrap();
        let mesh = city.building_detail_mesh(Region::WholeMap, &config());
        let panes: Vec<_> = mesh
            .vertices
            .chunks_exact(6)
            .filter(|p| {
                p[0].surface[0] == SurfaceKind::Window as u32 as f32 && p[0].surface[3] >= 16.
            })
            .collect();
        assert!(!panes.is_empty());
        for pane in panes {
            let n = pane[0].normal;
            let u_axis = [n[1], -n[0]];
            let packed = pane[0].surface[3];
            let aspect = packed - (packed / 16.).floor() * 16.;
            assert!((0.125..=15.).contains(&aspect));
            for v in pane {
                assert_eq!(v.surface[3], packed);
                assert!((0. ..=1.).contains(&v.surface[1]));
                assert!((0. ..=1.).contains(&v.surface[2]));
            }
            // The two triangles share a room; U increases in the shader's tangent direction.
            for pair in pane.windows(2) {
                let du = pair[1].surface[1] - pair[0].surface[1];
                let distance = (pair[1].position[0] - pair[0].position[0]) * u_axis[0]
                    + (pair[1].position[1] - pair[0].position[1]) * u_axis[1];
                if du.abs() > 0.5 {
                    assert!(du * distance > 0.);
                }
            }
        }
    }
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
            assert_eq!(
                panels.len(),
                4,
                "two front sections and two clear side returns"
            );
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
fn attached_gardens_share_one_divider_and_keep_each_fence_section_removable() {
    let city = City::from_program(
        &parse(
            "road street: sidewalk carlane sidewalk; end street(-1,0,1,0); \
                building(0.07, {floor_height: 0.035}, 0,0.1, 0.08,0.1, 0.08,0.21, \
                0,0.21); building(0.07, {floor_height: 0.035}, 0.08,0.1, 0.16,\
                0.1, 0.16,0.21, 0.08,0.21);",
        )
        .unwrap(),
        None,
    )
    .unwrap();
    let mut choices = config();
    choices.garden_props = vec![(GardenProp::None, 1)];
    let (_, objects) = city.building_detail_parts(Region::WholeMap, &choices);
    let fences: Vec<_> = objects
        .iter()
        .filter(|object| {
            object
                .vertices
                .iter()
                .all(|v| v.surface[0] == SurfaceKind::Fence as u32 as f32)
        })
        .collect();
    assert_eq!(
        fences.len(),
        7,
        "four front sections and three lot dividers"
    );
    assert!(fences.iter().all(|fence| fence.vertices.len() == 6));
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
        building(0.07, {floor_height: 0.035}, -0.04,0.1, 0.04,0.1, 0.07,\
        0.16, 0.02,0.21, -0.04,0.21);";

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

#[test]
fn commercial_and_mixed_use_shells_never_receive_residential_details() {
    for (keyword, eligible) in [
        ("unspecified", true),
        ("residential", true),
        ("commercial", false),
        ("mixed_use", false),
    ] {
        let source = format!(
            "road street: sidewalk carlane sidewalk; end street(-1,0,1,0);\
            b=building(0.07,{{use:{keyword},floor_height:0.035}},\
            -0.04,0.1,0.04,0.1,0.04,0.21,-0.04,0.21);"
        );
        let city = City::from_program(&parse(&source).unwrap(), None).unwrap();
        let (details, props) = city.building_detail_parts(Region::WholeMap, &config());
        assert_eq!(!details.vertices.is_empty(), eligible, "{keyword}");
        if !eligible {
            assert!(props.is_empty());
        }
    }
}

#[test]
fn explicit_facade_edge_overrides_nearest_road_and_respects_winding() {
    for (points, expected) in [
        ("0,0,1,0,1,1,0,1", [0.0, 1.0]),
        ("0,0,0,1,1,1,1,0", [1.0, 0.0]),
        // Concave footprint, where a centroid-based outside test can be wrong.
        ("0,0,3,0,3,3,2,3,2,1,1,1,1,3,0,3", [0.0, 1.0]),
    ] {
        let source = format!(
            "road street: carlane; end street(-1,-0.1,4,-0.1);\
            b=building(0.07,{{use:commercial,front_edge:2}}, {points});"
        );
        let city = City::from_program(&parse(&source).unwrap(), None).unwrap();
        let building = city.query(Region::WholeMap).buildings[0].1;
        let frame = city.building_frontage(building, 0.5).unwrap();
        assert_eq!(frame.origin, building.points[2]);
        assert_eq!(frame.outward, expected);
        assert_eq!(frame.base, building.elevation);
        assert!(city.building_frontage(building, f32::NAN).is_none());
    }
}
