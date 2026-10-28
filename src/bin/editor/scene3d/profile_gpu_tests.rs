//! Actual GPU seam checks for the default city's road profile families.
use super::*;
fn callback(
    scene: Arc<Scene>,
    point: WorldPoint,
    zoom: f32,
    camera: CameraView,
    palette: EditorPalette,
) -> SceneCallback {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(128., 128.));
    let pan = if camera == CameraView::Top {
        egui::vec2(-point.x * zoom, point.y * zoom)
    } else {
        rect.center()
            - iso_screen(
                rect,
                egui::Vec2::ZERO,
                zoom,
                point,
                0.7,
                ISO_ELEVATION,
                point,
                0.005,
            )
    };
    SceneCallback {
        time_seconds: 0.0,
        transition: None,
        scene,
        rect,
        pan,
        zoom,
        target: point,
        angle: 0.7,
        elevation: ISO_ELEVATION,
        eye_height: 0.,
        camera,
        background: palette.map_background,
        target_format: wgpu::TextureFormat::Bgra8Unorm,
    }
}
fn save_preview(pixels: &[u8], name: &str, camera: CameraView) {
    let Some(folder) = std::env::var_os("OPENCITY_INTERSECTION_QA") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(format!(
        "{name}-{}.png",
        if camera == CameraView::Top {
            "top"
        } else {
            "isometric"
        }
    ));
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, 128, 128);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let rgba: Vec<_> = pixels
        .chunks_exact(4)
        .flat_map(|p| [p[2], p[1], p[0], p[3]])
        .collect();
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&rgba)
        .unwrap();
}
#[test]
fn profile_pavement_and_sidewalk_colors_match_across_ports_in_both_themes_and_views() {
    let program = parse(include_str!(
        "../../../../examples/intersection_profiles.road"
    ))
    .unwrap();
    let projection = map::project(&program);
    let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let palette = editor_palette(theme);
        let scene = Arc::new(build_scene(&program, &projection, palette, &[], None));
        for junction in map::intersections(&program, &projection) {
            let half = map::intersection_footprint_half(
                &junction,
                program
                    .statements
                    .iter()
                    .filter_map(|s| match s {
                        Statement::Road(road) if road.name == junction.road_name => Some(
                            road.elements
                                .iter()
                                .map(|e| map::element_world_width(e.kind))
                                .sum::<f32>()
                                / 2.,
                        ),
                        _ => None,
                    })
                    .next()
                    .unwrap(),
            );
            for (index, end) in map::intersection_arms(&projection, &junction.point_ref)
                .into_iter()
                .take(1)
            {
                let segment = &projection.segments[index];
                let road = road_for_segment(&program, segment).unwrap();
                let row = if end {
                    geometry.cross_sections(index).last().unwrap()
                } else {
                    &geometry.cross_sections(index)[0]
                };
                for kind in [RoadElementKind::CarLane, RoadElementKind::Sidewalk] {
                    let Some(band) = road
                        .elements
                        .iter()
                        .position(|element| element.kind == kind)
                    else {
                        continue;
                    };
                    let a = row.boundaries[band].point;
                    let b = row.boundaries[band + 1].point;
                    let middle = WorldPoint {
                        x: (a.x + b.x) / 2.,
                        y: (a.y + b.y) / 2.,
                    };
                    let expected = kind_color(kind, theme).to_array();
                    for camera in [CameraView::Top, CameraView::Isometric] {
                        for factor in [-0.03, 0.03] {
                            let center = segment.point_at(row.t);
                            let point = WorldPoint {
                                x: middle.x + (junction.position.x - center.x) * factor,
                                y: middle.y + (junction.position.y - center.y) * factor,
                            };
                            let Some(pixels) = render_test_callback(callback(
                                Arc::clone(&scene),
                                point,
                                1000.,
                                camera,
                                palette,
                            )) else {
                                return;
                            };
                            for y in 63..65 {
                                for x in 63..65 {
                                    let pixel = &pixels[(y * 128 + x) * 4..][..4];
                                    for (channel, rgb) in [2, 1, 0].into_iter().enumerate() {
                                        assert!(
                                            (pixel[channel] as i32 - expected[rgb] as i32).abs()
                                                < 6,
                                            "{} {:?} theme light={} at seam factor {factor}, \
                                            pixel {pixel:?}, \
                                            expected {expected:?}",
                                            junction.point_ref,
                                            kind,
                                            theme == egui::Theme::Light
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if theme == egui::Theme::Dark && std::env::var_os("OPENCITY_INTERSECTION_QA").is_some()
            {
                for camera in [CameraView::Top, CameraView::Isometric] {
                    if let Some(pixels) = render_test_callback(callback(
                        Arc::clone(&scene),
                        junction.position,
                        128. / (half * 3.),
                        camera,
                        palette,
                    )) {
                        save_preview(&pixels, &junction.point_ref, camera);
                    }
                }
            }
        }
    }
}
