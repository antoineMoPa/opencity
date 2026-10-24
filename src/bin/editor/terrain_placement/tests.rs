use super::*;
fn program(extra: &str) -> Program {
    parse(&format!("road street: carlane; end; {extra}")).unwrap()
}
fn free_road(program: &mut Program, start: f32, end: f32) -> usize {
    let Statement::Road(road) = program.statements[0].clone() else {
        unreachable!()
    };
    map::append_connected(
        program,
        &road,
        WorldPoint { x: start, y: 0.0 },
        None,
        WorldPoint { x: end, y: 0.0 },
        None,
    )
    .0
}
fn segment(program: &Program, index: usize) -> map::Segment {
    map::project(program)
        .segments
        .into_iter()
        .find(|s| s.source == SegmentSource::TopLevel(index))
        .unwrap()
}
fn editor(source: &str, camera: CameraView) -> RoadEditor {
    let mut e = super::super::tests::sample_editor();
    e.program = program(source);
    e.saved_program = e.program.clone();
    e.selected_road = Some(0);
    e.selected_segment = None;
    e.selected_segments.clear();
    e.camera_view = camera;
    e.map_zoom = 100.0;
    e.map_pan = egui::Vec2::ZERO;
    e.fit_scene_once = false;
    e
}
fn frame(e: &mut RoadEditor, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 650.0),
            )),
            events,
            ..Default::default()
        },
        |ui| e.editor_frame_ui(ui),
    );
    output.textures_delta.clear();
    output
}
fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}
fn click(e: &mut RoadEditor, ctx: &egui::Context, pos: egui::Pos2) {
    for pressed in [true, false] {
        frame(
            e,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}
fn palette_road(e: &mut RoadEditor, ctx: &egui::Context, query: &str) {
    e.command_palette.open();
    frame(e, ctx, vec![]);
    frame(e, ctx, vec![egui::Event::Text(query.into())]);
    frame(e, ctx, vec![key(egui::Key::Enter)]);
    frame(e, ctx, vec![]);
    assert!(e.map_tool == MapTool::Draw);
}
fn screen(e: &RoadEditor, p: WorldPoint, z: f32) -> egui::Pos2 {
    let rect = e.map_rect.unwrap();
    if e.camera_view == CameraView::Top {
        screen_of(rect, e.map_pan, e.map_zoom, p)
    } else {
        iso_screen(
            rect,
            e.map_pan,
            e.map_zoom,
            e.orbit_target,
            e.orbit_angle,
            e.orbit_elevation,
            p,
            z,
        )
    }
}
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.0002, "{a} != {b}");
}

#[test]
fn terrain_root_palette_top_and_iso_place_actual_xy_grade_roundtrip_and_undo() {
    for camera in [CameraView::Top, CameraView::Isometric] {
        for source in ["ridge=terrain(0.75,-4,-4,4,-4,4,4,-4,4);",
        "block plateau: terrain(0.25,-6,-7,2,-7,2,1,-6,1); end; terrain(0.4,-4,-4,4,-4,4,4,-4,4); high=plateau(2,3,0.2); elevation(high,0.3);"] {
        let mut e=editor(source,camera);
        let ctx=egui::Context::default(); frame(&mut e,&ctx,vec![]);
        palette_road(&mut e,&ctx,"Add road: street");
        let original=e.program.clone();
        let (a,b)=(WorldPoint{x:-1.2,y:0.2},WorldPoint{x:1.4,y:0.2});
        let (sa,sb)=(screen(&e,a,0.75),screen(&e,b,0.75));
        click(&mut e,&ctx,sa);
        close(e.map_start.unwrap().x,a.x);close(e.map_start.unwrap().y,a.y);
        assert_eq!(e.program,original,"first click must not change saved source");
        assert!(e.undo_stack.is_empty());
        if camera==CameraView::Isometric {
            let preview=e.terrain_draw_preview(&map::project(&e.program),e.map_rect.unwrap(),sb).unwrap();
            close(preview.start_z,0.75);close(preview.end.x,b.x);close(preview.end_z,0.75);
        }
        click(&mut e,&ctx,sb);
        let placed=map::project(&e.program).segments.pop().unwrap();
        close(placed.start.x,a.x);close(placed.start.y,a.y);close(placed.end.x,b.x);close(placed.end.y,b.y);
        close(placed.start_z,0.75);close(placed.end_z,0.75);
        assert_eq!(parse(&e.program.to_road_source()).unwrap(),e.program);
        assert_eq!(e.undo_stack.len(),1);
        assert!(e.restore_history(true));assert_eq!(e.program,original);
      }
    }
}

