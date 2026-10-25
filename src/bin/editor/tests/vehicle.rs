use super::*;

#[test]
fn vehicular_w_moves_without_deadlocking_or_rebuilding_the_scene() {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut editor = sample_editor();
        editor.program = parse("road lane: carlane; end; lane(0,0,1,0);").unwrap();
        let program = editor.program.clone();
        editor.camera_view = CameraView::Vehicular;
        editor.fit_scene_once = false;
        editor.vehicle_heading = 0.0;
        let original = editor.vehicle_position;
        let scene = Arc::new(scene3d::Scene::default());
        editor.scene_cache = Some(SceneCache {
            program: program.clone(),
            theme: egui::Theme::Dark,
            scene: Arc::clone(&scene),
        });
        let ctx = egui::Context::default();
        ctx.set_theme(egui::ThemePreference::Dark);
        // Hover hit testing uses the preceding frame's widget rectangles.
        let mut warmup = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 400.0),
                )),
                events: vec![egui::Event::PointerMoved(egui::pos2(200.0, 200.0))],
                ..Default::default()
            },
            |ui| editor.vehicular_canvas(ui),
        );
        warmup.textures_delta.clear();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 400.0),
                )),
                events: vec![
                    egui::Event::PointerMoved(egui::pos2(200.0, 200.0)),
                    egui::Event::Key {
                        key: egui::Key::W,
                        physical_key: Some(egui::Key::W),
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| editor.vehicular_canvas(ui),
        );
        output.textures_delta.clear();
        sender
            .send((
                editor.vehicle_position.x > original.x,
                editor.program == program,
                editor.scene_build.is_none()
                    && Arc::ptr_eq(&editor.scene_cache.as_ref().unwrap().scene, &scene),
            ))
            .unwrap();
    });
    assert_eq!(
        receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("W must return without waiting on egui's input lock"),
        (true, true, true)
    );
}

#[test]
fn vehicular_camera_starts_on_selected_road_and_falls_back_to_nearest() {
    let mut editor = sample_editor();
    editor.program = parse("road lane: carlane; end; lane(0,0,1,0);").unwrap();
    let projection = map::project(&editor.program);
    let segment = &projection.segments[0];
    editor.selected_segment = Some(segment.source);
    editor.place_vehicle_camera();
    let middle = segment.point_at(0.5);
    assert!((editor.vehicle_position.x - middle.x).abs() < 0.0001);
    assert!((editor.vehicle_position.y - middle.y).abs() < 0.0001);
    assert!((editor.vehicle_eye_height - segment.height_at(0.5) - 0.027).abs() < 0.0001);
    editor.set_camera_view(CameraView::Top);
    editor.selected_segment = None;
    editor.orbit_target = middle;
    editor.place_vehicle_camera();
    assert!((editor.vehicle_position.x - middle.x).abs() < 0.0001);
    assert!((editor.vehicle_position.y - middle.y).abs() < 0.0001);
}

#[test]
fn vehicular_camera_uses_a_car_lane_and_its_travel_direction() {
    let mut editor = sample_editor();
    editor.program =
        parse("road divided: carlane-middle-carlane; end; divided(0,0,10,0);").unwrap();
    editor.place_vehicle_camera();
    let width = map::element_world_width(RoadElementKind::CarLane);
    let median = map::element_world_width(RoadElementKind::Middle);
    assert!((editor.vehicle_position.x - 5.0).abs() < 0.0001);
    assert!((editor.vehicle_position.y + (width + median) / 2.0).abs() < 0.0001);
    assert!((editor.vehicle_heading - std::f32::consts::PI).abs() < 0.0001);
    assert!((editor.vehicle_eye_height - 0.027).abs() < 0.0001);
}

#[test]
fn vehicular_height_follows_only_the_selected_graded_road() {
    let mut editor = sample_editor();
    editor.program =
        parse("road lane: carlane; end; lane(0,0,1,0); lane(0.75,-1,0.75,1);").unwrap();
    let mut projection = map::project(&editor.program);
    projection.segments[0].start_z = 0.1;
    projection.segments[0].end_z = 0.5;
    projection.segments[1].start_z = 2.0;
    projection.segments[1].end_z = 2.0;
    editor.selected_segment = Some(projection.segments[0].source);
    editor.vehicle_position = WorldPoint { x: 0.75, y: 0.0 };
    editor.follow_selected_road_height(&projection);
    assert!(
        (editor.vehicle_eye_height - 0.427).abs() < 0.001,
        "crossing road must not change the selected level"
    );
    editor.vehicle_position = WorldPoint { x: 0.75, y: 0.2 };
    let height = editor.vehicle_eye_height;
    editor.follow_selected_road_height(&projection);
    assert_eq!(
        editor.vehicle_eye_height, height,
        "leaving selected road keeps current height"
    );
}

#[test]
fn vehicular_arrows_turn_in_opposite_directions_at_a_time_based_rate() {
    for (key, sign) in [(egui::Key::ArrowLeft, -1.0), (egui::Key::ArrowRight, 1.0)] {
        for dt in [1.0 / 60.0, 1.0 / 30.0] {
            let mut editor = sample_editor();
            editor.program = parse("road lane: carlane; end; lane(0,0,1,0);").unwrap();
            editor.fit_scene_once = false;
            editor.vehicle_heading = 0.0;
            let position = editor.vehicle_position;
            let ctx = egui::Context::default();
            let input = |events| egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 400.0),
                )),
                predicted_dt: dt,
                events,
                ..Default::default()
            };
            let mut frame = ctx.run_ui(
                input(vec![egui::Event::PointerMoved(egui::pos2(200.0, 200.0))]),
                |ui| editor.vehicular_canvas(ui),
            );
            frame.textures_delta.clear();
            let mut frame = ctx.run_ui(
                input(vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]),
                |ui| editor.vehicular_canvas(ui),
            );
            frame.textures_delta.clear();
            assert!((editor.vehicle_heading - sign * 1.5 * dt).abs() < 0.0001);
            editor.camera_view = CameraView::Vehicular;
            let rect = editor.map_rect.unwrap();
            let ahead = WorldPoint {
                x: position.x + 2.0,
                y: position.y,
            };
            let projected = editor.annotation_screen(rect, ahead).unwrap();
            match key {
                egui::Key::ArrowLeft => assert!(
                    projected.x > rect.center().x,
                    "stationary scenery moves right when looking left"
                ),
                egui::Key::ArrowRight => assert!(
                    projected.x < rect.center().x,
                    "stationary scenery moves left when looking right"
                ),
                _ => unreachable!(),
            }
            assert_eq!(editor.vehicle_position, position);
            let heading = editor.vehicle_heading;
            let mut text = String::new();
            let mut frame = ctx.run_ui(input(vec![]), |ui| {
                ui.text_edit_singleline(&mut text).request_focus();
                editor.vehicular_canvas(ui);
            });
            frame.textures_delta.clear();
            assert_eq!(
                editor.vehicle_heading, heading,
                "text entry keeps arrow keys"
            );
        }
    }
}
