//! Shared world geometry for the top and isometric cameras. The detailed map
//! painters are tessellated in world space, then drawn with wgpu depth testing.
use super::*;
use eframe::{egui_wgpu, wgpu};
use std::ops::Range;
use std::sync::{Arc, Mutex, OnceLock};
use std::{collections::HashMap, path::Path};
use wgpu::util::DeviceExt;

const GEOMETRY_SCALE: f32 = 100.0;
const SCENE_SAMPLES: u32 = 4;
const MULTISAMPLE_MAX_ZOOM: f32 = 240.0;
const GEOMETRY_RECT: egui::Rect = egui::Rect::from_min_max(
    egui::pos2(-100_000.0, -100_000.0),
    egui::pos2(100_000.0, 100_000.0),
);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldVertex {
    pub point: WorldPoint,
    pub height: f32,
    // Render-order offset independent of physical elevation. Grid lines sit
    // under terrain even when a land feature has a negative elevation.
    pub depth_bias: f32,
    pub color: Color32,
}

#[derive(Default)]
pub struct Scene {
    pub vertices: Vec<WorldVertex>,
    road_span: Option<Range<u32>>,
    road_ranges: Vec<RoadDrawRange>,
    gpu_vertices: std::sync::OnceLock<Vec<GpuVertex>>,
}

struct RoadDrawRange {
    vertices: Range<u32>,
    min: WorldPoint,
    max: WorldPoint,
    min_height: f32,
    max_height: f32,
}

impl Scene {
    fn gpu_vertices(&self) -> &[GpuVertex] {
        self.gpu_vertices
            .get_or_init(|| self.vertices.iter().map(gpu_vertex_from_world).collect())
    }
}

fn gpu_vertex_from_world(vertex: &WorldVertex) -> GpuVertex {
    let rgba = vertex.color.to_array();
    GpuVertex {
        world: [
            vertex.point.x,
            vertex.point.y,
            vertex.height,
            vertex.depth_bias,
        ],
        color: rgba.map(|channel| channel as f32 / 255.0),
    }
}

fn capture_geometry(
    ctx: &egui::Context,
    paint: impl FnOnce(&egui::Painter),
    height: impl Fn(WorldPoint) -> f32,
    scene: &mut Scene,
) {
    // The tessellated vertices are kept in world space and reused at every
    // zoom. A one-pixel egui feather here becomes a 24-pixel blur at 2400x.
    // Let the scene target resolve edge coverage in screen pixels instead.
    ctx.tessellation_options_mut(|options| {
        options.feathering = false;
        options.round_line_segments_to_pixels = false;
        options.round_rects_to_pixels = false;
    });
    ctx.begin_pass(egui::RawInput {
        screen_rect: Some(GEOMETRY_RECT),
        ..Default::default()
    });
    let painter = ctx
        .layer_painter(egui::LayerId::background())
        .with_clip_rect(GEOMETRY_RECT);
    paint(&painter);
    let mut output = ctx.end_pass();
    output.textures_delta.clear();
    // A painter shape is one visual layer. Give later shapes a small depth
    // priority without moving their geometry: raising each tessellated
    // triangle made adjacent road facets visible as separate rectangles.
    for (layer, shape) in output.shapes.into_iter().enumerate() {
        for clipped in ctx.tessellate(vec![shape], 1.0) {
            let egui::epaint::Primitive::Mesh(mesh) = clipped.primitive else {
                continue;
            };
            if mesh
                .vertices
                .iter()
                .any(|vertex| vertex.uv != egui::epaint::WHITE_UV)
            {
                continue;
            }
            let vertices: Vec<_> = mesh
                .vertices
                .iter()
                .map(|vertex| {
                    let world =
                        world_of(GEOMETRY_RECT, egui::Vec2::ZERO, GEOMETRY_SCALE, vertex.pos);
                    WorldVertex {
                        point: world,
                        height: height(world),
                        depth_bias: -(layer as f32) * 0.0003,
                        color: vertex.color,
                    }
                })
                .collect();
            scene.vertices.reserve(mesh.indices.len());
            scene
                .vertices
                .extend(mesh.indices.iter().map(|index| vertices[*index as usize]));
        }
    }
}

fn add_building_walls(
    scene: &mut Scene,
    feature: &opencity::LandscapeFeature,
    palette: EditorPalette,
) {
    let Some(height) = feature.height else {
        return;
    };
    if feature.kind != LandscapeKind::Building {
        return;
    }
    let color = match palette.theme {
        egui::Theme::Dark => Color32::from_rgb(130, 142, 143),
        egui::Theme::Light => Color32::from_rgb(141, 151, 159),
    };
    for index in 0..feature.points.len() {
        let a = feature.points[index];
        let b = feature.points[(index + 1) % feature.points.len()];
        let corners = [
            (a, feature.elevation),
            (b, feature.elevation),
            (b, feature.elevation + height),
            (a, feature.elevation + height),
        ];
        for corner in [0, 1, 2, 0, 2, 3] {
            let (point, height) = corners[corner];
            scene.vertices.push(WorldVertex {
                point: WorldPoint {
                    x: point[0],
                    y: point[1],
                },
                height,
                depth_bias: -0.002,
                color,
            });
        }
    }
}

type ModelResult = Result<Arc<model::ModelMesh>, String>;

fn model_cache() -> &'static Mutex<HashMap<String, ModelResult>> {
    static MODELS: OnceLock<Mutex<HashMap<String, ModelResult>>> = OnceLock::new();
    MODELS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(target_arch = "wasm32")]
pub fn register_model(path: &str, bytes: &[u8]) -> Result<(), String> {
    let parsed = model::parse_glb(bytes).map(Arc::new);
    model_cache()
        .lock()
        .unwrap()
        .insert(path.to_owned(), parsed.clone());
    parsed.map(|_| ())
}