#[test]
fn terrain_midspan_and_edge_footprint_narrow_patches_raise_whole_grade() {
    for polygon in [
        "0.112,-0.01,0.113,-0.01,0.113,0.01,0.112,0.01",
        "0.112,0.001,0.113,0.001,0.113,0.002,0.112,0.002",
    ] {
        let mut p = program(&format!("terrain(0.8,{polygon});"));
        let i = free_road(&mut p, -1.0, 1.0);
        // Put the second tiny patch at the edge of the actual shared road
        // footprint, away from its centerline and all section row points.
        if polygon.contains("0.001") {
            let projection = map::project(&p);
            let (rows, _) = RoadGeometry::placement_cross_sections(&p, &projection, 0).unwrap();
            let edge = rows[0].boundaries.last().unwrap().point.y;
            let lo = edge * 0.8;
            let hi = edge * 0.9;
            p = program(&format!(
                "terrain(0.8,0.112,{lo},0.113,{lo},0.113,{hi},0.112,{hi});"
            ));
            free_road(&mut p, -1.0, 1.0);
        }
        apply(&mut p, SegmentSource::TopLevel(i), 0.0, 0.2, (false, false)).unwrap();
        let s = segment(&p, i);
        assert!(s.start_z >= 0.68, "{}", s.start_z);
        close(s.end_z - s.start_z, 0.2);
    }
}

#[test]
fn terrain_projected_blocks_overlap_and_iso_top_ray() {
    let p=program("block patch: terrain(0.4,-1,-1,1,-1,1,1,-1,1); end; hill=patch(2,3,0.2); elevation(hill,0.6); terrain(0.3,1,2,3,2,3,4,1,4);");
    let features = terrain(&p);
    assert_eq!(features.len(), 2);
    let top = features.iter().map(|f| f.elevation).fold(0.0, f32::max);
    close(height(&p, WorldPoint { x: 2.0, y: 3.0 }), top);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    let target = WorldPoint { x: 2.0, y: 3.0 };
    let pixel = iso_screen(
        rect,
        egui::Vec2::ZERO,
        100.0,
        target,
        0.8,
        ISO_ELEVATION,
        target,
        top,
    );
    let (picked, z) = iso_pick(&p, |h| {
        iso_world_at_height(
            rect,
            egui::Vec2::ZERO,
            100.0,
            target,
            0.8,
            ISO_ELEVATION,
            pixel,
            h,
        )
    });
    close(picked.x, 2.0);
    close(picked.y, 3.0);
    close(z, top);
}

#[test]
fn terrain_no_terrain_keeps_flat_source_and_invalid_height_rejects() {
    let mut p = program("");
    let i = free_road(&mut p, -1.0, 1.0);
    let before = p.clone();
    apply(&mut p, SegmentSource::TopLevel(i), 0.0, 0.0, (false, false)).unwrap();
    assert_eq!(p, before);
    assert!(apply(
        &mut p,
        SegmentSource::TopLevel(i),
        f32::NAN,
        0.0,
        (false, false)
    )
    .is_err());
    set_grade(&mut p, "road_1", 0.5, 0.5);
    let before = p.clone();
    apply(&mut p, SegmentSource::TopLevel(i), 0.0, 0.0, (true, false)).unwrap();
    assert_eq!(p, before);
}

