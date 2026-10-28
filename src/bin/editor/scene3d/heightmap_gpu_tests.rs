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
            time_seconds: 0.0,
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
            time_seconds: 0.0,
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

#[test]
fn forest_and_concrete_ground_cover_show_above_png_terrain_below_roads() {
    let fixture = super::super::super::super::elevation_map::tests::Fixture::new(|_, _| 255);
    let palette = editor_palette(egui::Theme::Light);
    for (declaration, expected) in [
        (
            "cover=vegetation(forest,-0.8,-0.8,0.8,-0.8,0.8,0.8,-0.8,0.8);",
            [126, 191, 140],
        ),
        (
            "cover=concrete(-0.8,-0.8,0.8,-0.8,0.8,0.8,-0.8,0.8);",
            [190, 191, 184],
        ),
    ] {
        let mut program = fixture.program();
        program.statements.extend(
            parse(&format!(
                "{declaration} elevation(cover,1.022); route=street(-1,0.35,1,\
                    0.35); elevation(route,1.005);"
            ))
            .unwrap()
            .statements,
        );
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
            let target = WorldPoint { x: 0.0, y: 0.0 };
            let pan = if camera == CameraView::Top {
                egui::Vec2::ZERO
            } else {
                rect.center()
                    - iso_screen(
                        rect,
                        egui::Vec2::ZERO,
                        70.0,
                        target,
                        0.74,
                        ISO_ELEVATION,
                        target,
                        1.002,
                    )
            };
            let callback = SceneCallback {
                time_seconds: 0.0,
                transition: None,
                scene: Arc::clone(&scene),
                rect,
                pan,
                zoom: 70.0,
                target,
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
            let center = &pixels[(64 * 128 + 64) * 4..][..3];
            assert!(
                center
                    .iter()
                    .zip(expected)
                    .all(|(channel, e)| (*channel as i32 - e).abs() < 4),
                "ground cover must show above terrain: {center:?} vs {expected:?}"
            );
            let road = WorldPoint { x: 0.0, y: 0.35 };
            let screen = if camera == CameraView::Top {
                screen_of(rect, pan, 70.0, road)
            } else {
                iso_screen(rect, pan, 70.0, target, 0.74, ISO_ELEVATION, road, 1.010)
            };
            let p = &pixels[((screen.y as usize) * 128 + screen.x as usize) * 4..][..3];
            assert!(
                p[0] > p[1] + 10 && p[1] > p[2] + 10,
                "road must stay over ground cover: {p:?}"
            );
        }
    }
}
