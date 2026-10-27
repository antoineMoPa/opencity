use super::*;

#[test]
fn first_scene_build_returns_loading_scene_without_waiting() {
    let mut editor = sample_editor();
    editor.program = parse("road lane: carlane; end; lane(0,0,1,0);").unwrap();
    let ctx = egui::Context::default();
    let palette = editor_palette(egui::Theme::Dark);
    let projection = map::project(&editor.program);
    let loading = editor.scene_geometry(&ctx, palette, &projection);
    assert!(Arc::ptr_eq(&loading, &scene3d::loading_scene(palette)));
    assert!(editor.scene_cache.is_none());
    let completed = editor
        .scene_build
        .take()
        .unwrap()
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    assert_eq!(completed.program, editor.program);
    let rendered = Arc::clone(&completed.scene);
    editor.scene_cache = Some(completed);
    assert!(Arc::ptr_eq(
        &rendered,
        &editor.scene_geometry(&ctx, palette, &projection)
    ));
}

#[test]
fn design_analysis_does_not_wait_for_pending_work() {
    let mut editor = sample_editor();
    let ctx = egui::Context::default();
    let projection = map::project(&editor.program);
    let (sender, receiver) = std::sync::mpsc::channel();
    editor.design_analysis_job = Some(receiver);
    assert!(editor.design_issues(&ctx, &projection).is_empty());
    assert!(editor.design_analysis_cache.is_none());
    sender
        .send(DesignAnalysisCache {
            program: editor.program.clone(),
            issues: vec![DesignIssue {
                code: "test_issue",
                message: "finished".into(),
                at: WorldPoint { x: 0.0, y: 0.0 },
                roads: [String::new(), String::new()],
            }],
        })
        .unwrap();
    let issues = editor.design_issues(&ctx, &projection);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].code, "test_issue");
    assert!(editor.design_analysis_job.is_none());
}

#[test]
fn design_analysis_discards_old_results_and_checks_latest_document() {
    let mut editor = sample_editor();
    editor.program = parse("road lane: carlane; end; lane(0,0,1,0);").unwrap();
    let ctx = egui::Context::default();
    let projection = map::project(&editor.program);
    let (sender, receiver) = std::sync::mpsc::channel();
    editor.design_analysis_job = Some(receiver);
    sender
        .send(DesignAnalysisCache {
            program: Program::default(),
            issues: Vec::new(),
        })
        .unwrap();
    assert!(editor.design_issues(&ctx, &projection).is_empty());
    assert!(editor.design_analysis_cache.is_none());
    let result = editor
        .design_analysis_job
        .take()
        .unwrap()
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    assert_eq!(result.program, editor.program);
    assert!(result.issues.is_empty());
}

#[test]
fn placed_car_only_merge_mouth_receives_parent_profile() {
    let program = parse("place(cloverleaf, sample, 0, 0, 0);").unwrap();
    let projection = map::project(&program);
    let profiles = paint_profiles(&program, &projection);
    let mouth_index = projection
        .segments
        .iter()
        .position(|segment| segment.name.as_deref() == Some("sample_se_loop_c_merge"))
        .unwrap();
    let parent_index = projection
        .segments
        .iter()
        .position(|segment| segment.name.as_deref() == Some("sample_i_ns_02"))
        .unwrap();
    assert_eq!(profiles[mouth_index].merge_alignment, Some(0));
    assert!(profiles[mouth_index].end_parent.is_some());
    let order = road_paint_order(&program, &projection, &profiles);
    assert!(
        order
            .iter()
            .position(|index| *index == parent_index)
            .unwrap()
            < order
                .iter()
                .position(|index| *index == mouth_index)
                .unwrap()
    );
}

