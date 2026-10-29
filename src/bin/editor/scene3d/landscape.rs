//! Landscape surfaces, vertical walls and referenced GLB model assets.
use super::*;

fn add_building_walls(
    scene: &mut Scene,
    feature: &opencity::LandscapeFeature,
    _palette: EditorPalette,
) {
    let Some(height) = feature.height else {
        return;
    };
    if feature.kind != LandscapeKind::Building {
        return;
    }
    let style = feature.building_style.unwrap_or_default();
    for index in 0..feature.points.len() {
        let a = feature.points[index];
        let b = feature.points[(index + 1) % feature.points.len()];
        let rgb = style.material.wall_rgb_for_edge(a, b);
        let color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
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
                surface: [
                    1.0,
                    feature.elevation,
                    style.floor_height,
                    style.material.shader_id() as f32,
                ],
            });
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
type ModelResult = Result<Arc<model::ModelMesh>, String>;

#[cfg(not(target_arch = "wasm32"))]
fn model_cache() -> &'static Mutex<HashMap<String, ModelResult>> {
    static MODELS: OnceLock<Mutex<HashMap<String, ModelResult>>> = OnceLock::new();
    MODELS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(not(target_arch = "wasm32"))]
fn model_for(path: &str, base: Option<&Path>) -> Option<Arc<model::ModelMesh>> {
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
}

#[cfg(target_arch = "wasm32")]
fn model_for(_path: &str, _base: Option<&Path>) -> Option<Arc<model::ModelMesh>> {
    None
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
                surface: [0.0; 4],
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

pub(super) fn add_terrain_walls(
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
                surface: [0.0; 4],
            });
        }
    }
}

#[cfg(any(not(target_arch = "wasm32"), test))]
pub(super) fn append_landscape(
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

pub(super) fn append_landscape_feature(
    scene: &mut Scene,
    ctx: &egui::Context,
    feature: &opencity::LandscapeFeature,
    features: &[opencity::LandscapeFeature],
    palette: EditorPalette,
    selected_landscape: Option<&str>,
    model_base: Option<&Path>,
) {
    if let Some(scaffold) = opencity::runtime::scaffold::BuildingScaffold::from_feature(feature) {
        for vertex in scaffold.mesh().vertices {
            let shade = 0.72 + 0.28 * vertex.normal[2].abs();
            scene.vertices.push(WorldVertex {
                point: WorldPoint {
                    x: vertex.position[0],
                    y: vertex.position[1],
                },
                height: vertex.position[2],
                depth_bias: 0.,
                color: Color32::from_gray((160. * shade) as u8),
                surface: [0.; 4],
            });
        }
        return;
    }
    // Flat landscape features frequently overlap at the same elevation. A
    // depth-only offset keeps their screen position fixed while giving the
    // fill a stable order regardless of triangle interpolation or draw order.
    // Roads use zero bias, so they remain in front of ground cover.
    let depth_priority = match feature.kind {
        LandscapeKind::Beach => 0.08,
        LandscapeKind::Land => 0.06,
        LandscapeKind::Lake | LandscapeKind::River => 0.04,
        // Keep ground cover ahead of PNG terrain's 0.001 bias as well as land.
        LandscapeKind::Vegetation => 0.0004,
        LandscapeKind::Concrete => 0.0002,
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
                - if feature.kind == LandscapeKind::Building {
                    0.0
                } else {
                    0.02
                }
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
