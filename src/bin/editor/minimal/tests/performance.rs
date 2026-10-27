use super::*;

fn focus(editor: &mut RoadEditor, point: WorldPoint) {
    editor.map_zoom = 90.0;
    editor.orbit_target = point;
    editor.map_pan = if editor.camera_view == CameraView::Top {
        egui::vec2(-point.x * editor.map_zoom, point.y * editor.map_zoom)
    } else {
        egui::Vec2::ZERO
    };
    editor.selected_segment = None;
    editor.selected_segments.clear();
    editor.selected_landscape = None;
    editor.point_options = None;
    editor.minimal.network_options = None;
}

fn timed_frame(
    editor: &mut RoadEditor,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    label: &str,
) -> egui::FullOutput {
    let start = std::time::Instant::now();
    let output = frame(editor, ctx, events);
    eprintln!("{label}: {:?}", start.elapsed());
    output
}

/// Exercise actual startup geography and first-click menus. Keeping the derived
/// geometry Arcs proves that opening popups does not rebuild document caches.
#[test]
fn default_map_first_click_menus_reuse_document_geometry() {
    let mut editor = editor();
    editor.program = boot_program();
    editor.saved_program = editor.program.clone();
    let projection = editor.projected_map();
    assert_eq!(projection.segments.len(), 592);
    assert_eq!(map::intersections(&editor.program, &projection).len(), 268);
    let landscape = editor.projected_landscape();
    assert_eq!(
        landscape
            .features
            .iter()
            .filter(|f| f.kind == LandscapeKind::Building)
            .count(),
        1863
    );
    let interaction = editor.interaction_geometry();
    let scene = scene3d::loading_scene(editor_palette(egui::Theme::Dark));
    editor.scene_cache = Some(SceneCache {
        program: editor.program.clone(),
        theme: egui::Theme::Dark,
        scene: Arc::clone(&scene),
    });
    let segment = projection
        .segments
        .iter()
        .find(|s| {
            s.source.editable()
                && s.start_ref
                    .as_deref()
                    .is_none_or(|name| !map::anchored_reference(&editor.program, name))
        })
        .unwrap();
    let building = landscape
        .features
        .iter()
        .find(|f| f.kind == LandscapeKind::Building && f.name.is_some())
        .unwrap();
    let building_center = WorldPoint {
        x: building.points.iter().map(|p| p[0]).sum::<f32>() / building.points.len() as f32,
        y: building.points.iter().map(|p| p[1]).sum::<f32>() / building.points.len() as f32,
    };
    for camera in [CameraView::Top, CameraView::Isometric] {
        editor.camera_view = camera;
        let label = if camera == CameraView::Top {
            "Top"
        } else {
            "Isometric"
        };
        focus(&mut editor, segment.start);
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        let p = editor.spatial_screen(
            editor.map_rect.unwrap(),
            segment.start,
            segment.start_z,
            camera == CameraView::Isometric,
        );
        let _ = timed_frame(
            &mut editor,
            &ctx,
            vec![egui::Event::PointerMoved(p), pointer(p, true)],
            &format!("{label} first road press"),
        );
        let _ = timed_frame(
            &mut editor,
            &ctx,
            vec![pointer(p, false)],
            &format!("{label} first road release"),
        );
        let _ = frame(&mut editor, &ctx, vec![]);
        let output = timed_frame(&mut editor, &ctx, vec![], &format!("{label} road menu"));
        assert!(
            editor.point_options.is_some(),
            "{label}: {:?}",
            labels(&output)
        );
        assert!(labels(&output).contains(&"Edit cross-section".into()));

        focus(&mut editor, building_center);
        let ctx = egui::Context::default();
        let _ = frame(&mut editor, &ctx, vec![]);
        let p = editor.spatial_screen(
            editor.map_rect.unwrap(),
            building_center,
            building.elevation + building.height.unwrap_or(0.0),
            camera == CameraView::Isometric,
        );
        let _ = timed_frame(
            &mut editor,
            &ctx,
            vec![egui::Event::PointerMoved(p), pointer(p, true)],
            &format!("{label} first building press"),
        );
        let _ = timed_frame(
            &mut editor,
            &ctx,
            vec![pointer(p, false)],
            &format!("{label} first building release"),
        );
        let _ = frame(&mut editor, &ctx, vec![]);
        let output = timed_frame(&mut editor, &ctx, vec![], &format!("{label} building menu"));
        assert!(
            editor.selected_landscape.is_some(),
            "{label}: {:?}",
            labels(&output)
        );
        assert!(labels(&output).contains(&"Building properties".into()));
        assert!(Arc::ptr_eq(&projection, &editor.projected_map()));
        assert!(Arc::ptr_eq(&interaction, &editor.interaction_geometry()));
        assert!(Arc::ptr_eq(&landscape, &editor.projected_landscape()));
        assert!(Arc::ptr_eq(
            &scene,
            &editor.scene_cache.as_ref().unwrap().scene
        ));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        editor.launch_action = Ok(Some(crate::external_launch::LaunchAction::parse(
            r#"{"label":"Inspect here","program":"never-run-during-menu-test","args":["{map}","{x},{y}"]}"#,
            std::env::temp_dir(),
        ).unwrap()));
    }
    editor.camera_view = CameraView::Top;
    focus(&mut editor, segment.start);
    let ctx = egui::Context::default();
    let _ = frame(&mut editor, &ctx, vec![]);
    let p = editor.map_rect.unwrap().center();
    let button = |pressed| egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let _ = timed_frame(
        &mut editor,
        &ctx,
        vec![egui::Event::PointerMoved(p), button(true)],
        "Top first context press",
    );
    let _ = timed_frame(
        &mut editor,
        &ctx,
        vec![button(false)],
        "Top first context release",
    );
    let output = timed_frame(&mut editor, &ctx, vec![], "Top context menu");
    assert!(labels(&output).contains(&"Copy coordinates".into()));
    #[cfg(not(target_arch = "wasm32"))]
    assert!(labels(&output).contains(&"Inspect here".into()));
    assert!(Arc::ptr_eq(&projection, &editor.projected_map()));
    assert!(Arc::ptr_eq(&interaction, &editor.interaction_geometry()));
    assert!(editor.undo_stack.is_empty());
    assert_eq!(editor.program, editor.saved_program);
}
