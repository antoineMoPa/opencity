//! Isolated, matched specimen captures through the production editor pipeline.
use super::*;

#[test]
#[ignore = "manual architecture comparison capture"]
fn render_architecture_comparison() {
    let catalog = parse(include_str!("../../../../examples/building_catalog.road")).unwrap();
    let features = project_landscape_features(&catalog).features;
    let directory = std::env::var("OPENCITY_BUILDING_CAPTURE_DIR")
        .unwrap_or_else(|_| "/private/tmp/opencity-building-comparison".into());
    std::fs::create_dir_all(&directory).unwrap();
    for feature in features
        .iter()
        .filter(|f| f.kind == LandscapeKind::Building)
    {
        let recipe =
            opencity::runtime::architecture::BuildingRecipe::from_feature(feature).unwrap();
        let program = Program {
            statements: catalog
                .statements
                .iter()
                .filter(|s| match s {
                    Statement::Call(c) => {
                        c.name == "concrete"
                            || c.name == "elevation"
                            || c.assigns.first() == feature.name.as_ref()
                    }
                    _ => false,
                })
                .cloned()
                .collect(),
        };
        let palette = editor_palette(egui::Theme::Dark);
        let scene = Arc::new(build_scene(
            &program,
            &map::project(&program),
            palette,
            &[],
            None,
        ));
        assert!(scene.vertices.iter().any(|v| v.surface[0] == 3.));
        let target = WorldPoint {
            x: feature.points.iter().map(|p| p[0]).sum::<f32>() / 4.,
            y: feature.points.iter().map(|p| p[1]).sum::<f32>() / 4.,
        };
        let span = recipe.width.max(recipe.depth) * 1.42;
        let vertical = recipe.height * ISO_ELEVATION.cos() + span * ISO_ELEVATION.sin() * 0.5;
        let zoom = (1024. * 0.75 / span).min(768. * 0.80 / vertical);
        let callback = SceneCallback {
            time_seconds: 0.,
            transition: None,
            scene,
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024., 768.)),
            pan: egui::vec2(0., recipe.height * ISO_ELEVATION.cos() * zoom * 0.5),
            zoom,
            target,
            angle: 0.74,
            elevation: ISO_ELEVATION,
            eye_height: 0.,
            camera: CameraView::Isometric,
            background: Color32::from_rgb(163, 179, 184),
            target_format: wgpu::TextureFormat::Bgra8Unorm,
        };
        let pixels =
            render_test_callback_at(callback, 1024, 768).expect("native GPU capture required");
        let mut colors = std::collections::HashSet::new();
        for p in pixels.chunks_exact(4) {
            colors.insert([p[0], p[1], p[2]]);
        }
        assert!(
            colors.len() > 64,
            "{name}: facade detail must survive GPU shading",
            name = feature.name.as_deref().unwrap()
        );
        let name = recipe.family.keyword();
        let path = format!("{directory}/native-{name}.png");
        let rgba: Vec<_> = pixels
            .chunks_exact(4)
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = png::Encoder::new(file, 1024, 768);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&rgba)
            .unwrap();
        eprintln!("captured {path}");
    }
}
