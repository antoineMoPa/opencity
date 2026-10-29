//! Complete and incremental scene assembly share the same geometry pipeline.
use super::*;

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

#[cfg(any(not(target_arch = "wasm32"), test))]
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
    let heightmap = super::super::elevation_map::load(program, model_base)
        .ok()
        .flatten();
    if let Some(map) = &heightmap {
        heightmap::append_heightmap(&mut scene, &map, palette);
    }
    networks::append_networks(&mut scene, program, palette.theme);
    let ctx = egui::Context::default();
    let profiles = paint_profiles(program, projection);
    let mut geometry = opencity::road_geometry::RoadGeometry::compile(program, projection);
    geometry.configure_safety(program, projection, heightmap.as_deref());
    let expanded = super::super::frontage::scaffold_program(program, model_base);
    let mut features = project_landscape_features(expanded.as_ref().unwrap_or(program)).features;
    if heightmap.is_none()
        && !features
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
        append_intersection(
            &mut scene,
            &ctx,
            program,
            projection,
            &geometry,
            &intersection,
            palette,
        );
    }
    #[cfg(test)]
    if std::env::var_os("OPENCITY_SCENE_PROFILE").is_some() {
        eprintln!(
            "scene parts: terrain={} vertices/{:?}, roads={} vertices/{:?}, \
                intersections={} vertices/{:?}",
            before_roads,
            landscape_time,
            before_intersections - before_roads,
            roads_time - landscape_time,
            scene.vertices.len() - before_intersections,
            started.elapsed() - roads_time
        );
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
    pub(super) scene: Scene,
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
        let mut geometry = opencity::road_geometry::RoadGeometry::compile(&program, &projection);
        let road_order = road_paint_order(&program, &projection, &profiles);
        let intersections = map::intersections(&program, &projection);
        let expanded = super::super::frontage::scaffold_program(&program, None);
        let mut landscape =
            project_landscape_features(expanded.as_ref().unwrap_or(&program)).features;
        let mut scene = Scene::default();
        let heightmap = super::super::elevation_map::load(&program, None)
            .ok()
            .flatten();
        if let Some(map) = &heightmap {
            heightmap::append_heightmap(&mut scene, map, palette);
        }
        geometry.configure_safety(&program, &projection, heightmap.as_deref());
        networks::append_networks(&mut scene, &program, palette.theme);
        if heightmap.is_none()
            && !landscape
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
            append_intersection(
                &mut self.scene,
                &self.ctx,
                &self.program,
                &self.projection,
                &self.geometry,
                intersection,
                self.palette,
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
