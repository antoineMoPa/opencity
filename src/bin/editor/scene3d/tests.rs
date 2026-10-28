//! Scene geometry and incremental assembly regressions.
use super::*;

#[test]
fn car_lane_only_road_has_no_extra_edge_bands() {
    let program = parse("road street: carlane carlane; end street(0,0,1,0);").unwrap();
    let projection = map::project(&program);
    let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
    let profiles = paint_profiles(&program, &projection);
    let road = road_for_segment(&program, &projection.segments[0]).unwrap();
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let palette = editor_palette(theme);
        for selected in [false, true] {
            let mut scene = Scene::default();
            add_resolved_road_surface(
                &mut scene,
                &geometry,
                0,
                road,
                &profiles[0],
                palette,
                selected,
            );
            assert!(!scene.vertices.is_empty());
            assert!(scene.vertices.iter().all(|v| {
                v.color == kind_color(RoadElementKind::CarLane, theme)
                    || v.color == palette.map_centerline
                    || (selected && v.color == palette.map_selected)
            }));
            assert_eq!(
                scene
                    .vertices
                    .iter()
                    .any(|v| v.color == palette.map_selected),
                selected,
            );
        }
    }
}

#[test]
fn library_junction_rendered_edges_meet_road_pavement_in_every_orientation() {
    let palette = editor_palette(egui::Theme::Dark);
    for asset in ["three_way", "four_way"] {
        for rotation in 0..4 {
            for plateau in [0.0, 0.4] {
                let program = parse(&format!("place({asset}, sample, 0, 0, {rotation});")).unwrap();
                let mut projection = map::project(&program);
                assert_eq!(projection.unresolved, 0);
                for segment in &mut projection.segments {
                    segment.start_z = plateau;
                    segment.end_z = plateau;
                }
                let geometry =
                    opencity::road_geometry::RoadGeometry::compile(&program, &projection);
                let intersection = map::intersections(&program, &projection).remove(0);
                let mut junction_scene = Scene::default();
                append_intersection(
                    &mut junction_scene,
                    &egui::Context::default(),
                    &program,
                    &projection,
                    &geometry,
                    &intersection,
                    palette,
                );
                assert!(!junction_scene.vertices.is_empty());
                for vertex in &junction_scene.vertices {
                    assert!(
                        (vertex.height - plateau - PAVEMENT_OFFSET).abs() < 1e-6,
                        "{asset}/{rotation}: junction left its approach plane"
                    );
                }
                let full = build_scene(&program, &projection, palette, &[], None);
                let mut incremental =
                    WebSceneBuild::new(program.clone(), projection.clone(), palette, None);
                while !incremental.advance(std::time::Duration::from_secs(1)) {}
                for port in &intersection.ports {
                    let (index, is_end) = projection
                        .segments
                        .iter()
                        .enumerate()
                        .find_map(|(i, segment)| {
                            if segment.start_ref.as_deref() == Some(&port.point_ref) {
                                Some((i, false))
                            } else if segment.end_ref.as_deref() == Some(&port.point_ref) {
                                Some((i, true))
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    let rows = geometry.cross_sections(index);
                    let row = if is_end {
                        rows.last().unwrap()
                    } else {
                        &rows[0]
                    };
                    for boundary in [
                        row.boundaries.first().unwrap(),
                        row.boundaries.last().unwrap(),
                    ] {
                        for scene in [&junction_scene, &full, &incremental.scene] {
                            assert!(
                                scene.vertices.iter().any(|vertex| {
                                    (vertex.point.x - boundary.point.x).abs() < 2e-5
                                        && (vertex.point.y - boundary.point.y).abs() < 2e-5
                                        && (vertex.height - boundary.height - PAVEMENT_OFFSET).abs()
                                            < 1e-6
                                }),
                                "{asset}/{rotation}: missing matched rendered port at {:?}",
                                boundary.point
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn tee_rendered_approach_bands_continue_without_steps_in_every_orientation() {
    let palette = editor_palette(egui::Theme::Dark);
    for rotation in 0..4 {
        let program = parse(&format!("place(three_way, sample, 0, 0, {rotation});")).unwrap();
        let projection = map::project(&program);
        let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
        let intersection = map::intersections(&program, &projection).remove(0);
        let mut scene = Scene::default();
        append_intersection(
            &mut scene,
            &egui::Context::default(),
            &program,
            &projection,
            &geometry,
            &intersection,
            palette,
        );
        let color_at = |point: WorldPoint| {
            let mut result = None;
            let mut nearest_bias = f32::INFINITY;
            for triangle in scene.vertices.chunks_exact(3) {
                let signs: Vec<_> = (0..3)
                    .map(|i| {
                        let a = triangle[i].point;
                        let b = triangle[(i + 1) % 3].point;
                        (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x)
                    })
                    .collect();
                if (signs.iter().all(|s| *s >= -1e-9) || signs.iter().all(|s| *s <= 1e-9))
                    && triangle[0].depth_bias <= nearest_bias
                {
                    result = Some(triangle[0].color);
                    nearest_bias = triangle[0].depth_bias;
                }
            }
            result
        };
        for port in &intersection.ports {
            let (index, is_end) = projection
                .segments
                .iter()
                .enumerate()
                .find_map(|(i, segment)| {
                    if segment.start_ref.as_deref() == Some(&port.point_ref) {
                        Some((i, false))
                    } else if segment.end_ref.as_deref() == Some(&port.point_ref) {
                        Some((i, true))
                    } else {
                        None
                    }
                })
                .unwrap();
            let segment = &projection.segments[index];
            let road = program
                .statements
                .iter()
                .find_map(|statement| match statement {
                    Statement::Road(road) if road.name == segment.road_name => Some(road),
                    _ => None,
                })
                .unwrap();
            let rows = geometry.cross_sections(index);
            let row = if is_end {
                rows.last().unwrap()
            } else {
                &rows[0]
            };
            for (band, element) in road.elements.iter().enumerate() {
                if element.kind == RoadElementKind::Middle {
                    continue;
                }
                let a = row.boundaries[band].point;
                let b = row.boundaries[band + 1].point;
                let point = WorldPoint {
                    x: (a.x + b.x) * 0.5 - port.direction.x * 0.0002,
                    y: (a.y + b.y) * 0.5 - port.direction.y * 0.0002,
                };
                assert_eq!(
                    color_at(point),
                    Some(kind_color(element.kind, palette.theme)),
                    "rotation {rotation}, port {}, band {band}",
                    port.point_ref
                );
            }
        }
        assert!(
            scene
                .vertices
                .iter()
                .all(|vertex| vertex.point.x.abs() <= 0.12152 && vertex.point.y.abs() <= 0.12152),
            "outline protrudes beyond pavement"
        );
    }
}

#[test]
fn junction_height_preserves_approach_edges_and_detached_ground_plane() {
    let seams = [
        (
            WorldPoint { x: -1.0, y: -1.0 },
            WorldPoint { x: -1.0, y: 1.0 },
            0.2,
        ),
        (
            WorldPoint { x: 1.0, y: -1.0 },
            WorldPoint { x: 1.0, y: 1.0 },
            0.4,
        ),
    ];
    for y in [-1.0, 0.0, 1.0] {
        assert!(
            (intersection_surface_height(WorldPoint { x: -1.0, y }, &seams) - 0.205).abs() < 1e-6
        );
        assert!(
            (intersection_surface_height(WorldPoint { x: 1.0, y }, &seams) - 0.405).abs() < 1e-6
        );
    }
    assert!(
        (intersection_surface_height(WorldPoint { x: 0.0, y: 0.0 }, &seams) - 0.305).abs() < 1e-6
    );
    assert_eq!(
        intersection_surface_height(WorldPoint { x: 0.0, y: 0.0 }, &[]),
        PAVEMENT_OFFSET
    );
}

#[test]
fn captured_map_edges_have_no_world_space_feather() {
    let mut scene = Scene::default();
    capture_geometry(
        &egui::Context::default(),
        |painter| {
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(-50.0, -50.0), egui::pos2(50.0, 50.0)),
                0.0,
                Color32::WHITE,
            );
        },
        |_| 0.0,
        &mut scene,
    );
    assert!(!scene.vertices.is_empty());
    assert!(scene.vertices.iter().all(|vertex| vertex.color.a() == 255));
}

#[test]
fn close_zoom_culls_roads_outside_the_camera() {
    let program = starter_program();
    let projection = map::project(&program);
    let scene = Arc::new(build_scene(
        &program,
        &projection,
        editor_palette(egui::Theme::Dark),
        &[],
        None,
    ));
    let first = &scene.road_ranges[0];
    let center = WorldPoint {
        x: (first.min.x + first.max.x) * 0.5,
        y: (first.min.y + first.max.y) * 0.5,
    };
    let mut callback = SceneCallback {
        time_seconds: 0.0,
        transition: None,
        scene: Arc::clone(&scene),
        rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(512.0, 512.0)),
        pan: egui::vec2(-center.x * 1000.0, center.y * 1000.0),
        zoom: 1000.0,
        target: WorldPoint { x: 0.0, y: 0.0 },
        angle: 0.0,
        elevation: ISO_ELEVATION,
        eye_height: 0.0,
        camera: CameraView::Top,
        background: Color32::BLACK,
        target_format: wgpu::TextureFormat::Bgra8Unorm,
    };
    let visible = scene
        .road_ranges
        .iter()
        .filter(|road| callback.road_is_visible(road))
        .count();
    assert!(visible > 0);
    assert!(visible < scene.road_ranges.len());
    let span = scene.road_span.as_ref().unwrap();
    assert_eq!(
        scene.road_ranges.first().unwrap().vertices.start,
        span.start
    );
    assert_eq!(scene.road_ranges.last().unwrap().vertices.end, span.end);
    callback.camera = CameraView::Isometric;
    callback.target = center;
    callback.pan = egui::Vec2::ZERO;
    callback.angle = 0.5;
    let visible_isometric = scene
        .road_ranges
        .iter()
        .filter(|road| callback.road_is_visible(road))
        .count();
    assert!(visible_isometric > 0);
    assert!(visible_isometric < scene.road_ranges.len());
}

#[test]
fn connector_wall_tops_are_actual_pavement_vertices() {
    let program = parse(include_str!("../../../../road_library/cloverleaf.road")).unwrap();
    let projection = map::project(&program);
    let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
    let profiles = paint_profiles(&program, &projection);
    let index = projection
        .segments
        .iter()
        .position(|s| s.name.as_deref() == Some("ne_connector_down"))
        .unwrap();
    let road = road_for_segment(&program, &projection.segments[index]).unwrap();
    let mut surface = Scene::default();
    add_resolved_road_surface(
        &mut surface,
        &geometry,
        index,
        road,
        &profiles[index],
        editor_palette(egui::Theme::Dark),
        false,
    );
    let mut walls = Scene::default();
    add_elevated_edges(&mut walls, &geometry, index, Color32::WHITE);
    assert!(!walls.vertices.is_empty());
    for face in walls.vertices.chunks_exact(6) {
        for top in [&face[0], &face[1]] {
            assert!(
                surface
                    .vertices
                    .iter()
                    .any(|v| v.point == top.point && v.height == top.height),
                "wall top must meet the emitted pavement without height \
                    interpolation or offsets"
            );
        }
    }
}

#[test]
fn elevated_library_walls_use_shared_geometry() {
    let program = parse(include_str!(
        "../../../../road_library/street_to_interstate_elevated.road"
    ))
    .unwrap();
    let projection = map::project(&program);
    let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
    let mut scene = Scene::default();
    for index in 0..projection.segments.len() {
        add_elevated_edges(&mut scene, &geometry, index, Color32::WHITE);
    }
    assert_eq!(scene.vertices.len(), geometry.walls.len() * 6);
    assert!(
        geometry.obstructions().is_empty(),
        "{:?}",
        geometry.obstructions()
    );
}

#[test]
fn coastal_scene_keeps_water_free_of_editor_grid() {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let palette = editor_palette(theme);
        assert_eq!(palette.map_background, Color32::from_rgb(7, 112, 172));
        let program = parse("land(0, 0, 2, 0, 2, 2, 0, 2);").unwrap();
        let projection = map::project(&program);
        let scene = build_scene(&program, &projection, palette, &[], None);
        assert!(!scene
            .vertices
            .iter()
            .any(|vertex| vertex.depth_bias == 100.0));
    }
}

#[test]
fn overlapping_ground_surfaces_have_stable_depth_order() {
    let program = parse(
        "beach(0, 0, 3, 0, 3, 3, 0, 3); land(0, 0, 3, 0, 3, 3, 0, 3); \
            vegetation(forest, 0, 0, 3, 0, 3, 3, 0, 3); concrete(0, 0, 3, 0, \
            3, 3, 0, 3);",
    )
    .unwrap();
    let palette = editor_palette(egui::Theme::Dark);
    let scene = build_scene(&program, &map::project(&program), palette, &[], None);
    let fill_bias = |color| {
        scene
            .vertices
            .iter()
            .find(|vertex| vertex.color == color)
            .expect("landscape fill exists")
            .depth_bias
    };
    let beach = fill_bias(Color32::from_rgb(244, 219, 159));
    let land = fill_bias(Color32::from_rgb(105, 158, 75));
    let vegetation = fill_bias(Color32::from_rgb(61, 128, 59));
    let concrete = fill_bias(Color32::from_rgb(125, 139, 145));
    assert!(beach > land && land > vegetation && vegetation > concrete);
    assert!(beach - land >= 0.01 && land - vegetation >= 0.01);
    assert!(concrete > 0.0, "roads at zero bias stay above ground cover");
}

#[test]
fn overlapping_terrain_outlines_expose_both_mountain_levels() {
    let program = parse(
        "lower = terrain(0.2, 0, 0, 2, 0, 2, 2, 0, 2); upper = \
            terrain(0.4, 1, 1, 3, 1, 3, 3, 1, 3);",
    )
    .unwrap();
    let features = project_landscape_features(&program).features;
    let mut scene = Scene::default();
    add_terrain_walls(
        &mut scene,
        &features[1],
        &features,
        editor_palette(egui::Theme::Dark),
    );
    assert!(scene.vertices.iter().any(|vertex| vertex.height == 0.0));
    assert!(scene.vertices.iter().any(|vertex| vertex.height == 0.2));
    assert!(scene.vertices.iter().any(|vertex| vertex.height == 0.4));
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn referenced_glb_replaces_building_extrusion_in_scene() {
    let source =
        opencity::parse("building(0.5, {material:brick}, 0, 0, 1, 0, 1, 1, 0, 1);").unwrap();
    let bytes = glb::export(&source);
    let directory = std::env::temp_dir().join(format!("opencity-glb-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("test.glb"), bytes).unwrap();
    let program = parse("hall = building(0.8, \"test.glb\", 2, 2, 4, 2, 4, 4, 2, 4);").unwrap();
    let projection = map::project(&program);
    let scene = build_scene_with_model_base(
        &program,
        &projection,
        editor_palette(egui::Theme::Dark),
        &[],
        None,
        Some(&directory),
    );
    assert!(scene
        .vertices
        .iter()
        .any(|vertex| vertex.color.to_array()[..3]
            .iter()
            .zip([154, 143, 133])
            .all(|(channel, expected)| (*channel as i32 - expected).abs() <= 1)
            && vertex.point.x >= 2.0
            && vertex.point.x <= 4.0
            && vertex.height > 0.0
            && vertex.height <= 0.80001));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn incremental_web_scene_matches_full_scene() {
    let program = boot_program();
    let projection = map::project(&program);
    let palette = editor_palette(egui::Theme::Dark);
    let full = build_scene(&program, &projection, palette, &[], None);
    let mut incremental = WebSceneBuild::new(program, projection, palette, None);
    assert_eq!(incremental.theme, egui::Theme::Dark);
    assert_eq!(incremental.selected_landscape, None);
    let mut passes = 0;
    let started = std::time::Instant::now();
    while !incremental.advance(std::time::Duration::from_millis(1)) {
        passes += 1;
        assert!(
            started.elapsed() < std::time::Duration::from_secs(120),
            "scene builder did not finish after {passes} slices"
        );
    }
    let incremental = incremental.finish();
    assert_eq!(incremental.vertices, full.vertices);
    assert_eq!(incremental.gpu_vertices().len(), full.gpu_vertices().len());
    assert_eq!(incremental.road_span, full.road_span);
    assert_eq!(incremental.road_ranges.len(), full.road_ranges.len());
    for (built, expected) in incremental.road_ranges.iter().zip(&full.road_ranges) {
        assert_eq!(built.vertices, expected.vertices);
        assert_eq!((built.min, built.max), (expected.min, expected.max));
        assert_eq!(
            (built.min_height, built.max_height),
            (expected.min_height, expected.max_height)
        );
    }
}

#[test]
fn building_roofs_meet_their_wall_tops() {
    let program =
        parse("tower=building(0.4,0,0,0.2,0,0.2,0.2,0,0.2); elevation(tower,0.1);").unwrap();
    let scene = build_scene(
        &program,
        &map::project(&program),
        editor_palette(egui::Theme::Dark),
        &[],
        None,
    );
    let roof: Vec<_> = scene
        .vertices
        .iter()
        .filter(|v| v.color == Color32::from_rgb(192, 198, 194))
        .collect();
    assert!(roof.len() >= 3);
    assert!(
        roof.iter().all(|v| (v.height - 0.5).abs() < 0.00001),
        "roof must meet the 0.5 wall tops instead of inheriting the ground-surface offset"
    );
}

#[test]
#[ignore = "manual browser rebuild budget timing"]
fn profile_web_scene_slices() {
    let program = boot_program();
    let projection = map::project(&program);
    let palette = editor_palette(egui::Theme::Dark);
    let _ = grid_vertices(palette);
    let start = std::time::Instant::now();
    let mut builder = WebSceneBuild::new(program, projection, palette, None);
    let setup = start.elapsed();
    let mut slices = Vec::new();
    loop {
        let start = std::time::Instant::now();
        let complete = builder.advance(std::time::Duration::from_millis(6));
        slices.push(start.elapsed());
        if complete {
            break;
        }
    }
    eprintln!(
        "web scene setup={setup:?}, slices={slices:?}, vertices={}",
        builder.scene.vertices.len()
    );
}

#[test]
#[ignore = "manual CPU scene timing"]
fn profile_target_scene_cpu() {
    let program = boot_program();
    let projection = map::project(&program);
    eprintln!(
        "straight segments={}/{}",
        projection
            .segments
            .iter()
            .filter(|segment| segment_is_straight(segment))
            .count(),
        projection.segments.len()
    );
    let bins = [0.01, 0.05, 0.1, 0.25, 0.5, 1.0, f32::INFINITY];
    eprintln!(
        "curve deviation bins={:?}",
        bins.map(|limit| projection
            .segments
            .iter()
            .filter(|segment| {
                let dx = segment.end.x - segment.start.x;
                let dy = segment.end.y - segment.start.y;
                let length = (dx * dx + dy * dy).sqrt().max(0.00001);
                segment
                    .controls
                    .iter()
                    .map(|point| {
                        ((point.x - segment.start.x) * dy - (point.y - segment.start.y) * dx).abs()
                            / length
                    })
                    .fold(0.0, f32::max)
                    <= limit
            })
            .count())
    );
    let start = std::time::Instant::now();
    let scene = build_scene(
        &program,
        &projection,
        editor_palette(egui::Theme::Dark),
        &[],
        None,
    );
    eprintln!(
        "scene build={:?} vertices={}",
        start.elapsed(),
        scene.vertices.len()
    );
    let start = std::time::Instant::now();
    let scene = build_scene(
        &program,
        &projection,
        editor_palette(egui::Theme::Dark),
        &[],
        None,
    );
    eprintln!(
        "warm scene build={:?} vertices={}",
        start.elapsed(),
        scene.vertices.len()
    );
}

#[test]
fn airport_surfaces_and_markings_reach_shared_scene() {
    let source = "road strip: runway; end; road connector: taxiway; end; road \
        stand: apron; end; strip(0, 0, 3, 0); connector(0, 1, 3, 1); \
        stand(0, 2, 3, 2);";
    let program = parse(source).unwrap();
    assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    let scene = build_scene(
        &program,
        &map::project(&program),
        editor_palette(egui::Theme::Dark),
        &[],
        None,
    );
    for color in [
        kind_color(RoadElementKind::Runway, egui::Theme::Dark),
        kind_color(RoadElementKind::Taxiway, egui::Theme::Dark),
        kind_color(RoadElementKind::Apron, egui::Theme::Dark),
        Color32::from_rgb(235, 236, 224),
        Color32::from_rgb(233, 191, 73),
    ] {
        assert!(
            scene.vertices.iter().any(|vertex| {
                vertex.color.r() == color.r()
                    && vertex.color.g() == color.g()
                    && vertex.color.b() == color.b()
            }),
            "missing {color:?}"
        );
    }
}

#[test]
fn shared_scene_contains_detailed_starter_roads_landscape_and_junctions() {
    let program = starter_program();
    let projection = map::project(&program);
    let palette = editor_palette(egui::Theme::Dark);
    let scene = build_scene(&program, &projection, palette, &[], None);
    assert!(
        scene.vertices.len() > 80_000,
        "{} scene vertices",
        scene.vertices.len()
    );
    assert!(scene
        .vertices
        .iter()
        .any(|vertex| vertex.color == kind_color(RoadElementKind::Sidewalk, palette.theme)));
    assert!(scene.vertices.iter().any(|vertex| vertex.height > 0.1));
    assert!(scene.vertices.iter().any(|vertex| vertex.height < -0.1));
    for intersection in map::intersections(&program, &projection) {
        let mut junction_scene = Scene::default();
        let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
        append_intersection(
            &mut junction_scene,
            &egui::Context::default(),
            &program,
            &projection,
            &geometry,
            &intersection,
            palette,
        );
        assert!(!junction_scene.vertices.is_empty());
        assert!(
            junction_scene
                .vertices
                .iter()
                .all(|vertex| scene.vertices.contains(vertex)),
            "missing intersection {}",
            intersection.point_ref
        );
    }
}
