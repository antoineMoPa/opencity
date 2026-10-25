//! Offscreen rendering regressions and manual QA gallery captures.
use super::*;
use egui_wgpu::CallbackTrait;
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

#[test]
#[ignore = "manual GPU frame timing"]
fn profile_starter_camera_frames() {
    let qa_target = std::env::var_os("OPENCITY_QA_TARGET").is_some();
    let program = if qa_target {
        boot_program()
    } else {
        starter_program()
    };
    let project_start = std::time::Instant::now();
    let projection = map::project(&program);
    eprintln!(
        "projection={:?} segments={} unresolved={}",
        project_start.elapsed(),
        projection.segments.len(),
        projection.unresolved
    );
    let start = std::time::Instant::now();
    let scene = Arc::new(build_scene(
        &program,
        &projection,
        editor_palette(egui::Theme::Dark),
        &[],
        None,
    ));
    let build = start.elapsed();
    let instance = wgpu::Instance::default();
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("GPU adapter required for manual profile");
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .expect("GPU device required for manual profile");
    let mut resources = egui_wgpu::CallbackResources::default();
    let mut callback = SceneCallback {
        transition: None,
        scene: Arc::clone(&scene),
        rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024.0, 768.0)),
        pan: egui::Vec2::ZERO,
        zoom: 75.0,
        target: WorldPoint { x: 0.0, y: 0.0 },
        angle: 0.0,
        elevation: ISO_ELEVATION,
        eye_height: 0.0,
        camera: CameraView::Isometric,
        background: Color32::BLACK,
        target_format: wgpu::TextureFormat::Bgra8Unorm,
    };
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [1024, 768],
        pixels_per_point: 1.0,
    };
    let start = std::time::Instant::now();
    let mut encoder = device.create_command_encoder(&Default::default());
    callback.prepare(&device, &queue, &screen, &mut encoder, &mut resources);
    queue.submit([encoder.finish()]);
    let cold = start.elapsed();
    let start = std::time::Instant::now();
    for frame in 0..100 {
        callback.angle = frame as f32 * 0.01;
        let mut encoder = device.create_command_encoder(&Default::default());
        callback.prepare(&device, &queue, &screen, &mut encoder, &mut resources);
        queue.submit([encoder.finish()]);
    }
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let warm = start.elapsed() / 100;
    callback.scene = Arc::new(build_scene(
        &program,
        &projection,
        editor_palette(egui::Theme::Dark),
        &[],
        None,
    ));
    let start = std::time::Instant::now();
    let mut encoder = device.create_command_encoder(&Default::default());
    callback.prepare(&device, &queue, &screen, &mut encoder, &mut resources);
    queue.submit([encoder.finish()]);
    let scene_swap = start.elapsed();
    eprintln!(
            "starter vertices={} scene_build={build:?} cold_frame={cold:?} warm_frame={warm:?} scene_swap={scene_swap:?}",
            scene.vertices.len(),
        );
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn render_test_callback(callback: SceneCallback) -> Option<Vec<u8>> {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        eprintln!("Skipping GPU readback: no wgpu adapter is available");
        return None;
    };
    let Ok((device, queue)) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
    else {
        eprintln!("Skipping GPU readback: no wgpu device is available");
        return None;
    };
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let _resources = RenderResources::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let _srgb_resources = RenderResources::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
    let mut resources = egui_wgpu::CallbackResources::default();
    let mut encoder = device.create_command_encoder(&Default::default());
    callback.prepare(
        &device,
        &queue,
        &egui_wgpu::ScreenDescriptor {
            size_in_pixels: [128, 128],
            pixels_per_point: 1.0,
        },
        &mut encoder,
        &mut resources,
    );
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scene test output"),
        size: wgpu::Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = output.create_view(&Default::default());
    {
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene test composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let mut pass = pass.forget_lifetime();
        pass.set_viewport(0.0, 0.0, 128.0, 128.0, 0.0, 1.0);
        callback.paint(
            egui::epaint::PaintCallbackInfo {
                viewport: callback.rect,
                clip_rect: callback.rect,
                pixels_per_point: 1.0,
                screen_size_px: [128, 128],
            },
            &mut pass,
            &resources,
        );
    }
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("scene test readback"),
        size: 128 * 128 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &output,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(128 * 4),
                rows_per_image: Some(128),
            },
        },
        wgpu::Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        sender.send(result).unwrap();
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver.recv().unwrap().unwrap();
    let pixels = readback.get_mapped_range(..).unwrap();
    assert_eq!(block_on(scope.pop()).map(|error| error.to_string()), None);
    let result = pixels.to_vec();
    drop(pixels);
    readback.unmap();
    Some(result)
}

