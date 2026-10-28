use super::*;
use opencity::{BuildingMaterial, BuildingStyle};

#[test]
fn building_selection_controls_edit_and_dismiss_in_both_views() {
    for iso in [false, true] {
        let mut editor = editor();
        editor.program = parse(
            "elevation(0.12); office=building(0.24,{material:glass,\
                floor_height:0.04},-0.5,-0.5,0.5,-0.5,0.5,0.5,-0.5,0.5);",
        )
        .unwrap();
        editor.saved_program = editor.program.clone();
        editor.selected_segment = None;
        editor.selected_segments.clear();
        editor.camera_view = if iso {
            CameraView::Isometric
        } else {
            CameraView::Top
        };
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        let screen = editor.spatial_screen(
            editor.map_rect.unwrap(),
            WorldPoint { x: 0.0, y: 0.0 },
            0.36,
            iso,
        );
        click(&mut editor, &ctx, screen);
        assert_eq!(
            editor.selected_landscape.as_deref(),
            Some("office"),
            "iso={iso}"
        );
        let _ = frame(&mut editor, &ctx, vec![]);
        let output = frame(&mut editor, &ctx, vec![]);
        for label in ["Building properties", "Height", "Material", "Floor spacing"] {
            assert!(labels(&output).contains(&label.into()), "{label} iso={iso}");
        }
        // Double-click the height field, then type an actual numeric edit.
        let height_field = text_position(&output, "0.24");
        click(&mut editor, &ctx, height_field);
        click(&mut editor, &ctx, height_field);
        let _ = frame(
            &mut editor,
            &ctx,
            vec![
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                },
                egui::Event::Text("0.48".into()),
                key(egui::Key::Enter),
            ],
        );
        assert_eq!(
            project_landscape_features(&editor.program).features[0].height,
            Some(0.48)
        );
        let _ = frame(&mut editor, &ctx, vec![]);
        let output = frame(&mut editor, &ctx, vec![]);
        let spacing_field = text_position(&output, "0.040");
        click(&mut editor, &ctx, spacing_field);
        click(&mut editor, &ctx, spacing_field);
        let _ = frame(
            &mut editor,
            &ctx,
            vec![
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                },
                egui::Event::Text("0.06".into()),
                key(egui::Key::Enter),
            ],
        );
        assert_eq!(
            project_landscape_features(&editor.program).features[0]
                .building_style
                .unwrap()
                .floor_height,
            0.06
        );
        let _ = frame(&mut editor, &ctx, vec![]);
        let output = frame(&mut editor, &ctx, vec![]);
        click(&mut editor, &ctx, text_position(&output, "glass"));

        let _ = frame(&mut editor, &ctx, vec![]);
        let output = frame(&mut editor, &ctx, vec![]);
        click(&mut editor, &ctx, text_position(&output, "brick"));
        let feature = project_landscape_features(&editor.program)
            .features
            .remove(0);
        assert_eq!(
            feature.building_style.unwrap().material,
            BuildingMaterial::Brick,
            "iso={iso}"
        );
        assert_eq!(editor.undo_stack.len(), 3);
        let _ = frame(&mut editor, &ctx, vec![]);
        let _ = frame(&mut editor, &ctx, vec![key(egui::Key::Escape)]);
        assert!(editor.selected_landscape.is_none());
        assert!(editor.restore_history(true));
        assert!(editor.restore_history(true));
        assert!(editor.restore_history(true));
        assert_eq!(editor.program, editor.saved_program);
        let _ = frame(&mut editor, &ctx, vec![]);
        click(&mut editor, &ctx, screen);
        assert!(editor.selected_landscape.is_some());
        click(&mut editor, &ctx, egui::pos2(850.0, 550.0));
        assert!(editor.selected_landscape.is_none());
    }
}

#[test]
fn building_properties_preserve_model_footprint_elevation_and_roundtrip_undo() {
    let mut editor = editor();
    editor.program = parse(
        "elevation(0.12); office=building(0.24,\"office.glb\",-0.5,-0.5,\
            0.5,-0.5,0.5,0.5,-0.5,0.5);",
    )
    .unwrap();
    let before = editor.program.clone();
    let original = project_landscape_features(&before).features.remove(0);
    assert!(editor.edit_building_properties(
        "office",
        0.48,
        BuildingStyle {
            material: BuildingMaterial::Brick,
            floor_height: 0.06
        }
    ));
    editor.record_history(before.clone(), false);
    let feature = project_landscape_features(&editor.program)
        .features
        .remove(0);
    assert_eq!(feature.points, original.points);
    assert_eq!(feature.elevation, original.elevation);
    assert_eq!(feature.model, original.model);
    assert_eq!(feature.height, Some(0.48));
    assert_eq!(feature.building_style.unwrap().floor_height, 0.06);
    assert_eq!(
        parse(&editor.program.to_road_source()).unwrap(),
        editor.program
    );
    assert!(!editor.edit_building_properties("office", f32::NAN, BuildingStyle::default()));
    assert!(editor.restore_history(true));
    assert_eq!(editor.program, before);
}

