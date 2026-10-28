#![cfg(feature = "game-runtime")]
use opencity::{
    parse,
    runtime::{City, SurfaceKind},
};

fn city(source: &str) -> City {
    City::from_program(&parse(source).unwrap(), None).unwrap()
}

#[test]
fn elevated_roads_have_paved_shoulders_and_height_limited_barriers() {
    for height in [0.08, -0.08] {
        let source = format!(
            "road r: verge carlane verge; end s=r(0,0,1,0); elevation(s,\
                {height}); ground = land(-1,-1,2,-1,2,1,-1,1); elevation(ground,\
                {});",
            height - 0.08
        );
        // Use a plain bridge for the positive case and an authored lower land for the negative
        // case.
        let source = if height > 0. {
            "road r: verge carlane verge; end s=r(0,0,1,0); elevation(s,0.08);".to_owned()
        } else {
            source
        };
        let bridge = city(&source);
        assert!(!bridge.road_barriers.is_empty());
        for wall in &bridge.road_barriers {
            let p = [
                (wall.start.point.x + wall.end.point.x) * 0.5,
                (wall.start.point.y + wall.end.point.y) * 0.5,
            ];
            assert!(bridge
                .road_barrier_contact(p, 0.003, height, height + 0.005)
                .is_some());
            assert!(bridge
                .road_barrier_contact(p, 0.003, height - 0.04, height - 0.02)
                .is_none());
        }
        assert_eq!(
            bridge.surface_at([0.5, 0.04], height + 0.005).unwrap().kind,
            SurfaceKind::Asphalt
        );
    }
}

#[test]
fn ground_highways_and_ramps_are_protected_and_ordinary_verges_stay_grass() {
    for role in ["highway", "ramp", "merge"] {
        let city = city(&format!(
            "road r: verge carlane verge; role {role}; end r(0,0,1,0);"
        ));
        assert!(!city.road_barriers.is_empty());
        assert_eq!(
            city.surface_at([0.5, 0.04], 0.01).unwrap().kind,
            SurfaceKind::Asphalt
        );
    }
    for ground in ["", "land(-1,-1,2,-1,2,1,-1,1);"] {
        let ordinary = city(&format!(
            "road r: verge carlane verge; end r(0,0,1,0); {ground}"
        ));
        assert!(ordinary.road_barriers.is_empty());
        assert_eq!(
            ordinary.surface_at([0.5, 0.04], 0.01).unwrap().kind,
            SurfaceKind::Grass
        );
        let on_land =
            city("road r: verge carlane verge; end r(0,0,1,0); land(-1,-1,2,-1,2,1,-1,1);");
        assert!(on_land.road_barriers.is_empty());
    }
}

#[test]
fn highway_role_survives_write_and_parse() {
    let program = parse("road r: carlane; role highway; end;").unwrap();
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
}

#[test]
fn raised_terrain_is_not_a_bridge() {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1024, 1024);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![0; 1024 * 1024])
            .unwrap();
    }
    let program = parse(
        "elevation_map(\"terrain.png\",-1,-1,3,2,0.1,0.2); road r: verge \
            carlane verge; end s=r(0,0,1,0); elevation(s,0.1);",
    )
    .unwrap();
    let city = City::from_program_with_elevation_loader(&program, |spec| {
        opencity::heightmap::ElevationMap::from_png(spec.clone(), &bytes)
    })
    .unwrap();
    assert!(city.road_barriers.is_empty());
    assert_eq!(
        city.surface_at([0.5, 0.04], 0.11).unwrap().kind,
        SurfaceKind::Grass
    );
}

#[test]
fn interchange_mouths_remain_open_in_each_orientation() {
    for turn in 0..4 {
        let program = parse(&format!(
            "place(street_to_interstate_elevated, sample, 5, -3, {turn});"
        ))
        .unwrap();
        let projection = opencity::map::project(&program);
        let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
        let city = City::from_program(&program, None).unwrap();
        for (id, segment) in projection.segments.iter().enumerate() {
            let road = opencity::road_profiles::road_for_segment(&program, segment).unwrap();
            if road.role != opencity::RoadRole::Merge {
                continue;
            }
            for row in geometry.cross_sections(id) {
                let a = row.boundaries[0];
                let b = *row.boundaries.last().unwrap();
                let p = [(a.point.x + b.point.x) * 0.5, (a.point.y + b.point.y) * 0.5];
                assert!(
                    city.road_barrier_contact(p, 0.0092, a.height + 0.003, a.height + 0.01)
                        .is_none(),
                    "mouth blocked at {p:?}, turn {turn}"
                );
            }
        }
    }
}