#[test]
fn terrain_first_click_reference_cancel_leaves_authoritative_source_unchanged() {
    for camera in [CameraView::Top, CameraView::Isometric] {
        let mut e = editor("old=street(-1,0,1,0); grade(old,0.5,0.5);", camera);
        let ctx = egui::Context::default();
        frame(&mut e, &ctx, vec![]);
        palette_road(&mut e, &ctx, "Add road: street");
        let original = e.program.clone();
        let pos = screen(&e, WorldPoint { x: 1.0, y: 0.0 }, 0.5);
        click(&mut e, &ctx, pos);
        assert!(e.map_start_ref.is_some());
        assert_eq!(e.program, original);
        assert_eq!(parse(&e.program.to_road_source()).unwrap(), original);
        assert!(e.undo_stack.is_empty());
        frame(&mut e, &ctx, vec![key(egui::Key::Escape)]);
        assert_eq!(e.program, original);
        assert!(e.map_placement_draft.is_none());
        assert!(e.undo_stack.is_empty());
    }
}

#[test]
fn terrain_anchor_collision_rejection_is_atomic_visible_and_retryable() {
    for camera in [CameraView::Top, CameraView::Isometric] {
        let mut e=editor("old=street(-2,0,-1,0); grade(old,0.2,0.2); terrain(0.8,-0.1,-0.3,0.1,-0.3,0.1,0.3,-0.1,0.3);",camera);
        let ctx = egui::Context::default();
        frame(&mut e, &ctx, vec![]);
        palette_road(&mut e, &ctx, "Add road: street");
        let original = e.program.clone();
        let pos = screen(&e, WorldPoint { x: -1.0, y: 0.0 }, 0.2);
        click(&mut e, &ctx, pos);
        let reference = e.map_start_ref.clone().unwrap();
        let bad = screen(&e, WorldPoint { x: 1.0, y: 0.0 }, 0.0);
        click(&mut e, &ctx, bad);
        assert_eq!(e.program, original);
        assert_eq!(e.map_start_ref.as_deref(), Some(reference.as_str()));
        assert!(e
            .minimal
            .placement_error
            .as_deref()
            .unwrap()
            .contains("terrain"));
        assert!(e.undo_stack.is_empty());
        let safe = screen(&e, WorldPoint { x: -1.0, y: 1.5 }, 0.0);
        click(&mut e, &ctx, safe);
        assert_eq!(map::project(&e.program).segments.len(), 2);
        assert!(e.minimal.placement_error.is_none());
        let s = map::project(&e.program).segments.pop().unwrap();
        close(s.start_z, 0.2);
        assert_eq!(s.start_ref.as_deref(), Some(reference.as_str()));
        assert_eq!(e.undo_stack.len(), 1);
        assert!(e.restore_history(true));
        assert_eq!(e.program, original);
    }
}

#[test]
fn terrain_top_snapping_chooses_visible_deck_and_block_roads_stay_read_only() {
    let e=editor("low=street(-1,0,1,0); high=street(-1,0,1,0); grade(high,0.7,0.7); block b: inside=street(-1,2,1,2); end; b(0,0);",CameraView::Top);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 650.0));
    let projection = map::project(&e.program);
    let pointer = screen_of(rect, e.map_pan, e.map_zoom, WorldPoint { x: 1.0, y: 0.0 });
    let picked = e
        .draw_endpoint_near(&projection, rect, pointer, false)
        .unwrap();
    let s = projection
        .segments
        .iter()
        .find(|s| s.source == picked.0)
        .unwrap();
    close(s.end_z, 0.7);
    let block_pointer = screen_of(rect, e.map_pan, e.map_zoom, WorldPoint { x: 1.0, y: 2.0 });
    assert!(e
        .draw_endpoint_near(&projection, rect, block_pointer, false)
        .is_none());
}

