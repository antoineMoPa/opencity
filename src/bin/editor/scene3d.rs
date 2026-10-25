//! Shared world geometry for the top and isometric cameras. The detailed map
//! painters are tessellated in world space, then drawn with wgpu depth testing.
use super::*;
use eframe::{egui_wgpu, wgpu};
use std::ops::Range;
use std::path::Path;
use std::sync::{Arc, OnceLock};
#[cfg(not(target_arch = "wasm32"))]
use std::{collections::HashMap, sync::Mutex};
use wgpu::util::DeviceExt;

const GEOMETRY_SCALE: f32 = 100.0;
const PAVEMENT_OFFSET: f32 = 0.005;
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
    clip: [[f32; 4]; 4],
}

fn bytes_of<T>(values: &[T]) -> &[u8] {
    // GpuVertex and CameraUniform contain only initialized f32 fields.
    // wgpu consumes these bytes without interpreting them as Rust objects.
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

#[path = "scene3d/builders.rs"]
mod builders;
#[path = "scene3d/geometry.rs"]
mod geometry;
#[path = "scene3d/heightmap.rs"]
mod heightmap;
#[path = "scene3d/landscape.rs"]
mod landscape;
#[path = "scene3d/networks.rs"]
mod networks;
#[path = "scene3d/renderer.rs"]
mod renderer;

#[cfg(test)]
pub use builders::build_scene;
#[cfg(any(not(target_arch = "wasm32"), test))]
pub use builders::build_scene_with_model_base;
#[cfg(any(target_arch = "wasm32", test))]
pub use builders::WebSceneBuild;
use geometry::*;
use landscape::*;
pub use renderer::SceneCallback;

#[cfg(test)]
#[path = "scene3d/tests.rs"]
mod tests;