#[test]
fn elevated_intersection_barriers_join_approaches_and_leave_mouths_open() {
    for three_way in [false, true] {
        let mut source = include_str!("../examples/default_intersection.road").replace(
            "sidewalk carlane carlane sidewalk",
            "verge sidewalk carlane middle carlane sidewalk verge",
        );
        if three_way {
            source = source
                .lines()
                .filter(|line| !line.starts_with("south_approach"))
                .collect::<Vec<_>>()
                .join("\n")
                .replace(
                    "intersection4(center, street);",
                    "intersection3(center, street);",
                );
        }
        for name in ["north_approach", "east_approach", "west_approach"] {
            source.push_str(&format!("\nelevation({name},0.08);"));
        }
        if !three_way {
            source.push_str("\nelevation(south_approach,0.08);");
        }
        let program = parse(&source).unwrap();
        let projection = opencity::map::project(&program);
        let mut geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
        geometry.configure_safety(&program, &projection, None);
        assert!(geometry.junction_is_protected("center"));
        let city = City::from_program(&program, None).unwrap();
        for id in 0..city.roads.len() {
            let rows = geometry.cross_sections(id);
            let row = rows
                .iter()
                .min_by(|a, b| {
                    a.boundaries[0]
                        .point
                        .x
                        .hypot(a.boundaries[0].point.y)
                        .total_cmp(&b.boundaries[0].point.x.hypot(b.boundaries[0].point.y))
                })
                .unwrap();
            for vertex in [row.boundaries[0], *row.boundaries.last().unwrap()] {
                let count = geometry
                    .safety_walls
                    .iter()
                    .filter(|w| {
                        [w.start, w.end].iter().any(|v| {
                            (v.point.x - vertex.point.x).hypot(v.point.y - vertex.point.y) < 1e-5
                        })
                    })
                    .count();
                assert!(
                    count >= 2,
                    "open barrier at {:?}, three_way={three_way}, count={count}",
                    vertex.point
                );
            }
            let a = row.boundaries[0];
            let b = *row.boundaries.last().unwrap();
            let mouth = [(a.point.x + b.point.x) * 0.5, (a.point.y + b.point.y) * 0.5];
            assert!(
                city.road_barrier_contact(mouth, 0.0092, 0.083, 0.09)
                    .is_none(),
                "blocked junction mouth {mouth:?}"
            );
        }
        let mesh = city
            .mesh(opencity::runtime::Region::WholeMap, 0.04)
            .unwrap();
        assert!(
            !mesh
                .vertices
                .iter()
                .any(|v| v.surface[0] == SurfaceKind::Grass as u32 as f32 && v.position[2] > 0.04),
            "grass on elevated junction"
        );
    }
}

#[test]
fn real_diamond_bridge_intersection_has_connected_corner_barriers() {
    let city = City::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/default_map.road"
    ))
    .unwrap();
    let corner = [-26.1, 8.82];
    let nearby: Vec<_> = city
        .road_barriers
        .iter()
        .filter(|wall| {
            [wall.start, wall.end]
                .iter()
                .any(|v| (v.point.x - corner[0]).hypot(v.point.y - corner[1]) < 0.4)
        })
        .collect();
    assert!(!nearby.is_empty());
    for wall in &nearby {
        for end in [wall.start, wall.end] {
            if (end.point.x - corner[0]).hypot(end.point.y - corner[1]) > 0.32 {
                continue;
            }
            let count = nearby
                .iter()
                .filter(|w| {
                    [w.start, w.end].iter().any(|v| {
                        (v.point.x - end.point.x).hypot(v.point.y - end.point.y) < 1e-5
                            && (v.height - end.height).abs() < 1e-4
                    })
                })
                .count();
            assert!(count >= 2, "disconnected diamond bridge barrier at {end:?}");
        }
    }
    let mesh = city
        .mesh(
            opencity::runtime::Region::radius(corner, 0.32).unwrap(),
            0.04,
        )
        .unwrap();
    assert!(
        !mesh
            .vertices
            .iter()
            .any(|v| v.surface[0] == SurfaceKind::Grass as u32 as f32 && v.position[2] > 0.15),
        "elevated grass at the diamond bridge junction"
    );
}

#[test]
fn ramp_mouth_shoulders_are_paved_and_clear_for_vehicle_width() {
    for source in [
        "place(cloverleaf, c, 0, 0, 0);",
        "place(street_to_interstate_flat, c, 0, 0, 0);",
        "place(street_to_interstate_elevated, c, 0, 0, 0);",
        include_str!("../examples/default_map.road"),
    ] {
        let program = parse(source).unwrap();
        let projection = opencity::map::project(&program);
        let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
        let city = City::from_program(
            &program,
            Some(std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/examples"
            ))),
        )
        .unwrap();
        let mut mouths = 0;
        for (id, segment) in projection.segments.iter().enumerate() {
            let road = opencity::road_profiles::road_for_segment(&program, segment).unwrap();
            if road.role != opencity::RoadRole::Merge {
                continue;
            }
            mouths += 1;
            for row in geometry.cross_sections(id) {
                let a = row.boundaries[0];
                let b = *row.boundaries.last().unwrap();
                let width = (a.point.x - b.point.x).hypot(a.point.y - b.point.y);
                let expected =
                    opencity::map::element_world_width(opencity::RoadElementKind::CarLane)
                        + 2. * opencity::map::element_world_width(opencity::RoadElementKind::Verge);
                assert!(
                    (width - expected).abs() < 0.00001,
                    "{:?}: shoulders pinch at t={}",
                    segment.name,
                    row.t
                );
                // Both shoulders and the lane must admit a vehicle, rather than
                // merely opening the barrier along the centerline.
                for fraction in [0.2, 0.5, 0.8] {
                    let p = [
                        a.point.x + (b.point.x - a.point.x) * fraction,
                        a.point.y + (b.point.y - a.point.y) * fraction,
                    ];
                    let contact =
                        city.road_barrier_contact(p, 0.0092, a.height + 0.003, a.height + 0.01);
                    assert!(
                        contact.is_none(),
                        "{:?}: blocked shoulder at t={}, fraction={fraction}, point={p:?}, \
                            contact={contact:?}",
                        segment.name,
                        row.t
                    );
                    // An entry on a ground street may also overlap its existing sidewalk.
                    assert!(
                        matches!(
                            city.surface_at(p, a.height + 0.005).unwrap().kind,
                            SurfaceKind::Asphalt | SurfaceKind::Sidewalk
                        ),
                        "{:?}: unpaved shoulder at t={}",
                        segment.name,
                        row.t
                    );
                }
            }
        }
        assert!(mouths > 0);
    }
}
