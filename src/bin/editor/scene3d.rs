//! Shared world geometry for the top and isometric cameras. The detailed map
//! painters are tessellated in world space, then drawn with wgpu depth testing.
use super::*;
use eframe::{egui_wgpu, wgpu};
use std::sync::{Arc, Mutex, OnceLock};
use std::{collections::HashMap, path::Path};
use wgpu::util::DeviceExt;

const GEOMETRY_SCALE: f32 = 100.0;
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
    gpu_vertices: std::sync::OnceLock<Vec<GpuVertex>>,
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

#[cfg(target_arch = "wasm32")]
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
    segment: &map::Segment,
    road: Option<&RoadDefinition>,
    profile: &PaintProfile,
    color: Color32,
) {
    if segment.start_z.max(segment.end_z) <= 0.05 {
        return;
    }
    for side in [0, 1] {
        let mut previous = None;
        let steps = road_curve_samples(segment);
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let center = segment.point_at(t);
            let tangent = segment.tangent_at(t);
            let length = (tangent.x * tangent.x + tangent.y * tangent.y)
                .sqrt()
                .max(0.000001);
            let (left, right) = road
                .map(|road| section_edges(road, profile, t))
                .unwrap_or((-0.08, 0.08));
            let offset = if side == 0 { left } else { right };
            let point = WorldPoint {
                x: center.x - tangent.y / length * offset,
                y: center.y + tangent.x / length * offset,
            };
            let height = segment.height_at(t);
            if let Some((prior_point, prior_height)) = previous {
                if height.max(prior_height) > 0.05 {
                    let current = WorldVertex {
                        point,
                        height: height + 0.003,
                        depth_bias: 0.0,
                        color,
                    };
                    let prior = WorldVertex {
                        point: prior_point,
                        height: prior_height + 0.003,
                        depth_bias: 0.0,
                        color,
                    };
                    let current_bottom = WorldVertex {
                        height: height - 0.08,
                        ..current
                    };
                    let prior_bottom = WorldVertex {
                        height: prior_height - 0.08,
                        ..prior
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
            previous = Some((point, height));
        }
    }
}

fn append_road(
    scene: &mut Scene,
    ctx: &egui::Context,
    program: &Program,
    projection: &map::Projection,
    profiles: &[PaintProfile],
    index: usize,
    palette: EditorPalette,
    selected: &[SegmentSource],
) {
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
    add_elevated_edges(
        scene,
        segment,
        road,
        &profiles[index],
        palette.map_outline.gamma_multiply(0.75),
    );
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
            &mut scene, &ctx, program, projection, &profiles, index, palette, selected,
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
    pub camera: CameraView,
    pub background: Color32,
    pub target_format: wgpu::TextureFormat,
}

struct RenderResources {
    scene_pipeline: wgpu::RenderPipeline,
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
    depth: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}
struct RenderTarget {
    width: u32,
    height: u32,
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

const SCENE_SHADER: &str = include_str!("shaders/scene.wgsl");
const COMPOSITE_SHADER: &str = include_str!("shaders/composite.wgsl");

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
                if self.camera == CameraView::Isometric {
                    1.0
                } else {
                    0.0
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
                ISO_ELEVATION.sin(),
                ISO_ELEVATION.cos(),
            ],
            elevation: [ISO_ELEVATION.tan(), 0.0, 0.0, 0.0],
        };
        queue.write_buffer(
            &renderer.camera_buffer,
            0,
            bytes_of(std::slice::from_ref(&camera)),
        );
        if renderer
            .target
            .as_ref()
            .is_none_or(|target| target.width != width || target.height != height)
        {
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
            let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("opencity scene depth"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth24Plus,
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
                color,
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
                    view: &target.color,
                    resolve_target: None,
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
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&renderer.scene_pipeline);
            pass.set_bind_group(0, &renderer.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, renderer.scene_buffer.as_ref().unwrap().1.slice(..));
            pass.draw(0..self.scene.vertices.len() as u32, 0..1);
        }
        let vertex_buffer = renderer.scene_buffer.as_ref().unwrap().1.clone();
        let color = target.color.clone();
        let depth = target.depth.clone();
        let bind_group = target.bind_group.clone();
        resources.insert(FrameResources {
            vertex_buffer,
            vertices: self.scene.vertices.len() as u32,
            color,
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
        let scene_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
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
        });
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
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            scene_pipeline,
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
            "beach(0, 0, 3, 0, 3, 3, 0, 3); land(0, 0, 3, 0, 3, 3, 0, 3); vegetation(forest, 0, 0, 3, 0, 3, 3, 0, 3);",
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
        assert!(beach > land && land > vegetation);
        assert!(beach - land >= 0.01 && land - vegetation >= 0.01);
        assert!(
            vegetation > 0.0,
            "roads at zero bias stay above ground cover"
        );
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
            scene.vertices.len() > 100_000,
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

    #[test]
    fn wgpu_scene_pipelines_validate_when_an_adapter_is_available() {
        let instance = wgpu::Instance::default();
        let Ok(adapter) =
            block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            eprintln!("Skipping GPU readback: no wgpu adapter is available");
            return;
        };
        let Ok((device, queue)) =
            block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        else {
            eprintln!("Skipping GPU readback: no wgpu device is available");
            return;
        };
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _resources = RenderResources::new(&device, wgpu::TextureFormat::Bgra8Unorm);
        let _srgb_resources = RenderResources::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut resources = egui_wgpu::CallbackResources::default();
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
            camera: CameraView::Top,
            background: Color32::BLACK,
            target_format: wgpu::TextureFormat::Bgra8Unorm,
        };
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
        let center = &pixels[(64 * 128 + 64) * 4..(64 * 128 + 64) * 4 + 4];
        let corner = &pixels[0..4];
        assert_eq!(block_on(scope.pop()).map(|error| error.to_string()), None);
        assert!(
            center[2] > 180 && center[1] < 80 && center[0] < 80,
            "center BGRA should be red: {center:?}"
        );
        assert!(
            corner[0] < 10 && corner[1] < 10 && corner[2] < 10,
            "corner BGRA should be black: {corner:?}"
        );
        drop(pixels);
        readback.unmap();
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
        let program = if qa_target {
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
        for (name, camera, target, zoom) in views {
            let rect = egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(WIDTH as f32, HEIGHT as f32),
            );
            let pan = if camera == CameraView::Top {
                egui::vec2(-target.x * zoom, target.y * zoom)
            } else {
                egui::Vec2::ZERO
            };
            let callback = SceneCallback {
                scene: Arc::clone(&scene),
                rect,
                pan,
                zoom,
                target,
                angle: 0.74,
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
            let prefix = if qa_target { "target" } else { "starter" };
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
