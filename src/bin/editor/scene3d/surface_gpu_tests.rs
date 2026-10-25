//! Exercise surface shading through the production GPU pipeline and readback.
use super::*;

fn callback(scene: Arc<Scene>, time_seconds: f32) -> SceneCallback {
    SceneCallback {
        time_seconds,
        transition: None,
        scene,
        rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(128.0, 128.0)),
        pan: egui::Vec2::ZERO,
        zoom: 48.0,
        target: WorldPoint { x: 0.0, y: 0.0 },
        angle: 0.0,
        elevation: ISO_ELEVATION,
        eye_height: 0.0,
        camera: CameraView::Top,
        background: Color32::BLACK,
        target_format: wgpu::TextureFormat::Bgra8Unorm,
    }
}
fn pixel(pixels: &[u8], x: usize, y: usize) -> &[u8] {
    &pixels[(y * 128 + x) * 4..][..4]
}

#[test]
fn coast_shading_uses_bed_depth_and_loops_after_four_seconds() {
    let fixture = super::super::super::super::elevation_map::tests::Fixture::new(|x, _| {
        if x < 512 {
            0
        } else {
            84
        }
    });
    let mut program = fixture.program();
    program
        .statements
        .extend(parse("water_level(0.35);").unwrap().statements);
    let scene = Arc::new(build_scene_with_model_base(
        &program,
        &map::project(&program),
        editor_palette(egui::Theme::Light),
        &[],
        None,
        Some(&fixture.dir),
    ));
    assert!(scene.has_animated_water());
    let Some(first) = render_test_callback(callback(Arc::clone(&scene), 0.0)) else {
        return;
    };
    let Some(second) = render_test_callback(callback(Arc::clone(&scene), 1.5)) else {
        return;
    };
    let Some(looped) = render_test_callback(callback(scene, 4.0)) else {
        return;
    };
    let deep = pixel(&first, 32, 64);
    let shallow = pixel(&first, 96, 64);
    assert!(
        shallow[1] > deep[1] + 3 && shallow[2] > deep[2] + 5,
        "shallow coast must lighten relative to deep water: {shallow:?} vs {deep:?}"
    );
    assert_eq!(
        pixel(&first, 32, 64),
        pixel(&second, 32, 64),
        "deep water must stay still"
    );
    assert_ne!(
        pixel(&first, 96, 64),
        pixel(&second, 96, 64),
        "coast must animate"
    );
    assert_eq!(
        first, looped,
        "water animation must repeat after four seconds"
    );
}

fn facade_scene(material: f32) -> Arc<Scene> {
    let mut scene = Scene::default();
    // A vertical wall faces the camera along +X. Its floor height projects to 16px.
    for [x, z] in [
        [-0.7, -0.6],
        [0.7, -0.6],
        [0.7, 0.6],
        [-0.7, -0.6],
        [0.7, 0.6],
        [-0.7, 0.6],
    ] {
        scene.vertices.push(WorldVertex {
            point: WorldPoint { x, y: 2.0 },
            height: z,
            depth_bias: 0.0,
            color: Color32::from_rgb(90, 105, 115),
            surface: [1.0, -0.6, 0.35, material],
        });
    }
    Arc::new(scene)
}
#[test]
fn procedural_floor_bands_are_gray_and_materials_are_distinct() {
    let mut images = Vec::new();
    for material in [1.0, 2.0, 3.0] {
        let mut frame = callback(facade_scene(material), 0.0);
        frame.camera = CameraView::Vehicular;
        frame.angle = std::f32::consts::FRAC_PI_2;
        let Some(image) = render_test_callback(frame) else {
            return;
        };
        let gray_count = image
            .chunks_exact(4)
            .filter(|p| p[0] > 160 && p[1] > 160 && p[2] > 160 && p[0].abs_diff(p[2]) < 20)
            .count();
        assert!(
            gray_count > 15,
            "material {material} must show horizontal gray floor bands"
        );
        images.push(image);
    }
    assert_ne!(images[0], images[1]);
    assert_ne!(images[1], images[2]);
    assert_ne!(images[0], images[2]);
}

#[test]
fn plain_surfaces_are_unaffected_by_animation_clock() {
    let mut scene = Scene::default();
    for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (0.0, 1.0)] {
        scene.vertices.push(WorldVertex {
            point: WorldPoint { x, y },
            height: 0.0,
            depth_bias: 0.0,
            color: Color32::RED,
            surface: [0.0; 4],
        });
    }
    assert!(!scene.has_animated_water());
    let scene = Arc::new(scene);
    let Some(first) = render_test_callback(callback(Arc::clone(&scene), 0.0)) else {
        return;
    };
    let Some(second) = render_test_callback(callback(scene, 1.5)) else {
        return;
    };
    assert_eq!(first, second);
}
