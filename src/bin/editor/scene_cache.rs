//! Scene cache for the city editor.
use super::*;

impl RoadEditor {
    pub(super) fn projected_map(&mut self) -> Arc<map::Projection> {
        if let Some((program, projection)) = &self.projection_cache {
            if *program == self.program {
                return Arc::clone(projection);
            }
        }
        let projection = Arc::new(map::project(&self.program));
        self.projection_cache = Some((self.program.clone(), Arc::clone(&projection)));
        projection
    }

    pub(super) fn scene_geometry(
        &mut self,
        ctx: &egui::Context,
        palette: EditorPalette,
        projection: &map::Projection,
    ) -> Arc<scene3d::Scene> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(receiver) = &self.scene_build {
            match receiver.try_recv() {
                Ok(cache) => {
                    self.scene_cache = Some(cache);
                    self.scene_build = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.scene_build = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(cache) = &self.scene_cache {
            if cache.program == self.program
                && cache.theme == palette.theme
                && cache.selected_landscape == self.selected_landscape
            {
                #[cfg(target_arch = "wasm32")]
                {
                    self.web_scene_build = None;
                }
                return Arc::clone(&cache.scene);
            }
        }
        #[cfg(target_arch = "wasm32")]
        if self.dragging_control.is_some()
            || self.dragging_endpoint.is_some()
            || self.dragging_intersection.is_some()
        {
            if let Some(cache) = &self.scene_cache {
                return Arc::clone(&cache.scene);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.scene_build.is_none() {
                let (sender, receiver) = std::sync::mpsc::channel();
                let program = self.program.clone();
                let projection = projection.clone();
                let selected_landscape = self.selected_landscape.clone();
                let model_base = self
                    .path
                    .as_ref()
                    .and_then(|path| path.parent())
                    .map(PathBuf::from);
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let scene = Arc::new(scene3d::build_scene_with_model_base(
                        &program,
                        &projection,
                        palette,
                        &[],
                        selected_landscape.as_deref(),
                        model_base.as_deref(),
                    ));
                    let _ = sender.send(SceneCache {
                        program,
                        theme: palette.theme,
                        selected_landscape,
                        scene,
                    });
                    ctx.request_repaint();
                });
                self.scene_build = Some(receiver);
            }
            return self
                .scene_cache
                .as_ref()
                .map(|cache| Arc::clone(&cache.scene))
                .unwrap_or_else(|| scene3d::loading_scene(palette));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let needs_build = self.web_scene_build.as_ref().is_none_or(|build| {
                build.program != self.program
                    || build.theme != palette.theme
                    || build.selected_landscape != self.selected_landscape
            });
            if needs_build {
                self.web_scene_build = Some(scene3d::WebSceneBuild::new(
                    self.program.clone(),
                    projection.clone(),
                    palette,
                    self.selected_landscape.clone(),
                ));
            }
            if self
                .web_scene_build
                .as_mut()
                .unwrap()
                .advance(std::time::Duration::from_millis(6))
            {
                let build = self.web_scene_build.take().unwrap();
                let scene = Arc::new(build.finish());
                self.scene_cache = Some(SceneCache {
                    program: self.program.clone(),
                    theme: palette.theme,
                    selected_landscape: self.selected_landscape.clone(),
                    scene: Arc::clone(&scene),
                });
                return scene;
            }
            ctx.request_repaint();
            return self
                .scene_cache
                .as_ref()
                .map(|cache| Arc::clone(&cache.scene))
                .unwrap_or_else(|| scene3d::loading_scene(palette));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn design_issues(
        &mut self,
        ctx: &egui::Context,
        projection: &map::Projection,
    ) -> Vec<DesignIssue> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(receiver) = &self.design_analysis_job {
            match receiver.try_recv() {
                Ok(cache) => {
                    // An edit may have overtaken this analysis. Never display stale issues.
                    if cache.program == self.program {
                        self.design_analysis_cache = Some(cache);
                    }
                    self.design_analysis_job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.design_analysis_job = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(cache) = &self.design_analysis_cache {
            if cache.program == self.program
                || self.dragging_endpoint.is_some()
                || self.dragging_control.is_some()
                || self.dragging_intersection.is_some()
            {
                return cache.issues.clone();
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.design_analysis_job.is_none() {
                let program = self.program.clone();
                let projection = projection.clone();
                let repaint = ctx.clone();
                let (sender, receiver) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let profiles = paint_profiles(&program, &projection);
                    let issues = design_rule_issues(&program, &projection, &profiles);
                    let _ = sender.send(DesignAnalysisCache { program, issues });
                    repaint.request_repaint();
                });
                self.design_analysis_job = Some(receiver);
            }
            Vec::new()
        }
    }

    pub(super) fn paint_selection_overlay(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        projection: &map::Projection,
        palette: EditorPalette,
        camera: CameraView,
    ) {
        if camera == CameraView::Vehicular {
            return;
        }
        if self.selected_segment.is_none() && self.selected_segments.is_empty() {
            return;
        }
        let profiles = paint_profiles(&self.program, projection);
        for (index, segment) in projection.segments.iter().enumerate() {
            if self.selected_segment != Some(segment.source)
                && !self.selected_segments.contains(&segment.source)
            {
                continue;
            }
            let steps = if segment_is_straight(segment) { 1 } else { 32 };
            let start = profiles[index].start_clip.unwrap_or(0.);
            let end = profiles[index].end_clip.unwrap_or(1.);
            if start >= end {
                continue;
            }
            let path = (0..=steps)
                .map(|step| {
                    let t = start + (end - start) * step as f32 / steps as f32;
                    let world = segment.point_at(t);
                    match camera {
                        CameraView::Top => screen_of(rect, self.map_pan, self.map_zoom, world),
                        CameraView::Isometric => iso_screen(
                            rect,
                            self.map_pan,
                            self.map_zoom,
                            self.orbit_target,
                            self.orbit_angle,
                            self.orbit_elevation,
                            world,
                            segment.height_at(t),
                        ),
                        CameraView::Vehicular => unreachable!(),
                    }
                })
                .collect();
            painter.add(egui::Shape::line(
                path,
                egui::Stroke::new(3.0, palette.map_selected),
            ));
        }
    }
}
