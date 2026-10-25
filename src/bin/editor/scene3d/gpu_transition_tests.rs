//! Readback checks exercise the optional projection uniform and shader path.
use super::*;

fn callback(
    scene: Arc<Scene>,
    editor: &RoadEditor,
    rect: egui::Rect,
    transition: Option<camera_transition::ClipMatrix>,
) -> SceneCallback {
    SceneCallback {
        time_seconds: 0.0,
        transition,
        scene,
        rect,
        pan: editor.map_pan,
        zoom: editor.map_zoom,
        target: editor.vehicle_position,
        angle: editor.vehicle_heading,
        elevation: editor.orbit_elevation,
        eye_height: editor.vehicle_eye_height,
        camera: editor.camera_view,
        background: Color32::BLACK,
        target_format: wgpu::TextureFormat::Bgra8Unorm,
    }
}

fn red_center(pixels: &[u8]) {
    let pixel = &pixels[(64 * 128 + 64) * 4..(64 * 128 + 64) * 4 + 4];
    assert!(
        pixel[2] > 180 && pixel[1] < 80 && pixel[0] < 80,
        "center pixel {pixel:?}"
    );
}

#[test]
fn transition_shader_keeps_far_origin_focus_visible_and_matches_vehicle_endpoint() {
    let mut editor = crate::tests::sample_editor();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(128.0, 128.0));
    editor.map_rect = Some(rect);
    editor.map_zoom = 40.0;
    editor.map_pan = egui::vec2(-4000.0, -2800.0);
    let vertices = [
        [99.5, -70.5, 0.0],
        [100.5, -70.5, 0.0],
        [100.5, -69.5, 0.0],
        [99.5, -70.5, 0.0],
        [100.5, -69.5, 0.0],
        [99.5, -69.5, 0.0],
    ]
    .into_iter()
    .map(|point| WorldVertex {
        point: WorldPoint {
            x: point[0],
            y: point[1],
        },
        height: point[2],
        depth_bias: 0.0,
        color: Color32::RED,
        surface: [0.0; 4],
    })
    .collect();
    let scene = Arc::new(Scene {
        vertices,
        ..Default::default()
    });
    let ctx = egui::Context::default();
    for destination in [CameraView::Isometric, CameraView::Top] {
        editor.set_camera_view(destination);
        for _ in 0..3 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    predicted_dt: 0.04,
                    ..Default::default()
                },
                |ui| editor.advance_camera_transition(ui.ctx()),
            );
            output.textures_delta.clear();
        }
        let matrix = editor
            .camera_transition_matrix(rect)
            .expect("mid-transition projection");
        let Some(pixels) =
            render_test_callback(callback(Arc::clone(&scene), &editor, rect, Some(matrix)))
        else {
            return;
        };
        red_center(&pixels);
    }
    editor.camera_transition = None;
    editor.camera_view = CameraView::Vehicular;
    editor.vehicle_position = WorldPoint { x: 100.0, y: -70.0 };
    editor.vehicle_heading = 0.0;
    editor.vehicle_eye_height = 0.127;
    let vertices = [
        [101.0, -70.5, 0.0],
        [101.0, -69.5, 0.0],
        [101.0, -69.5, 0.5],
        [101.0, -70.5, 0.0],
        [101.0, -69.5, 0.5],
        [101.0, -70.5, 0.5],
    ]
    .into_iter()
    .map(|point| WorldVertex {
        point: WorldPoint {
            x: point[0],
            y: point[1],
        },
        height: point[2],
        depth_bias: 0.005,
        color: Color32::RED,
        surface: [0.0; 4],
    })
    .collect();
    let scene = Arc::new(Scene {
        vertices,
        ..Default::default()
    });
    let Some(legacy) = render_test_callback(callback(Arc::clone(&scene), &editor, rect, None))
    else {
        return;
    };
    let Some(endpoint) = render_test_callback(callback(
        scene,
        &editor,
        rect,
        Some(editor.camera_clip_matrix(rect)),
    )) else {
        return;
    };
    red_center(&endpoint);
    let changed = legacy
        .chunks_exact(4)
        .zip(endpoint.chunks_exact(4))
        .filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 5))
        .count();
    assert!(
        changed < 20,
        "perspective endpoint changed {changed} pixels"
    );
}