#[test]
fn terrain_pending_profile_commits_with_road_in_one_undo_and_cancels_cleanly() {
    for query in ["Add highway", "Add interstate"] {
        let mut e = editor("terrain(0.6,-4,-4,4,-4,4,4,-4,4);", CameraView::Isometric);
        let ctx = egui::Context::default();
        frame(&mut e, &ctx, vec![]);
        palette_road(&mut e, &ctx, query);
        assert!(e.minimal.pending_road.is_some());
        let original = e.program.clone();
        let name = e.road().unwrap().name.clone();
        let a = screen(&e, WorldPoint { x: -1.0, y: 0.0 }, 0.6);
        let b = screen(&e, WorldPoint { x: 1.0, y: 0.0 }, 0.6);
        click(&mut e, &ctx, a);
        assert_eq!(e.program, original);
        frame(&mut e, &ctx, vec![key(egui::Key::Escape)]);
        assert_eq!(e.program, original);
        assert!(e.minimal.pending_road.is_none());
        palette_road(&mut e, &ctx, query);
        click(&mut e, &ctx, a);
        click(&mut e, &ctx, b);
        let projection = map::project(&e.program);
        let road = projection.segments.last().unwrap();
        assert_eq!(road.road_name, name);
        close(road.start_z, 0.6);
        close(road.end_z, 0.6);
        assert_eq!(
            e.program
                .statements
                .iter()
                .filter(|s| matches!(s,Statement::Road(r) if r.name==name))
                .count(),
            1
        );
        assert_eq!(e.undo_stack.len(), 1);
        assert!(e.restore_history(true));
        assert_eq!(e.program, original);
    }
}

#[test]
fn terrain_external_edit_during_pending_draft_is_preserved_on_commit_or_cancel() {
    for cancel in [false, true] {
        let mut e = editor(
            "old=street(-1,0,1,0); grade(old,0.5,0.5);",
            CameraView::Isometric,
        );
        let ctx = egui::Context::default();
        frame(&mut e, &ctx, vec![]);
        palette_road(&mut e, &ctx, "Add road: street");
        let a = screen(&e, WorldPoint { x: 1.0, y: 0.0 }, 0.5);
        click(&mut e, &ctx, a);
        e.program.statements.extend(
            parse("external=annotation_text(\"keep\",2,2);")
                .unwrap()
                .statements,
        );
        let changed = e.program.clone();
        if cancel {
            e.cancel_placement();
        } else {
            let b = screen(&e, WorldPoint { x: 2.0, y: 1.0 }, 0.0);
            click(&mut e, &ctx, b);
            assert!(e
                .minimal
                .placement_error
                .as_deref()
                .unwrap()
                .contains("Map changed"));
        }
        assert_eq!(e.program, changed);
        assert!(e.map_start.is_none());
        assert!(e.map_placement_draft.is_none());
    }
}

#[test]
fn terrain_clear_pending_draft_cancel_keeps_preview_confirm_never_restores_old_calls() {
    let mut e = editor(
        "old=street(-1,0,1,0); grade(old,0.5,0.5); terrain(0.5,-4,-4,4,-4,4,4,-4,4);",
        CameraView::Isometric,
    );
    let ctx = egui::Context::default();
    frame(&mut e, &ctx, vec![]);
    palette_road(&mut e, &ctx, "Add road: street");
    let original = e.program.clone();
    let a = screen(&e, WorldPoint { x: 1.0, y: 0.0 }, 0.5);
    click(&mut e, &ctx, a);
    let reference = e.map_start_ref.clone();
    e.file_action(FileAction::New);
    frame(&mut e, &ctx, vec![]);
    frame(&mut e, &ctx, vec![key(egui::Key::Escape)]);
    assert_eq!(e.program, original);
    assert_eq!(e.map_start_ref, reference);
    assert!(e.map_placement_draft.is_some());
    e.file_action(FileAction::New);
    frame(&mut e, &ctx, vec![]);
    let output = frame(&mut e, &ctx, vec![]);
    fn clear_position(shapes: &[egui::epaint::ClippedShape]) -> egui::Pos2 {
        fn find(shape: &egui::Shape) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.job.text == "Clear" => {
                    Some(t.pos + t.galley.rect.size() * 0.5)
                }
                egui::Shape::Vec(v) => v.iter().find_map(find),
                _ => None,
            }
        }
        shapes
            .iter()
            .find_map(|s| find(&s.shape))
            .expect("Clear button")
    }
    click(&mut e, &ctx, clear_position(&output.shapes));
    assert!(e
        .program
        .statements
        .iter()
        .all(|s| !matches!(s, Statement::Call(_))));
    assert!(e.map_placement_draft.is_none());
    assert!(e.map_placement_before.is_none());
    assert_eq!(e.undo_stack.len(), 1);
    assert!(e.restore_history(true));
    assert_eq!(e.program, original);
}