#[test]
#[ignore = "manual target map scene timing"]
fn profile_target_scene_selection_cache() {
    let mut editor = sample_editor();
    editor.program = starter_program();
    let projection = map::project(&editor.program);
    let palette = editor_palette(egui::Theme::Dark);
    let ctx = egui::Context::default();
    let first = editor.scene_geometry(&ctx, palette, &projection);
    editor.selected_segment = Some(projection.segments[0].source);
    let start = std::time::Instant::now();
    let second = editor.scene_geometry(&ctx, palette, &projection);
    eprintln!("selection scene lookup={:?}", start.elapsed());
    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn projection_cache_tracks_edits_across_zoom_frames() {
    let mut editor = sample_editor();
    editor.program = starter_program();
    let first = editor.projected_map();
    editor.map_zoom += 10.0;
    assert!(Arc::ptr_eq(&first, &editor.projected_map()));
    let segment = first
        .segments
        .iter()
        .find(|segment| segment.source.editable())
        .unwrap();
    let control = segment.controls[0];
    assert!(map::set_control(
        &mut editor.program,
        segment.source,
        false,
        WorldPoint {
            x: control.x + 0.5,
            y: control.y
        }
    ));
    let edited = editor.projected_map();
    assert!(!Arc::ptr_eq(&first, &edited));
    let changed = edited
        .segments
        .iter()
        .find(|candidate| candidate.source == segment.source)
        .unwrap();
    assert_ne!(segment.controls, changed.controls);
}

#[test]
#[ignore = "manual target map scroll timing"]
fn profile_target_zoom_ui_frames() {
    for (camera, base_zoom) in [
        (CameraView::Top, 45.0),
        (CameraView::Top, 1000.0),
        (CameraView::Isometric, 45.0),
        (CameraView::Isometric, 1000.0),
    ] {
        let mut editor = sample_editor();
        editor.program = starter_program();
        editor.camera_view = camera;
        let ctx = egui::Context::default();
        let mut frames = Vec::new();
        for frame in 0..24 {
            editor.map_zoom = base_zoom + frame as f32 * 2.0;
            let start = std::time::Instant::now();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| editor.map_canvas(ui));
            output.textures_delta.clear();
            if frame >= 4 {
                frames.push(start.elapsed());
            }
        }
        frames.sort();
        let label = match camera {
            CameraView::Top => "Top",
            CameraView::Isometric => "Isometric",
            CameraView::Vehicular => "Vehicular",
        };
        eprintln!(
            "{label} zoom UI at {base_zoom}: median={:?} p95={:?} max={:?}",
            frames[frames.len() / 2],
            frames[frames.len() * 95 / 100],
            frames.last().unwrap()
        );
    }
}

#[test]
#[ignore = "manual target map edit timing"]
fn profile_target_geometry_edit() {
    let mut editor = sample_editor();
    editor.program = starter_program();
    let first = map::project(&editor.program);
    let segment = first
        .segments
        .iter()
        .find(|segment| segment.source.editable())
        .unwrap();
    let control = segment.controls[0];
    let ctx = egui::Context::default();
    let _ = editor.design_issues(&ctx, &first);
    let previous_scene = editor.scene_geometry(&ctx, editor_palette(egui::Theme::Dark), &first);
    editor.dragging_control = Some((segment.source, false));
    let start = std::time::Instant::now();
    assert!(map::set_control(
        &mut editor.program,
        segment.source,
        false,
        WorldPoint {
            x: control.x + 0.01,
            y: control.y
        }
    ));
    let edit = start.elapsed();
    let projection = map::project(&editor.program);
    let projected = start.elapsed();
    let _issues = editor.design_issues(&ctx, &projection);
    let analysis = start.elapsed();
    let scene = editor.scene_geometry(&ctx, editor_palette(egui::Theme::Dark), &projection);
    assert!(Arc::ptr_eq(&previous_scene, &scene));
    eprintln!(
        "edit={:?} projection={:?} analysis={:?} scene={:?} total={:?}",
        edit,
        projected - edit,
        analysis - projected,
        start.elapsed() - analysis,
        start.elapsed()
    );
    #[cfg(not(target_arch = "wasm32"))]
    {
        let completed = editor.scene_build.take().unwrap().recv().unwrap();
        assert_eq!(completed.program, editor.program);
        eprintln!("background scene ready after {:?}", start.elapsed());
    }
}

#[test]
fn interaction_geometry_survives_selection_and_refreshes_after_edit_and_undo() {
    let mut editor = sample_editor();
    editor.program = parse("road street: carlane-sidewalk; end; main=street(0,0,2,0);").unwrap();
    let original = editor.program.clone();
    let projection = editor.projected_map();
    let geometry = editor.interaction_geometry();
    let source = projection.segments[0].source;
    let point = geometry.element_ends[0].position;
    editor.selected_segment = Some(source);
    editor.map_zoom *= 2.0;
    editor.camera_view = CameraView::Isometric;
    assert!(Arc::ptr_eq(&geometry, &editor.interaction_geometry()));
    assert!(map::set_control(
        &mut editor.program,
        source,
        false,
        WorldPoint { x: 0.5, y: 1.0 }
    ));
    let changed = editor.interaction_geometry();
    assert!(!Arc::ptr_eq(&geometry, &changed));
    assert_ne!(point, changed.element_ends[0].position);
    assert_eq!(changed.profiles.len(), projection.segments.len());
    editor.program = original;
    let restored = editor.interaction_geometry();
    assert!(!Arc::ptr_eq(&changed, &restored));
    assert_eq!(point, restored.element_ends[0].position);
}