#[test]
fn building_selection_reuses_scene_arc_and_does_not_spawn_build() {
    let mut editor = editor();
    editor.program = parse("office=building(1,-0.5,-0.5,0.5,-0.5,0.5,0.5,-0.5,0.5);").unwrap();
    let scene = Arc::new(scene3d::Scene::default());
    editor.scene_cache = Some(SceneCache {
        program: editor.program.clone(),
        theme: egui::Theme::Dark,
        scene: Arc::clone(&scene),
    });
    let ctx = egui::Context::default();
    let projection = map::project(&editor.program);
    for selected in [Some("office".into()), None, Some("office".into())] {
        editor.selected_landscape = selected;
        let cached = editor.scene_geometry(&ctx, editor_palette(egui::Theme::Dark), &projection);
        assert!(Arc::ptr_eq(&scene, &cached));
        assert!(editor.scene_build.is_none());
    }
}

#[test]
fn isometric_building_walls_and_occluded_roofs_are_pickable() {
    let mut editor = editor();
    editor.program = parse("front=building(2,-0.5,-0.5,0.5,-0.5,0.5,0.5,-0.5,0.5);").unwrap();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 650.0));
    let wall = editor.spatial_screen(rect, WorldPoint { x: 0.5, y: -0.5 }, 1.0, true);
    assert_eq!(
        editor.building_at(rect, wall, true).as_deref(),
        Some("front")
    );
    let roof = editor.spatial_screen(rect, WorldPoint { x: 0.0, y: 0.0 }, 2.0, true);
    assert_eq!(
        editor.building_at(rect, roof, true).as_deref(),
        Some("front")
    );
}

#[test]
fn stale_scene_completion_cannot_replace_current_geometry() {
    let mut editor = editor();
    let projection = map::project(&editor.program);
    let scene = Arc::new(scene3d::Scene::default());
    editor.scene_cache = Some(SceneCache {
        program: editor.program.clone(),
        theme: egui::Theme::Dark,
        scene: Arc::clone(&scene),
    });
    let (sender, receiver) = std::sync::mpsc::channel();
    sender
        .send(SceneCache {
            program: parse("old=building(1,0,0,1,0,1,1,0,1);").unwrap(),
            theme: egui::Theme::Dark,
            scene: Arc::new(scene3d::Scene::default()),
        })
        .unwrap();
    editor.scene_build = Some(receiver);
    let cached = editor.scene_geometry(
        &egui::Context::default(),
        editor_palette(egui::Theme::Dark),
        &projection,
    );
    assert!(Arc::ptr_eq(&scene, &cached));
    assert!(editor.scene_build.is_none());
}

#[test]
fn isometric_picking_prefers_visible_building_over_roof_hidden_behind_it() {
    let mut editor = editor();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 650.0));
    // Place a lower roof farther from the camera along the same screen ray.
    let t = -1.5 * editor.orbit_elevation.cos() / editor.orbit_elevation.sin();
    let x = t * editor.orbit_angle.sin();
    let y = -t * editor.orbit_angle.cos();
    editor.program = parse(&format!(
        "front=building(2,-0.5,-0.5,0.5,-0.5,0.5,0.5,-0.5,0.5); \
            back=building(0.5,{},{},{},{},{},{},{},{});",
        x - 0.4,
        y - 0.4,
        x + 0.4,
        y - 0.4,
        x + 0.4,
        y + 0.4,
        x - 0.4,
        y + 0.4
    ))
    .unwrap();
    let roof = editor.spatial_screen(rect, WorldPoint { x: 0.0, y: 0.0 }, 2.0, true);
    assert_eq!(
        editor.building_at(rect, roof, true).as_deref(),
        Some("front")
    );
}

#[test]
fn default_map_picking_properties_and_overlay_share_immutable_landscape_cache() {
    let mut editor = editor();
    editor.program = parse(include_str!("../../../../../examples/default_map.road")).unwrap();
    let landscape = editor.projected_landscape();
    assert_eq!(
        landscape
            .features
            .iter()
            .filter(|f| f.kind == LandscapeKind::Building)
            .count(),
        1863
    );
    let building = landscape
        .features
        .iter()
        .find(|f| f.kind == LandscapeKind::Building)
        .unwrap();
    let name = building.name.clone().unwrap();
    let center = WorldPoint {
        x: building.points.iter().map(|p| p[0]).sum::<f32>() / building.points.len() as f32,
        y: building.points.iter().map(|p| p[1]).sum::<f32>() / building.points.len() as f32,
    };
    editor.map_pan = egui::vec2(-center.x * editor.map_zoom, center.y * editor.map_zoom);
    let ctx = egui::Context::default();
    for selected in [None, Some(name.clone()), Some(name.clone()), None] {
        editor.selected_landscape = selected;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 650.0),
                )),
                ..Default::default()
            },
            |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
                assert_eq!(
                    editor.building_at(rect, rect.center(), false),
                    Some(name.clone())
                );
                editor.building_options_ui(ui, &response, rect, false);
                editor.paint_landscape_selection(
                    ui.painter(),
                    rect,
                    editor_palette(egui::Theme::Dark),
                    CameraView::Top,
                );
            },
        );
        output.textures_delta.clear();
        assert!(
            Arc::ptr_eq(&landscape, &editor.projected_landscape()),
            "selection and idle frames reuse the full default-map landscape projection"
        );
    }
    assert!(editor.edit_building_properties(&name, 0.48, BuildingStyle::default()));
    let edited = editor.projected_landscape();
    assert!(
        !Arc::ptr_eq(&landscape, &edited),
        "a source edit refreshes projected properties"
    );
    assert_eq!(
        edited
            .features
            .iter()
            .find(|f| f.name.as_ref() == Some(&name))
            .unwrap()
            .height,
        Some(0.48)
    );
    assert!(Arc::ptr_eq(&edited, &editor.projected_landscape()));
}
