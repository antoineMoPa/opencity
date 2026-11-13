//! Native readbacks confirm the editor uses the same contact masks as the game.
use super::*;

#[test]
#[ignore = "requires a native GPU adapter"]
fn native_editor_contact_texture_on_straight_curved_and_junction_pavement() {
    let program = parse(include_str!(
        "../../../../tests/fixtures/contact_debris.road"
    ))
    .unwrap();
    let scene = Arc::new(build_scene(
        &program,
        &map::project(&program),
        editor_palette(egui::Theme::Light),
        &[],
        None,
    ));
    let prepared = scene.gpu_vertices();
    let end = scene.road_span.as_ref().unwrap().end as usize;
    let expected: Vec<_> = scene.vertices[..end]
        .iter()
        .map(gpu_vertex_from_world)
        .collect();
    assert_eq!(
        bytes_of(&prepared[..end]),
        bytes_of(&expected),
        "contact baking changed road culling ranges"
    );
    assert!(
        prepared
            .iter()
            .any(|v| v.surface[0] == 7. && v.surface[3] < -4.),
        "editor junction contacts missing"
    );
    for (name, camera, target, zoom, eye_height) in [
        (
            "editor-street",
            CameraView::Vehicular,
            WorldPoint { x: 0., y: -0.075 },
            4000.,
            0.025,
        ),
        (
            "editor-iso",
            CameraView::Isometric,
            WorldPoint { x: 0.15, y: 0.08 },
            1400.,
            0.,
        ),
    ] {
        let mut images = Vec::new();
        for enabled in [false, true] {
            let callback = SceneCallback {
                time_seconds: 0.,
                transition: None,
                scene: Arc::clone(&scene),
                rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024., 640.)),
                pan: egui::Vec2::ZERO,
                zoom,
                target,
                angle: std::f32::consts::FRAC_PI_2,
                elevation: ISO_ELEVATION,
                eye_height,
                camera,
                background: Color32::from_rgb(163, 179, 184),
                target_format: wgpu::TextureFormat::Bgra8Unorm,
            };
            let pixels = render_test_callback_with_contacts(callback, 1024, 640, enabled)
                .expect("native GPU required");
            if let Some(directory) = std::env::var_os("CONTACT_DEBRIS_CAPTURE") {
                std::fs::create_dir_all(&directory).unwrap();
                let mut rgba = pixels.clone();
                for pixel in rgba.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
                let path = std::path::Path::new(&directory).join(format!(
                    "{name}-{}.png",
                    if enabled { "after" } else { "before" }
                ));
                let mut png = png::Encoder::new(std::fs::File::create(path).unwrap(), 1024, 640);
                png.set_color(png::ColorType::Rgba);
                png.set_depth(png::BitDepth::Eight);
                png.write_header().unwrap().write_image_data(&rgba).unwrap();
            }
            images.push(pixels);
        }
        let changed = images[0]
            .chunks_exact(4)
            .zip(images[1].chunks_exact(4))
            .filter(|(a, b)| (0..3).any(|i| a[i].abs_diff(b[i]) > 2))
            .count();
        assert!(changed > 20, "{name} contact shading missing: {changed}");
    }
}