#[test]
fn wgpu_scene_pipelines_validate_when_an_adapter_is_available() {
    let callback = SceneCallback {
        transition: None,
        scene: Arc::new(Scene {
            vertices: vec![
                WorldVertex {
                    point: WorldPoint { x: -1.0, y: -1.0 },
                    height: 0.0,
                    depth_bias: 0.0,
                    color: Color32::RED,
                },
                WorldVertex {
                    point: WorldPoint { x: 1.0, y: -1.0 },
                    height: 0.0,
                    depth_bias: 0.0,
                    color: Color32::RED,
                },
                WorldVertex {
                    point: WorldPoint { x: 0.0, y: 1.0 },
                    height: 0.0,
                    depth_bias: 0.0,
                    color: Color32::RED,
                },
            ],
            ..Default::default()
        }),
        rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(128.0, 128.0)),
        pan: egui::Vec2::ZERO,
        zoom: 40.0,
        target: WorldPoint { x: 0.0, y: 0.0 },
        angle: 0.0,
        elevation: ISO_ELEVATION,
        eye_height: 0.0,
        camera: CameraView::Top,
        background: Color32::BLACK,
        target_format: wgpu::TextureFormat::Bgra8Unorm,
    };
    let Some(pixels) = render_test_callback(callback) else {
        return;
    };
    let center = &pixels[(64 * 128 + 64) * 4..(64 * 128 + 64) * 4 + 4];
    assert!(center[2] > 180 && center[1] < 80 && center[0] < 80);
    assert!(pixels[0] < 10 && pixels[1] < 10 && pixels[2] < 10);
}