fn model_for(path: &str, base: Option<&Path>) -> Option<Arc<model::ModelMesh>> {
    #[cfg(target_arch = "wasm32")]
    let _ = base;
    #[cfg(not(target_arch = "wasm32"))]
    let key = {
        let source = Path::new(path);
        if source.is_absolute() {
            source.to_path_buf()
        } else {
            base.unwrap_or_else(|| Path::new(".")).join(source)
        }
        .to_string_lossy()
        .into_owned()
    };
    #[cfg(target_arch = "wasm32")]
    let key = path.to_owned();
    if let Some(Ok(mesh)) = model_cache().lock().unwrap().get(&key) {
        return Some(Arc::clone(mesh));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let parsed = std::fs::read(&key)
            .map_err(|error| error.to_string())
            .and_then(|bytes| model::parse_glb(&bytes))
            .map(Arc::new);
        let result = parsed.as_ref().ok().cloned();
        if parsed.is_ok() {
            model_cache().lock().unwrap().insert(key, parsed);
        }
        result
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

fn add_building_model(
    scene: &mut Scene,
    feature: &opencity::LandscapeFeature,
    mesh: &model::ModelMesh,
) {
    let Some(height) = feature.height else {
        return;
    };
    let min_x = feature
        .points
        .iter()
        .map(|point| point[0])
        .fold(f32::INFINITY, f32::min);
    let max_x = feature
        .points
        .iter()
        .map(|point| point[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = feature
        .points
        .iter()
        .map(|point| point[1])
        .fold(f32::INFINITY, f32::min);
    let max_y = feature
        .points
        .iter()
        .map(|point| point[1])
        .fold(f32::NEG_INFINITY, f32::max);
    let source_width = (mesh.max[0] - mesh.min[0]).max(0.0001);
    let source_depth = (mesh.max[2] - mesh.min[2]).max(0.0001);
    let horizontal = ((max_x - min_x) / source_width).min((max_y - min_y) / source_depth);
    let vertical = height / (mesh.max[1] - mesh.min[1]).max(0.0001);
    let source_center_x = (mesh.max[0] + mesh.min[0]) * 0.5;
    let source_center_z = (mesh.max[2] + mesh.min[2]) * 0.5;
    let center_x = (max_x + min_x) * 0.5;
    let center_y = (max_y + min_y) * 0.5;
    for triangle in &mesh.triangles {
        for vertex in triangle {
            let position = vertex.position;
            scene.vertices.push(WorldVertex {
                point: WorldPoint {
                    x: center_x + (position[0] - source_center_x) * horizontal,
                    y: center_y - (position[2] - source_center_z) * horizontal,
                },
                height: feature.elevation + (position[1] - mesh.min[1]) * vertical,
                depth_bias: -0.01,
                color: Color32::from_rgba_unmultiplied(
                    (vertex.color[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (vertex.color[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (vertex.color[2].clamp(0.0, 1.0) * 255.0) as u8,
                    (vertex.color[3].clamp(0.0, 1.0) * 255.0) as u8,
                ),
            });
        }
    }
}

fn add_terrain_walls(
    scene: &mut Scene,
    feature: &opencity::LandscapeFeature,
    features: &[opencity::LandscapeFeature],
    palette: EditorPalette,
) {
    let color = match palette.theme {
        egui::Theme::Dark => Color32::from_rgb(91, 110, 61),
        egui::Theme::Light => Color32::from_rgb(147, 124, 87),
    };
    for wall in opencity::terrain_walls(feature, features) {
        let corners = [
            (wall.start, wall.bottom),
            (wall.end, wall.bottom),
            (wall.end, wall.top),
            (wall.start, wall.top),
        ];
        for corner in [0, 1, 2, 0, 2, 3] {
            let (at, height) = corners[corner];
            scene.vertices.push(WorldVertex {
                point: WorldPoint { x: at[0], y: at[1] },
                height,
                depth_bias: -0.001,
                color,
            });
        }
    }
}

fn road_height(samples: &[WorldPoint; 17], heights: &[f32; 17], point: WorldPoint) -> f32 {
    let mut best = (f32::INFINITY, 0.0);
    for step in 0..16 {
        let a = samples[step];
        let b = samples[step + 1];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let along = (((point.x - a.x) * dx + (point.y - a.y) * dy)
            / (dx * dx + dy * dy).max(0.000001))
        .clamp(0.0, 1.0);
        let x = a.x + along * dx;
        let y = a.y + along * dy;
        let distance = (x - point.x).powi(2) + (y - point.y).powi(2);
        if distance < best.0 {
            best = (
                distance,
                heights[step] + (heights[step + 1] - heights[step]) * along,
            );
        }
    }
    best.1
}

fn append_landscape(
    scene: &mut Scene,
    ctx: &egui::Context,
    features: &[opencity::LandscapeFeature],
    palette: EditorPalette,
    selected_landscape: Option<&str>,
    model_base: Option<&Path>,
) {
    for feature in features {
        append_landscape_feature(
            scene,
            ctx,
            feature,
            features,
            palette,
            selected_landscape,
            model_base,
        );
    }
}

fn append_landscape_feature(
    scene: &mut Scene,
    ctx: &egui::Context,
    feature: &opencity::LandscapeFeature,
    features: &[opencity::LandscapeFeature],
    palette: EditorPalette,
    selected_landscape: Option<&str>,
    model_base: Option<&Path>,
) {
    // Flat landscape features frequently overlap at the same elevation. A
    // depth-only offset keeps their screen position fixed while giving the
    // fill a stable order regardless of triangle interpolation or draw order.
    // Roads use zero bias, so they remain in front of ground cover.
    let depth_priority = match feature.kind {
        LandscapeKind::Beach => 0.08,
        LandscapeKind::Land => 0.06,
        LandscapeKind::Lake | LandscapeKind::River => 0.04,
        LandscapeKind::Vegetation => 0.02,
        LandscapeKind::Concrete => 0.01,
        LandscapeKind::Terrain | LandscapeKind::Building => 0.0,
    };
    let mesh = feature
        .model
        .as_deref()
        .and_then(|path| model_for(path, model_base));
    let first_vertex = scene.vertices.len();
    capture_geometry(
        ctx,
        |painter| {
            paint_landscape_feature(
                painter,
                GEOMETRY_RECT,
                egui::Vec2::ZERO,
                GEOMETRY_SCALE,
                feature,
                selected_landscape == feature.name.as_deref(),
                palette,
            );
        },
        |_| {
            feature.elevation
                + if mesh.is_some() {
                    0.0
                } else {
                    feature.height.unwrap_or(0.0)
                }
                - 0.02
        },
        scene,
    );
    for vertex in &mut scene.vertices[first_vertex..] {
        vertex.depth_bias += depth_priority;
    }
    if let Some(mesh) = mesh {
        add_building_model(scene, feature, &mesh);
    } else {
        add_building_walls(scene, feature, palette);
    }
    add_terrain_walls(scene, feature, features, palette);
}

fn grid_vertices(palette: EditorPalette) -> &'static [WorldVertex] {
    static DARK: std::sync::OnceLock<Vec<WorldVertex>> = std::sync::OnceLock::new();
    static LIGHT: std::sync::OnceLock<Vec<WorldVertex>> = std::sync::OnceLock::new();
    let cache = match palette.theme {
        egui::Theme::Dark => &DARK,
        egui::Theme::Light => &LIGHT,
    };
    cache.get_or_init(|| {
        let mut grid = Scene::default();
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_center_size(egui::Pos2::ZERO, egui::vec2(20_000.0, 20_000.0));
        capture_geometry(
            &ctx,
            |painter| {
                paint_grid(painter, rect, egui::Vec2::ZERO, GEOMETRY_SCALE, palette);
            },
            |_| 0.0,
            &mut grid,
        );
        for vertex in &mut grid.vertices {
            vertex.depth_bias = 100.0;
        }
        grid.vertices
    })
}

pub fn loading_scene(palette: EditorPalette) -> Arc<Scene> {
    static DARK: OnceLock<Arc<Scene>> = OnceLock::new();
    static LIGHT: OnceLock<Arc<Scene>> = OnceLock::new();
    let cache = match palette.theme {
        egui::Theme::Dark => &DARK,
        egui::Theme::Light => &LIGHT,
    };
    Arc::clone(cache.get_or_init(|| {
        let mut scene = Scene::default();
        scene.vertices.extend_from_slice(grid_vertices(palette));
        scene.gpu_vertices();
        Arc::new(scene)
    }))
}

fn add_elevated_edges(
    scene: &mut Scene,
    geometry: &opencity::road_geometry::RoadGeometry,
    index: usize,
    color: Color32,
) {
    for wall in geometry.walls.iter().filter(|w| w.segment == index) {
        let prior = WorldVertex {
            point: wall.start.point,
            height: wall.start.height + 0.005,
            depth_bias: 0.0,
            color,
        };
        let current = WorldVertex {
            point: wall.end.point,
            height: wall.end.height + 0.005,
            depth_bias: 0.0,
            color,
        };
        let prior_bottom = WorldVertex {
            height: wall.start.height - 0.08,
            ..prior
        };
        let current_bottom = WorldVertex {
            height: wall.end.height - 0.08,
            ..current
        };
        scene.vertices.extend_from_slice(&[
            prior,
            current,
            current_bottom,
            prior,
            current_bottom,
            prior_bottom,
        ]);
    }
}

/// Render ordinary road bands and their edge/center markings from the same
/// sampled cross-sections as the sidewalls and geometry validator.
fn add_resolved_road_surface(
    scene: &mut Scene,
    geometry: &opencity::road_geometry::RoadGeometry,
    index: usize,
    road: &RoadDefinition,
    profile: &PaintProfile,
    palette: EditorPalette,
    selected: bool,
) {
    let rows = geometry.cross_sections(index);
    let emit = |scene: &mut Scene,
                corners: [opencity::road_geometry::Vertex; 4],
                color: Color32,
                bias: f32| {
        for i in [0, 1, 2, 0, 2, 3] {
            let vertex = corners[i];
            scene.vertices.push(WorldVertex {
                point: vertex.point,
                height: vertex.height + 0.005,
                depth_bias: bias,
                color,
            });
        }
    };
    let between =
        |a: opencity::road_geometry::Vertex, b: opencity::road_geometry::Vertex, t: f32| {
            opencity::road_geometry::Vertex {
                point: WorldPoint {
                    x: a.point.x + (b.point.x - a.point.x) * t,
                    y: a.point.y + (b.point.y - a.point.y) * t,
                },
                height: a.height + (b.height - a.height) * t,
            }
        };
    for pair in rows.windows(2) {
        for (band, element) in road.elements.iter().enumerate() {
            emit(
                scene,
                [
                    pair[0].boundaries[band],
                    pair[1].boundaries[band],
                    pair[1].boundaries[band + 1],
                    pair[0].boundaries[band + 1],
                ],
                kind_color(element.kind, palette.theme),
                0.0,
            );
        }
        let left = [pair[0].boundaries[0], pair[1].boundaries[0]];
        let right = [
            *pair[0].boundaries.last().unwrap(),
            *pair[1].boundaries.last().unwrap(),
        ];
        let width = (left[0].point.x - right[0].point.x)
            .hypot(left[0].point.y - right[0].point.y)
            .max(0.000001);
        let marking = (0.004 / width).min(0.25);
        // The center marking is a coplanar decal, with depth ordering only.
        emit(
            scene,
            [
                between(left[0], right[0], 0.5 - marking),
                between(left[1], right[1], 0.5 - marking),
                between(left[1], right[1], 0.5 + marking),
                between(left[0], right[0], 0.5 + marking),
            ],
            palette.map_centerline,
            -0.003,
        );
        for side in [0, 1] {
            let edge = if side == 0 { left } else { right };
            let midpoint = between(edge[0], edge[1], 0.5).point;
            let hidden = [profile.start_parent.as_ref(), profile.end_parent.as_ref()]
                .into_iter()
                .flatten()
                .any(|parent| {
                    screen_point_in_road(
                        screen_of(GEOMETRY_RECT, egui::Vec2::ZERO, GEOMETRY_SCALE, midpoint),
                        parent,
                        GEOMETRY_RECT,
                        egui::Vec2::ZERO,
                        GEOMETRY_SCALE,
                    )
                });
            if hidden {
                continue;
            }
            let inset = if side == 0 { marking } else { 1.0 - marking };
            emit(
                scene,
                [
                    edge[0],
                    edge[1],
                    between(left[1], right[1], inset),
                    between(left[0], right[0], inset),
                ],
                if selected {
                    palette.map_selected
                } else {
                    palette.map_outline
                },
                -0.003,
            );
        }
    }
}

fn append_road(
    scene: &mut Scene,
    ctx: &egui::Context,
    program: &Program,
    projection: &map::Projection,
    profiles: &[PaintProfile],
    geometry: &opencity::road_geometry::RoadGeometry,
    index: usize,
    palette: EditorPalette,
    selected: &[SegmentSource],
) {
    let first_vertex = scene.vertices.len() as u32;
    let segment = &projection.segments[index];
    let road = program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Road(road) if road.name == segment.road_name => Some(road),
            _ => None,
        });
    let is_selected = selected.contains(&segment.source);
    let samples = std::array::from_fn(|step| segment.point_at(step as f32 / 16.0));
    let heights = std::array::from_fn(|step| segment.height_at(step as f32 / 16.0));
    let shared_surface = road.filter(|road| {
        !road.elements.iter().any(|element| {
            matches!(
                element.kind,
                RoadElementKind::TrainTrack
                    | RoadElementKind::TramTrack
                    | RoadElementKind::SubwayTrack
                    | RoadElementKind::Runway
                    | RoadElementKind::Taxiway
                    | RoadElementKind::Apron
            )
        })
    });
    if let Some(road) = shared_surface {
        add_resolved_road_surface(
            scene,
            geometry,
            index,
            road,
            &profiles[index],
            palette,
            is_selected,
        );
    } else {
        capture_geometry(
            ctx,
            |painter| {
                if let Some(road) = road.filter(|_| {
                    profiles[index].start_alignment.is_some()
                        || profiles[index].merge_alignment.is_some()
                        || !profiles[index].start_taper.is_empty()
                        || !profiles[index].end_taper.is_empty()
                }) {
                    paint_profiled_road(
                        painter,
                        GEOMETRY_RECT,
                        egui::Vec2::ZERO,
                        GEOMETRY_SCALE,
                        segment,
                        road,
                        is_selected,
                        palette,
                        &profiles[index],
                    );
                } else {
                    paint_road_segment(
                        painter,
                        GEOMETRY_RECT,
                        egui::Vec2::ZERO,
                        GEOMETRY_SCALE,
                        segment,
                        road,
                        is_selected,
                        palette,
                    );
                }
            },
            |point| {
                if segment.start_z == segment.end_z {
                    segment.start_z + 0.005
                } else {
                    road_height(&samples, &heights, point) + 0.005
                }
            },
            scene,
        );
    }
    // A declared merge replaces the receiving edge's pavement locally. Keep
    // that overlay ahead of the parent's band/outline layers in every camera.
    if road.is_some_and(|road| road.role == opencity::RoadRole::Merge) {
        for vertex in &mut scene.vertices[first_vertex as usize..] {
            vertex.depth_bias -= 0.02;
        }
    }
    add_elevated_edges(
        scene,
        geometry,
        index,
        palette.map_outline.gamma_multiply(0.75),
    );
    let end_vertex = scene.vertices.len() as u32;
    if first_vertex < end_vertex {
        let mut min = WorldPoint {
            x: f32::INFINITY,
            y: f32::INFINITY,
        };
        let mut max = WorldPoint {
            x: f32::NEG_INFINITY,
            y: f32::NEG_INFINITY,
        };
        let mut min_height = f32::INFINITY;
        let mut max_height = f32::NEG_INFINITY;
        for vertex in &scene.vertices[first_vertex as usize..end_vertex as usize] {
            min.x = min.x.min(vertex.point.x);
            min.y = min.y.min(vertex.point.y);
            max.x = max.x.max(vertex.point.x);
            max.y = max.y.max(vertex.point.y);
            min_height = min_height.min(vertex.height);
            max_height = max_height.max(vertex.height);
        }
        scene.road_ranges.push(RoadDrawRange {
            vertices: first_vertex..end_vertex,
            min,
            max,
            min_height,
            max_height,
        });
        let span = scene.road_span.get_or_insert(first_vertex..end_vertex);
        span.end = end_vertex;
    }
}

#[cfg(test)]
pub fn build_scene(
    program: &Program,
    projection: &map::Projection,
    palette: EditorPalette,
    selected: &[SegmentSource],
    selected_landscape: Option<&str>,
) -> Scene {
    build_scene_with_model_base(
        program,
        projection,
        palette,
        selected,
        selected_landscape,
        None,
    )
}

pub fn build_scene_with_model_base(
    program: &Program,
    projection: &map::Projection,
    palette: EditorPalette,
    selected: &[SegmentSource],
    selected_landscape: Option<&str>,
    model_base: Option<&Path>,
) -> Scene {
    #[cfg(test)]
    let started = std::time::Instant::now();
    let mut scene = Scene::default();
    let ctx = egui::Context::default();
    let profiles = paint_profiles(program, projection);
    let geometry = opencity::road_geometry::RoadGeometry::compile(program, projection);
    let mut features = project_landscape_features(program).features;
    if !features
        .iter()
        .any(|feature| matches!(feature.kind, LandscapeKind::Land | LandscapeKind::Beach))
    {
        scene.vertices.extend_from_slice(grid_vertices(palette));
    }
    features.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
    append_landscape(
        &mut scene,
        &ctx,
        &features,
        palette,
        selected_landscape,
        model_base,
    );
    #[cfg(test)]
    let (before_roads, landscape_time) = (scene.vertices.len(), started.elapsed());
    for index in road_paint_order(program, projection, &profiles) {
        append_road(
            &mut scene, &ctx, program, projection, &profiles, &geometry, index, palette, selected,
        );
    }
    #[cfg(test)]
    let (before_intersections, roads_time) = (scene.vertices.len(), started.elapsed());
    for intersection in map::intersections(program, projection) {
        capture_geometry(
            &ctx,
            |painter| {
                paint_intersection(
                    painter,
                    GEOMETRY_RECT,
                    egui::Vec2::ZERO,
                    GEOMETRY_SCALE,
                    program,
                    projection,
                    &intersection,
                    palette,
                );
            },
            |_| 0.02,
            &mut scene,
        );
    }
    #[cfg(test)]
    if std::env::var_os("OPENCITY_SCENE_PROFILE").is_some() {
        eprintln!("scene parts: terrain={} vertices/{:?}, roads={} vertices/{:?}, intersections={} vertices/{:?}",
            before_roads, landscape_time,
            before_intersections - before_roads, roads_time - landscape_time,
            scene.vertices.len() - before_intersections, started.elapsed() - roads_time);
    }
    // Convert vertices while the scene builder still runs on its worker thread.
    scene.gpu_vertices();
    scene
}

// The browser has one execution thread. Spread a rebuild across animation
// frames so a large map never holds the UI thread for the entire scene build.
#[cfg(any(target_arch = "wasm32", test))]
pub struct WebSceneBuild {
    pub program: Program,
    pub theme: egui::Theme,
    pub selected_landscape: Option<String>,
    projection: map::Projection,
    palette: EditorPalette,
    profiles: Vec<PaintProfile>,
    geometry: opencity::road_geometry::RoadGeometry,
    road_order: Vec<usize>,
    intersections: Vec<map::Intersection>,
    landscape: Vec<opencity::LandscapeFeature>,
    landscape_index: usize,
    road_index: usize,
    intersection_index: usize,
    gpu_build: Vec<GpuVertex>,
    gpu_index: usize,
    scene: Scene,
    ctx: egui::Context,
}

#[cfg(any(target_arch = "wasm32", test))]
impl WebSceneBuild {
    pub fn new(
        program: Program,
        projection: map::Projection,
        palette: EditorPalette,
        selected_landscape: Option<String>,
    ) -> Self {
        let ctx = egui::Context::default();
        let profiles = paint_profiles(&program, &projection);
        let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
        let road_order = road_paint_order(&program, &projection, &profiles);
        let intersections = map::intersections(&program, &projection);
        let mut landscape = project_landscape_features(&program).features;
        let mut scene = Scene::default();
        if !landscape
            .iter()
            .any(|feature| matches!(feature.kind, LandscapeKind::Land | LandscapeKind::Beach))
        {
            scene.vertices.extend_from_slice(grid_vertices(palette));
        }
        landscape.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
        Self {
            program,
            theme: palette.theme,
            selected_landscape,
            projection,
            palette,
            profiles,
            geometry,
            road_order,
            intersections,
            landscape,
            landscape_index: 0,
            road_index: 0,
            intersection_index: 0,
            gpu_build: Vec::new(),
            gpu_index: 0,
            scene,
            ctx,
        }
    }

    // Returns a complete scene when the frame budget permits. A partially
    // built scene is never handed to the GPU; the prior scene stays visible.
    pub fn advance(&mut self, budget: std::time::Duration) -> bool {
        #[cfg(target_arch = "wasm32")]
        let started = web_time::Instant::now();
        #[cfg(not(target_arch = "wasm32"))]
        let started = std::time::Instant::now();
        while self.landscape_index < self.landscape.len() {
            append_landscape_feature(
                &mut self.scene,
                &self.ctx,
                &self.landscape[self.landscape_index],
                &self.landscape,
                self.palette,
                self.selected_landscape.as_deref(),
                None,
            );
            self.landscape_index += 1;
            if started.elapsed() >= budget {
                return false;
            }
        }
        while self.road_index < self.road_order.len() {
            let index = self.road_order[self.road_index];
            append_road(
                &mut self.scene,
                &self.ctx,
                &self.program,
                &self.projection,
                &self.profiles,
                &self.geometry,
                index,
                self.palette,
                &[],
            );
            self.road_index += 1;
            if started.elapsed() >= budget {
                return false;
            }
        }
        while self.intersection_index < self.intersections.len() {
            let intersection = &self.intersections[self.intersection_index];
            capture_geometry(
                &self.ctx,
                |painter| {
                    paint_intersection(
                        painter,
                        GEOMETRY_RECT,
                        egui::Vec2::ZERO,
                        GEOMETRY_SCALE,
                        &self.program,
                        &self.projection,
                        intersection,
                        self.palette,
                    )
                },
                |_| 0.02,
                &mut self.scene,
            );
            self.intersection_index += 1;
            if started.elapsed() >= budget {
                return false;
            }
        }
        // Upload preparation is CPU work too. Convert in bounded batches so
        // completing a large scene does not freeze a browser frame.
        while self.gpu_index < self.scene.vertices.len() {
            let end = (self.gpu_index + 8192).min(self.scene.vertices.len());
            self.gpu_build.extend(
                self.scene.vertices[self.gpu_index..end]
                    .iter()
                    .map(gpu_vertex_from_world),
            );
            self.gpu_index = end;
            if started.elapsed() >= budget {
                return false;
            }
        }
        true
    }

    pub fn finish(self) -> Scene {
        assert_eq!(self.gpu_index, self.scene.vertices.len());
        assert!(self.scene.gpu_vertices.set(self.gpu_build).is_ok());
        self.scene
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GpuVertex {
    world: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CameraUniform {
    view: [f32; 4],
    target: [f32; 4],
    viewport: [f32; 4],
    elevation: [f32; 4],
}

fn bytes_of<T>(values: &[T]) -> &[u8] {
    // GpuVertex and CameraUniform contain only initialized f32 fields.
    // wgpu consumes these bytes without interpreting them as Rust objects.
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

pub struct SceneCallback {
    pub scene: Arc<Scene>,
    pub rect: egui::Rect,
    pub pan: egui::Vec2,
    pub zoom: f32,
    pub target: WorldPoint,
    pub angle: f32,
    pub elevation: f32,
    pub eye_height: f32,
    pub camera: CameraView,
    pub background: Color32,
    pub target_format: wgpu::TextureFormat,
}

struct RenderResources {
    scene_pipeline: wgpu::RenderPipeline,
    close_zoom_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    scene_buffer: Option<(Arc<Scene>, wgpu::Buffer)>,
    target: Option<RenderTarget>,
    composite_pipeline: wgpu::RenderPipeline,
    composite_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    target_format: wgpu::TextureFormat,
}
struct FrameResources {
    vertex_buffer: wgpu::Buffer,
    vertices: u32,
    color: wgpu::TextureView,
    multisampled_color: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}
struct RenderTarget {
    width: u32,
    height: u32,
    samples: u32,
    color: wgpu::TextureView,
    multisampled_color: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

const SCENE_SHADER: &str = include_str!("shaders/scene.wgsl");
const COMPOSITE_SHADER: &str = include_str!("shaders/composite.wgsl");

impl SceneCallback {
    fn road_is_visible(&self, road: &RoadDrawRange) -> bool {
        if self.camera == CameraView::Vehicular {
            return true;
        }
        let mut min = egui::pos2(f32::INFINITY, f32::INFINITY);
        let mut max = egui::pos2(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for x in [road.min.x, road.max.x] {
            for y in [road.min.y, road.max.y] {
                for height in [road.min_height, road.max_height] {
                    let point = WorldPoint { x, y };
                    let screen = if self.camera == CameraView::Top {
                        screen_of(self.rect, self.pan, self.zoom, point)
                    } else {
                        iso_screen(
                            self.rect,
                            self.pan,
                            self.zoom,
                            self.target,
                            self.angle,
                            self.elevation,
                            point,
                            height,
                        )
                    };
                    min.x = min.x.min(screen.x);
                    min.y = min.y.min(screen.y);
                    max.x = max.x.max(screen.x);
                    max.y = max.y.max(screen.y);
                }
            }
        }
        egui::Rect::from_min_max(min, max).intersects(self.rect.expand(2.0))
    }
}

impl egui_wgpu::CallbackTrait for SceneCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let width = (self.rect.width() * screen.pixels_per_point)
            .round()
            .max(1.0) as u32;
        let height = (self.rect.height() * screen.pixels_per_point)
            .round()
            .max(1.0) as u32;
        let samples = if self.camera == CameraView::Vehicular || self.zoom <= MULTISAMPLE_MAX_ZOOM {
            SCENE_SAMPLES
        } else {
            1
        };
        if resources
            .get::<RenderResources>()
            .is_none_or(|r| r.target_format != self.target_format)
        {
            resources.insert(RenderResources::new(device, self.target_format));
        }
        let renderer = resources.get_mut::<RenderResources>().unwrap();
        if renderer
            .scene_buffer
            .as_ref()
            .is_none_or(|(scene, _)| !Arc::ptr_eq(scene, &self.scene))
        {
            let vertices = self.scene.gpu_vertices();
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("opencity scene vertices"),
                contents: bytes_of(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            renderer.scene_buffer = Some((Arc::clone(&self.scene), buffer));
        }
        let camera = CameraUniform {
            view: [
                self.pan.x,
                self.pan.y,
                self.zoom,
                match self.camera {
                    CameraView::Top => 0.0,
                    CameraView::Isometric => 1.0,
                    CameraView::Vehicular => 2.0,
                },
            ],
            target: [
                self.target.x,
                self.target.y,
                self.angle.sin(),
                self.angle.cos(),
            ],
            viewport: [
                self.rect.width(),
                self.rect.height(),
                self.elevation.sin(),
                self.elevation.cos(),
            ],
            elevation: [self.elevation.tan(), self.eye_height, 0.0, 0.0],
        };
        queue.write_buffer(
            &renderer.camera_buffer,
            0,
            bytes_of(std::slice::from_ref(&camera)),
        );
        if renderer.target.as_ref().is_none_or(|target| {
            target.width != width || target.height != height || target.samples != samples
        }) {
            let size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            let color_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("opencity scene color"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let multisampled_color = (samples > 1).then(|| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("opencity multisampled scene color"),
                        size,
                        mip_level_count: 1,
                        sample_count: samples,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            });
            let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("opencity scene depth"),
                size,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let color = color_texture.create_view(&Default::default());
            let depth = depth_texture.create_view(&Default::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("opencity scene image"),
                layout: &renderer.composite_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&color),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&renderer.sampler),
                    },
                ],
            });
            renderer.target = Some(RenderTarget {
                width,
                height,
                samples,
                color,
                multisampled_color,
                depth,
                bind_group,
            });
        }
        let target = renderer.target.as_ref().unwrap();
        {
            let rgba = self.background.to_array();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("opencity depth-tested scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.multisampled_color.as_ref().unwrap_or(&target.color),
                    resolve_target: target.multisampled_color.as_ref().map(|_| &target.color),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: rgba[0] as f64 / 255.0,
                            g: rgba[1] as f64 / 255.0,
                            b: rgba[2] as f64 / 255.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(if samples == 1 {
                &renderer.close_zoom_pipeline
            } else {
                &renderer.scene_pipeline
            });
            pass.set_bind_group(0, &renderer.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, renderer.scene_buffer.as_ref().unwrap().1.slice(..));
            if let Some(span) = &self.scene.road_span {
                if span.start > 0 {
                    pass.draw(0..span.start, 0..1);
                }
                if samples == SCENE_SAMPLES {
                    pass.draw(span.clone(), 0..1);
                } else {
                    let mut visible: Option<Range<u32>> = None;
                    for road in &self.scene.road_ranges {
                        if self.road_is_visible(road) {
                            if let Some(range) = &mut visible {
                                if range.end == road.vertices.start {
                                    range.end = road.vertices.end;
                                    continue;
                                }
                                pass.draw(range.clone(), 0..1);
                            }
                            visible = Some(road.vertices.clone());
                        }
                    }
                    if let Some(range) = visible {
                        pass.draw(range, 0..1);
                    }
                }
                if span.end < self.scene.vertices.len() as u32 {
                    pass.draw(span.end..self.scene.vertices.len() as u32, 0..1);
                }
            } else {
                pass.draw(0..self.scene.vertices.len() as u32, 0..1);
            }
        }
        let vertex_buffer = renderer.scene_buffer.as_ref().unwrap().1.clone();
        let color = target.color.clone();
        let multisampled_color = target.multisampled_color.clone();
        let depth = target.depth.clone();
        let bind_group = target.bind_group.clone();
        resources.insert(FrameResources {
            vertex_buffer,
            vertices: self.scene.vertices.len() as u32,
            color,
            multisampled_color,
            depth,
            bind_group,
        });
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::epaint::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let renderer = resources.get::<RenderResources>().unwrap();
        let frame = resources.get::<FrameResources>().unwrap();
        let _keep_alive = (
            &frame.vertex_buffer,
            &frame.color,
            &frame.multisampled_color,
            &frame.depth,
            frame.vertices,
        );
        pass.set_pipeline(&renderer.composite_pipeline);
        pass.set_bind_group(0, &frame.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}

impl RenderResources {
    fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let scene_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("opencity scene shader"),
            source: wgpu::ShaderSource::Wgsl(SCENE_SHADER.into()),
        });
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("opencity composite shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("opencity camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("opencity camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("opencity camera binding"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 2,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("opencity scene layout"),
            bind_group_layouts: &[Some(&camera_layout)],
            immediate_size: 0,
        });
        let make_scene_pipeline = |samples| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("opencity scene pipeline"),
                layout: Some(&scene_layout),
                vertex: wgpu::VertexState {
                    module: &scene_shader,
                    entry_point: Some("scene_vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<GpuVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: samples,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &scene_shader,
                    entry_point: Some("scene_fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let scene_pipeline = make_scene_pipeline(SCENE_SAMPLES);
        let close_zoom_pipeline = make_scene_pipeline(1);
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("opencity composite image layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let composite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("opencity composite layout"),
                bind_group_layouts: &[Some(&composite_layout)],
                immediate_size: 0,
            });
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("opencity composite pipeline"),
            layout: Some(&composite_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &composite_shader,
                entry_point: Some("composite_vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &composite_shader,
                entry_point: Some(if target_format.is_srgb() {
                    "composite_fs_linear"
                } else {
                    "composite_fs"
                }),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("opencity scene sampler"),
            // The resolved scene already matches the viewport's pixel size.
            // Linear sampling would soften it a second time during compositing.
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self {
            scene_pipeline,
            close_zoom_pipeline,
            camera_buffer,
            camera_bind_group,
            scene_buffer: None,
            target: None,
            composite_pipeline,
            composite_layout,
            sampler,
            target_format,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_map_edges_have_no_world_space_feather() {
        let mut scene = Scene::default();
        capture_geometry(
            &egui::Context::default(),
            |painter| {
                painter.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(-50.0, -50.0), egui::pos2(50.0, 50.0)),
                    0.0,
                    Color32::WHITE,
                );
            },
            |_| 0.0,
            &mut scene,
        );
        assert!(!scene.vertices.is_empty());
        assert!(scene.vertices.iter().all(|vertex| vertex.color.a() == 255));
    }

    #[test]
    fn close_zoom_culls_roads_outside_the_camera() {
        let program = starter_program();
        let projection = map::project(&program);
        let scene = Arc::new(build_scene(
            &program,
            &projection,
            editor_palette(egui::Theme::Dark),
            &[],
            None,
        ));
        let first = &scene.road_ranges[0];
        let center = WorldPoint {
            x: (first.min.x + first.max.x) * 0.5,
            y: (first.min.y + first.max.y) * 0.5,
        };
        let mut callback = SceneCallback {
            scene: Arc::clone(&scene),
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(512.0, 512.0)),
            pan: egui::vec2(-center.x * 1000.0, center.y * 1000.0),
            zoom: 1000.0,
            target: WorldPoint { x: 0.0, y: 0.0 },
            angle: 0.0,
            elevation: ISO_ELEVATION,
            eye_height: 0.0,
            camera: CameraView::Top,
            background: Color32::BLACK,
            target_format: wgpu::TextureFormat::Bgra8Unorm,
        };
        let visible = scene
            .road_ranges
            .iter()
            .filter(|road| callback.road_is_visible(road))
            .count();
        assert!(visible > 0);
        assert!(visible < scene.road_ranges.len());
        let span = scene.road_span.as_ref().unwrap();
        assert_eq!(
            scene.road_ranges.first().unwrap().vertices.start,
            span.start
        );
        assert_eq!(scene.road_ranges.last().unwrap().vertices.end, span.end);
        callback.camera = CameraView::Isometric;
        callback.target = center;
        callback.pan = egui::Vec2::ZERO;
        callback.angle = 0.5;
        let visible_isometric = scene
            .road_ranges
            .iter()
            .filter(|road| callback.road_is_visible(road))
            .count();
        assert!(visible_isometric > 0);
        assert!(visible_isometric < scene.road_ranges.len());
    }

    #[test]
    fn connector_wall_tops_are_actual_pavement_vertices() {
        let program = parse(include_str!("../../../road_library/cloverleaf.road")).unwrap();
        let projection = map::project(&program);
        let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
        let profiles = paint_profiles(&program, &projection);
        let index = projection
            .segments
            .iter()
            .position(|s| s.name.as_deref() == Some("ne_connector_down"))
            .unwrap();
        let road = road_for_segment(&program, &projection.segments[index]).unwrap();
        let mut surface = Scene::default();
        add_resolved_road_surface(
            &mut surface,
            &geometry,
            index,
            road,
            &profiles[index],
            editor_palette(egui::Theme::Dark),
            false,
        );
        let mut walls = Scene::default();
        add_elevated_edges(&mut walls, &geometry, index, Color32::WHITE);
        assert!(!walls.vertices.is_empty());
        for face in walls.vertices.chunks_exact(6) {
            for top in [&face[0], &face[1]] {
                assert!(surface.vertices.iter().any(|v|v.point==top.point && v.height==top.height),"wall top must meet the emitted pavement without height interpolation or offsets");
            }
        }
    }

    #[test]
    fn elevated_library_walls_use_shared_geometry() {
        let program = parse(include_str!(
            "../../../road_library/street_to_interstate_elevated.road"
        ))
        .unwrap();
        let projection = map::project(&program);
        let geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
        let mut scene = Scene::default();
        for index in 0..projection.segments.len() {
            add_elevated_edges(&mut scene, &geometry, index, Color32::WHITE);
        }
        assert_eq!(scene.vertices.len(), geometry.walls.len() * 6);
        assert!(
            geometry.obstructions().is_empty(),
            "{:?}",
            geometry.obstructions()
        );
    }

    #[test]
    fn coastal_scene_keeps_water_free_of_editor_grid() {
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            let palette = editor_palette(theme);
            assert_eq!(palette.map_background, Color32::from_rgb(7, 112, 172));
            let program = parse("land(0, 0, 2, 0, 2, 2, 0, 2);").unwrap();
            let projection = map::project(&program);
            let scene = build_scene(&program, &projection, palette, &[], None);
            assert!(!scene
                .vertices
                .iter()
                .any(|vertex| vertex.depth_bias == 100.0));
        }
    }

    #[test]
    fn overlapping_ground_surfaces_have_stable_depth_order() {
        let program = parse(
            "beach(0, 0, 3, 0, 3, 3, 0, 3); land(0, 0, 3, 0, 3, 3, 0, 3); vegetation(forest, 0, 0, 3, 0, 3, 3, 0, 3); concrete(0, 0, 3, 0, 3, 3, 0, 3);",
        )
        .unwrap();
        let palette = editor_palette(egui::Theme::Dark);
        let scene = build_scene(&program, &map::project(&program), palette, &[], None);
        let fill_bias = |color| {
            scene
                .vertices
                .iter()
                .find(|vertex| vertex.color == color)
                .expect("landscape fill exists")
                .depth_bias
        };
        let beach = fill_bias(Color32::from_rgb(244, 219, 159));
        let land = fill_bias(Color32::from_rgb(105, 158, 75));
        let vegetation = fill_bias(Color32::from_rgb(61, 128, 59));
        let concrete = fill_bias(Color32::from_rgb(125, 139, 145));
        assert!(beach > land && land > vegetation && vegetation > concrete);
        assert!(beach - land >= 0.01 && land - vegetation >= 0.01);
        assert!(concrete > 0.0, "roads at zero bias stay above ground cover");
    }

    #[test]
    fn overlapping_terrain_outlines_expose_both_mountain_levels() {
        let program = parse("lower = terrain(0.2, 0, 0, 2, 0, 2, 2, 0, 2); upper = terrain(0.4, 1, 1, 3, 1, 3, 3, 1, 3);").unwrap();
        let features = project_landscape_features(&program).features;
        let mut scene = Scene::default();
        add_terrain_walls(
            &mut scene,
            &features[1],
            &features,
            editor_palette(egui::Theme::Dark),
        );
        assert!(scene.vertices.iter().any(|vertex| vertex.height == 0.0));
        assert!(scene.vertices.iter().any(|vertex| vertex.height == 0.2));
        assert!(scene.vertices.iter().any(|vertex| vertex.height == 0.4));
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn referenced_glb_replaces_building_extrusion_in_scene() {
        let source = opencity::parse("building(0.5, 0, 0, 1, 0, 1, 1, 0, 1);").unwrap();
        let bytes = glb::export(&source);
        let directory =
            std::env::temp_dir().join(format!("opencity-glb-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("test.glb"), bytes).unwrap();
        let program = parse("hall = building(0.8, \"test.glb\", 2, 2, 4, 2, 4, 4, 2, 4);").unwrap();
        let projection = map::project(&program);
        let scene = build_scene_with_model_base(
            &program,
            &projection,
            editor_palette(egui::Theme::Dark),
            &[],
            None,
            Some(&directory),
        );
        assert!(scene.vertices.iter().any(|vertex| vertex.color
            == Color32::from_rgb(158, 166, 172)
            && vertex.point.x >= 2.0
            && vertex.point.x <= 4.0
            && vertex.height > 0.0
            && vertex.height <= 0.8));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn incremental_web_scene_matches_full_scene() {
        let program = boot_program();
        let projection = map::project(&program);
        let palette = editor_palette(egui::Theme::Dark);
        let full = build_scene(&program, &projection, palette, &[], None);
        let mut incremental = WebSceneBuild::new(program, projection, palette, None);
        assert_eq!(incremental.theme, egui::Theme::Dark);
        assert_eq!(incremental.selected_landscape, None);
        let mut passes = 0;
        while !incremental.advance(std::time::Duration::from_millis(1)) {
            passes += 1;
            assert!(passes < 1000, "scene builder did not finish");
        }
        let incremental = incremental.finish();
        assert_eq!(incremental.vertices, full.vertices);
        assert_eq!(incremental.gpu_vertices().len(), full.gpu_vertices().len());
        assert_eq!(incremental.road_span, full.road_span);
        assert_eq!(incremental.road_ranges.len(), full.road_ranges.len());
        for (built, expected) in incremental.road_ranges.iter().zip(&full.road_ranges) {
            assert_eq!(built.vertices, expected.vertices);
            assert_eq!((built.min, built.max), (expected.min, expected.max));
            assert_eq!(
                (built.min_height, built.max_height),
                (expected.min_height, expected.max_height)
            );
        }
    }

    #[test]
    #[ignore = "manual browser rebuild budget timing"]
    fn profile_web_scene_slices() {
        let program = boot_program();
        let projection = map::project(&program);
        let palette = editor_palette(egui::Theme::Dark);
        let _ = grid_vertices(palette);
        let start = std::time::Instant::now();
        let mut builder = WebSceneBuild::new(program, projection, palette, None);
        let setup = start.elapsed();
        let mut slices = Vec::new();
        loop {
            let start = std::time::Instant::now();
            let complete = builder.advance(std::time::Duration::from_millis(6));
            slices.push(start.elapsed());
            if complete {
                break;
            }
        }
        eprintln!(
            "web scene setup={setup:?}, slices={slices:?}, vertices={}",
            builder.scene.vertices.len()
        );
    }

    #[test]
    #[ignore = "manual CPU scene timing"]
    fn profile_target_scene_cpu() {
        let program = boot_program();
        let projection = map::project(&program);
        eprintln!(
            "straight segments={}/{}",
            projection
                .segments
                .iter()
                .filter(|segment| segment_is_straight(segment))
                .count(),
            projection.segments.len()
        );
        let bins = [0.01, 0.05, 0.1, 0.25, 0.5, 1.0, f32::INFINITY];
        eprintln!(
            "curve deviation bins={:?}",
            bins.map(|limit| projection
                .segments
                .iter()
                .filter(|segment| {
                    let dx = segment.end.x - segment.start.x;
                    let dy = segment.end.y - segment.start.y;
                    let length = (dx * dx + dy * dy).sqrt().max(0.00001);
                    segment
                        .controls
                        .iter()
                        .map(|point| {
                            ((point.x - segment.start.x) * dy - (point.y - segment.start.y) * dx)
                                .abs()
                                / length
                        })
                        .fold(0.0, f32::max)
                        <= limit
                })
                .count())
        );
        let start = std::time::Instant::now();
        let scene = build_scene(
            &program,
            &projection,
            editor_palette(egui::Theme::Dark),
            &[],
            None,
        );
        eprintln!(
            "scene build={:?} vertices={}",
            start.elapsed(),
            scene.vertices.len()
        );
        let start = std::time::Instant::now();
        let scene = build_scene(
            &program,
            &projection,
            editor_palette(egui::Theme::Dark),
            &[],
            None,
        );
        eprintln!(
            "warm scene build={:?} vertices={}",
            start.elapsed(),
            scene.vertices.len()
        );
    }

    #[test]
    fn airport_surfaces_and_markings_reach_shared_scene() {
        let source = "road strip: runway; end; road connector: taxiway; end; road stand: apron; end; strip(0, 0, 3, 0); connector(0, 1, 3, 1); stand(0, 2, 3, 2);";
        let program = parse(source).unwrap();
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
        let scene = build_scene(
            &program,
            &map::project(&program),
            editor_palette(egui::Theme::Dark),
            &[],
            None,
        );
        for color in [
            kind_color(RoadElementKind::Runway, egui::Theme::Dark),
            kind_color(RoadElementKind::Taxiway, egui::Theme::Dark),
            kind_color(RoadElementKind::Apron, egui::Theme::Dark),
            Color32::from_rgb(235, 236, 224),
            Color32::from_rgb(233, 191, 73),
        ] {
            assert!(
                scene.vertices.iter().any(|vertex| {
                    vertex.color.r() == color.r()
                        && vertex.color.g() == color.g()
                        && vertex.color.b() == color.b()
                }),
                "missing {color:?}"
            );
        }
    }

    #[test]
    fn shared_scene_contains_detailed_starter_roads_landscape_and_junctions() {
        let program = starter_program();
        let projection = map::project(&program);
        let palette = editor_palette(egui::Theme::Dark);
        let scene = build_scene(&program, &projection, palette, &[], None);
        assert!(
            scene.vertices.len() > 80_000,
            "{} scene vertices",
            scene.vertices.len()
        );
        assert!(scene
            .vertices
            .iter()
            .any(|vertex| vertex.color == kind_color(RoadElementKind::Sidewalk, palette.theme)));
        assert!(scene.vertices.iter().any(|vertex| vertex.height > 0.1));
        assert!(scene.vertices.iter().any(|vertex| vertex.height < -0.1));
        for intersection in map::intersections(&program, &projection) {
            let half = 0.5;
            assert!(
                scene.vertices.iter().any(|vertex| {
                    (vertex.point.x - intersection.position.x).abs() < half
                        && (vertex.point.y - intersection.position.y).abs() < half
                        && vertex.height > 0.015
                        && vertex.height < 0.04
                }),
                "missing intersection {}",
                intersection.point_ref
            );
        }
    }
}

#[cfg(test)]
mod gpu_tests {
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
        let Ok(adapter) =
            block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            eprintln!("Skipping GPU readback: no wgpu adapter is available");
            return None;
        };
        let Ok((device, queue)) =
            block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
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
            include_str!("../../../road_library/street_to_interstate_flat.road"),
            include_str!("../../../road_library/street_to_interstate_elevated.road"),
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
                        assert!(pixel[0]>120 && (pixel[0] as i32-pixel[1] as i32).abs()<12 && (pixel[1] as i32-pixel[2] as i32).abs()<12,"merge must show gray pavement, not green verge/dark outline: {pixel:?}");
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
            parse(include_str!("../../../road_library/qa_gallery.road")).unwrap()
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
            let rect = egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(WIDTH as f32, HEIGHT as f32),
            );
            let pan = if camera == CameraView::Top {
                egui::vec2(-target.x * zoom, target.y * zoom)
            } else if qa_library && camera == CameraView::Isometric {
                egui::vec2(0.0, 300.0)
            } else {
                egui::Vec2::ZERO
            };
            let callback = SceneCallback {
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
}
