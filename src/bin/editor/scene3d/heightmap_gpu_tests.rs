//! Verify that elevated roads survive depth testing against PNG terrain.
use super::*;

#[test]
fn roads_above_png_terrain_render_in_top_and_isometric_views() {
    let fixture = super::super::super::super::elevation_map::tests::Fixture::new(|_, _| 255);
    let mut program = fixture.program();
    program.statements.extend(
        parse("route=street(-1,0,1,0); grade(route,1.002,1.002);")
            .unwrap()
            .statements,
    );
    let palette = editor_palette(egui::Theme::Light);
    let scene = Arc::new(build_scene_with_model_base(
        &program,
        &map::project(&program),
        palette,
        &[],
        None,
        Some(&fixture.dir),
    ));
    for camera in [CameraView::Top, CameraView::Isometric] {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(128.0, 128.0));
        let point = WorldPoint { x: 0.0, y: 0.0 };
        let pan = if camera == CameraView::Top {
            egui::Vec2::ZERO
        } else {
            rect.center()
                - iso_screen(
                    rect,
                    egui::Vec2::ZERO,
                    10000.0,
                    point,
                    0.74,
                    ISO_ELEVATION,
                    point,
                    1.002,
                )
        };
        let callback = SceneCallback {
            transition: None,
            scene: Arc::clone(&scene),
            rect,
            pan,
            zoom: 10000.0,
            target: point,
            angle: 0.74,
            elevation: ISO_ELEVATION,
            eye_height: 0.0,
            camera,
            background: palette.map_background,
            target_format: wgpu::TextureFormat::Bgra8Unorm,
        };
        let Some(pixels) = render_test_callback(callback) else {
            return;
        };
        // Light-theme car lanes are blue; PNG terrain is green/brown.
        // Readback uses BGRA bytes, so pavement's blue channel comes first.
        let pixel = &pixels[(64 * 128 + 64) * 4..][..4];
        assert!(
            pixel[0] > pixel[1] + 10 && pixel[1] > pixel[2] + 10,
            "road was hidden by elevation terrain: {pixel:?}"
        );
    }
}

#[test]
fn threshold_water_hides_lowlands_and_leaves_highlands_visible() {
    let fixture = super::super::super::super::elevation_map::tests::Fixture::new(|x, _| {
        if x >= 512 {
            255
        } else {
            0
        }
    });
    let mut program = fixture.program();
    program
        .statements
        .extend(parse("water_level(0.35);").unwrap().statements);
    let palette = editor_palette(egui::Theme::Light);
    let scene = Arc::new(build_scene_with_model_base(
        &program,
        &map::project(&program),
        palette,
        &[],
        None,
        Some(&fixture.dir),
    ));
    for (x, water) in [(-1.0, true), (1.0, false)] {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(128.0, 128.0));
        let callback = SceneCallback {
            transition: None,
            scene: Arc::clone(&scene),
            rect,
            pan: egui::vec2(-x * 100.0, 0.0),
            zoom: 100.0,
            target: WorldPoint { x, y: 0.0 },
            angle: 0.74,
            elevation: ISO_ELEVATION,
            eye_height: 0.0,
            camera: CameraView::Top,
            background: palette.map_background,
            target_format: wgpu::TextureFormat::Bgra8Unorm,
        };
        let Some(pixels) = render_test_callback(callback) else {
            return;
        };
        let p = &pixels[(64 * 128 + 64) * 4..][..4];
        if water {
            assert!(p[0] > p[1] && p[1] > p[2], "expected blue water: {p:?}");
        } else {
            assert!(
                p[0] < p[1],
                "expected dry terrain above the threshold: {p:?}"
            );
        }
    }
}