#[test]
fn vehicular_layers_are_stable_without_hiding_nearer_geometry() {
    for distance in [2.0, 200.0, 1000.0] {
        for occluded in [false, true] {
            let mut vertices = Vec::new();
            // Draw the preferred coplanar layer first: depth, not draw order,
            // must keep it in front. A genuinely nearer surface still wins.
            for (color, bias, forward) in [
                (Color32::GREEN, -0.003, distance),
                (
                    Color32::RED,
                    0.0,
                    if occluded { distance * 0.5 } else { distance },
                ),
            ] {
                for (x, height) in [
                    (-distance, -distance),
                    (distance, -distance),
                    (0.0, distance),
                ] {
                    vertices.push(WorldVertex {
                        point: WorldPoint { x, y: forward },
                        height,
                        depth_bias: bias,
                        color,
                    });
                }
            }
            let callback = SceneCallback {
                transition: None,
                scene: Arc::new(Scene {
                    vertices,
                    ..Default::default()
                }),
                rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(128.0, 128.0)),
                pan: egui::Vec2::ZERO,
                zoom: 1.0,
                target: WorldPoint { x: 0.0, y: 0.0 },
                angle: std::f32::consts::FRAC_PI_2,
                elevation: ISO_ELEVATION,
                eye_height: 0.0,
                camera: CameraView::Vehicular,
                background: Color32::BLACK,
                target_format: wgpu::TextureFormat::Bgra8Unorm,
            };
            let Some(pixels) = render_test_callback(callback) else {
                return;
            };
            for y in 48..80 {
                for x in 48..80 {
                    let pixel = &pixels[(y * 128 + x) * 4..][..4];
                    assert!(
                        if occluded {
                            pixel[2] > 180 && pixel[1] < 80
                        } else {
                            pixel[1] > 180 && pixel[2] < 80
                        },
                        "distance={distance}, occluded={occluded}, pixel={pixel:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn merge_pavement_renders_over_parent_verge_in_top_and_isometric_views() {
    for source in [
        include_str!("../../../../road_library/street_to_interstate_flat.road"),
        include_str!("../../../../road_library/street_to_interstate_elevated.road"),
    ] {
        let program = parse(source).unwrap();
        let projection = map::project(&program);
        let mouth = projection
            .segments
            .iter()
            .find(|s| s.name.as_deref() == Some("merge_mouth"))
            .unwrap();
        let t = (0..1000)
            .map(|i| i as f32 / 1000.0)
            .find(|&t| (mouth.point_at(t).y - 0.085).abs() < 0.0005)
            .unwrap();
        let point = mouth.point_at(t);
        let palette = editor_palette(egui::Theme::Dark);
        let scene = Arc::new(build_scene(&program, &projection, palette, &[], None));
        for camera in [CameraView::Top, CameraView::Isometric] {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(128.0, 128.0));
            let zoom = 1000.0;
            let pan = if camera == CameraView::Top {
                egui::vec2(-point.x * zoom, point.y * zoom)
            } else {
                rect.center()
                    - iso_screen(
                        rect,
                        egui::Vec2::ZERO,
                        zoom,
                        point,
                        0.74,
                        ISO_ELEVATION,
                        point,
                        mouth.height_at(t) + 0.005,
                    )
            };
            let callback = SceneCallback {
                transition: None,
                scene: Arc::clone(&scene),
                rect,
                pan,
                zoom,
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
            for y in 62..66 {
                for x in 62..66 {
                    let pixel = &pixels[(y * 128 + x) * 4..][..4];
                    assert!(
                        pixel[0] > 120
                            && (pixel[0] as i32 - pixel[1] as i32).abs() < 12
                            && (pixel[1] as i32 - pixel[2] as i32).abs() < 12,
                        "merge must show gray pavement, not green verge/dark outline: {pixel:?}"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "manual offscreen visual QA"]
fn render_starter_scene_pngs() {
    const WIDTH: u32 = 1024;
    const HEIGHT: u32 = 768;
    let instance = wgpu::Instance::default();
    let adapter =
        block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
    let (device, queue) =
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let qa_target = std::env::var_os("OPENCITY_QA_TARGET").is_some();
    let qa_library = std::env::var_os("OPENCITY_QA_LIBRARY").is_some();
    let program = if qa_library {
        parse(include_str!("../../../../road_library/qa_gallery.road")).unwrap()
    } else if qa_target {
        boot_program()
    } else {
        starter_program()
    };
    let projection = map::project(&program);
    let palette = editor_palette(egui::Theme::Dark);
    let scene = Arc::new(build_scene(&program, &projection, palette, &[], None));
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    for segment in &projection.segments {
        for point in [
            segment.start,
            segment.controls[0],
            segment.controls[1],
            segment.end,
        ] {
            min_x = min_x.min(point.x);
            min_y = min_y.min(point.y);
            max_x = max_x.max(point.x);
            max_y = max_y.max(point.y);
        }
    }
    let center = WorldPoint {
        x: (min_x + max_x) * 0.5,
        y: (min_y + max_y) * 0.5,
    };
    let fit = (WIDTH as f32 / (max_x - min_x + 5.0)).min(HEIGHT as f32 / (max_y - min_y + 5.0));
    eprintln!("starter bounds: ({min_x},{min_y}) to ({max_x},{max_y}), fit zoom {fit}");
    let junction = map::intersections(&program, &projection)
        .into_iter()
        .find(|item| item.point_ref == "grid_1_1")
        .map(|item| item.position)
        .unwrap_or(center);
    let interchange = if qa_target {
        WorldPoint { x: 0.82, y: 2.68 }
    } else {
        WorldPoint { x: 4.7, y: -3.0 }
    };
    let mut views = vec![
        ("top-full", CameraView::Top, center, fit),
        ("iso-full", CameraView::Isometric, center, fit * 0.72),
        ("top-intersection", CameraView::Top, junction, 125.0),
        ("iso-intersection", CameraView::Isometric, junction, 125.0),
        ("top-interchange", CameraView::Top, interchange, 115.0),
        ("iso-interchange", CameraView::Isometric, interchange, 115.0),
    ];
    if qa_target {
        for (name, x, y) in [
            ("top-west-junction", 265.0, 242.0),
            ("top-east-junction", 1385.0, 486.0),
            ("top-north-terminal", 816.0, 90.0),
            ("top-south-terminal", 817.0, 628.0),
        ] {
            views.push((
                name,
                CameraView::Top,
                WorldPoint {
                    x: (x - 768.0) / 60.0,
                    y: (512.0 - y) / 60.0,
                },
                90.0,
            ));
        }
    }
    if qa_library {
        views = vec![
            (
                "top-four-way-seams",
                CameraView::Top,
                WorldPoint { x: -8.0, y: 5.0 },
                1400.0,
            ),
            (
                "iso-four-way-seams",
                CameraView::Isometric,
                WorldPoint { x: -8.0, y: 5.0 },
                1400.0,
            ),
            (
                "top-three-way-seams",
                CameraView::Top,
                WorldPoint { x: -4.0, y: 5.0 },
                1400.0,
            ),
            (
                "iso-three-way-seams",
                CameraView::Isometric,
                WorldPoint { x: -4.0, y: 5.0 },
                1400.0,
            ),
            (
                "iso-connector-sidewalls",
                CameraView::Isometric,
                WorldPoint { x: 10.55, y: 2.45 },
                1200.0,
            ),
            (
                "iso-elevated-merge",
                CameraView::Isometric,
                WorldPoint { x: -7.0, y: 0.0 },
                1400.0,
            ),
            (
                "top-elevated-merge",
                CameraView::Top,
                WorldPoint { x: -7.0, y: 0.0 },
                1400.0,
            ),
            (
                "vehicle-cloverleaf",
                CameraView::Vehicular,
                WorldPoint { x: 5.7, y: -0.06 },
                1.0,
            ),
            (
                "vehicle-elevated-merge",
                CameraView::Vehicular,
                WorldPoint { x: -8.0, y: 0.4 },
                1.0,
            ),
        ];
    }
    for (name, camera, target, zoom) in views {
        let rect =
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WIDTH as f32, HEIGHT as f32));
        let pan = if camera == CameraView::Top {
            egui::vec2(-target.x * zoom, target.y * zoom)
        } else if qa_library && camera == CameraView::Isometric {
            egui::vec2(0.0, 300.0)
        } else {
            egui::Vec2::ZERO
        };
        let callback = SceneCallback {
            transition: None,
            scene: Arc::clone(&scene),
            rect,
            pan,
            zoom,
            target,
            angle: if camera == CameraView::Vehicular {
                0.0
            } else {
                0.74
            },
            elevation: ISO_ELEVATION,
            eye_height: if camera == CameraView::Vehicular {
                0.45
            } else {
                0.0
            },
            camera,
            background: palette.map_background,
            target_format: wgpu::TextureFormat::Bgra8Unorm,
        };
        let mut resources = egui_wgpu::CallbackResources::default();
        let mut encoder = device.create_command_encoder(&Default::default());
        callback.prepare(
            &device,
            &queue,
            &egui_wgpu::ScreenDescriptor {
                size_in_pixels: [WIDTH, HEIGHT],
                pixels_per_point: 1.0,
            },
            &mut encoder,
            &mut resources,
        );
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("starter QA output"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = output.create_view(&Default::default());
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("starter QA composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut pass = pass.forget_lifetime();
            pass.set_viewport(0.0, 0.0, WIDTH as f32, HEIGHT as f32, 0.0, 1.0);
            callback.paint(
                egui::epaint::PaintCallbackInfo {
                    viewport: rect,
                    clip_rect: rect,
                    pixels_per_point: 1.0,
                    screen_size_px: [WIDTH, HEIGHT],
                },
                &mut pass,
                &resources,
            );
        }
        let size = (WIDTH * HEIGHT * 4) as u64;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("starter QA readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &output,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(WIDTH * 4),
                    rows_per_image: Some(HEIGHT),
                },
            },
            wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            sender.send(result).unwrap();
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receiver.recv().unwrap().unwrap();
        let mapped = readback.get_mapped_range(..).unwrap();
        let mut rgba = mapped.to_vec();
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        drop(mapped);
        readback.unmap();
        let prefix = if qa_library {
            "library"
        } else if qa_target {
            "target"
        } else {
            "starter"
        };
        let path = format!("/private/tmp/opencity-{prefix}-{name}-qa.png");
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&rgba)
            .unwrap();
        eprintln!("wrote {path}");
    }
}

#[path = "gpu_transition_tests.rs"]
mod transition_tests;

#[test]
fn junction_interior_renders_once_without_approach_centerlines_in_both_views() {
    let program = parse(include_str!(
        "../../../../examples/default_intersection.road"
    ))
    .unwrap();
    let projection = map::project(&program);
    let scene = Arc::new(build_scene(
        &program,
        &projection,
        editor_palette(egui::Theme::Dark),
        &[],
        None,
    ));
    for camera in [CameraView::Top, CameraView::Isometric] {
        let callback = SceneCallback {
            transition: None,
            scene: Arc::clone(&scene),
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(128., 128.)),
            pan: egui::Vec2::ZERO,
            zoom: 800.,
            target: WorldPoint { x: 0., y: 0. },
            angle: 0.7,
            elevation: ISO_ELEVATION,
            eye_height: 0.,
            camera,
            background: Color32::BLACK,
            target_format: wgpu::TextureFormat::Bgra8Unorm,
        };
        let Some(pixels) = render_test_callback(callback) else {
            return;
        };
        for y in 62..66 {
            for x in 62..66 {
                let pixel = &pixels[(y * 128 + x) * 4..][..4];
                let expected = [174, 180, 175];
                for channel in 0..3 {
                    assert!(
                        (pixel[channel] as i32 - expected[channel]).abs() < 5,
                        "road paint leaks through junction: {pixel:?}"
                    );
                }
            }
        }
        if let Some(folder) = std::env::var_os("OPENCITY_INTERSECTION_QA") {
            let folder = std::path::PathBuf::from(folder);
            std::fs::create_dir_all(&folder).unwrap();
            let path = folder.join(if camera == CameraView::Top {
                "junction-top.png"
            } else {
                "junction-isometric.png"
            });
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
    }
}

#[path = "profile_gpu_tests.rs"]
mod profile_gpu_tests;
