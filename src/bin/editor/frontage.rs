//! Cached, neutral authoring overlay for road-side reservations.
use super::*;

impl RoadEditor {
    pub(super) fn projected_frontages(&mut self) -> Arc<opencity::FrontageProjection> {
        if let Some((program, projection)) = &self.frontage_cache {
            if *program == self.program {
                return Arc::clone(projection);
            }
        }
        let projection = Arc::new(opencity::project_frontage_zones(&self.program));
        self.frontage_cache = Some((self.program.clone(), Arc::clone(&projection)));
        projection
    }

    pub(super) fn paint_frontage_zones(&mut self, ui: &egui::Ui, rect: egui::Rect, iso: bool) {
        let projection = self.projected_frontages();
        let painter = ui.painter().with_clip_rect(rect);
        let color = ui.visuals().hyperlink_color;
        for zone in &projection.zones {
            let selected = self.selected_zone.as_ref() == Some(&zone.name);
            for (cell, quad) in zone.strip_quads(24).into_iter().enumerate() {
                let points: Vec<_> = quad
                    .iter()
                    .map(|p| {
                        self.spatial_screen(
                            rect,
                            WorldPoint { x: p[0], y: p[1] },
                            p[2] + 0.002,
                            iso,
                        )
                    })
                    .collect();
                // Triangles tolerate curved-strip cells without assuming convexity.
                for triangle in [[0, 1, 2], [0, 2, 3]] {
                    painter.add(egui::Shape::convex_polygon(
                        triangle.map(|i| points[i]).to_vec(),
                        color.gamma_multiply(if selected { 0.24 } else { 0.035 }),
                        egui::Stroke::NONE,
                    ));
                }
                if selected {
                    let stroke = egui::Stroke::new(3., color);
                    painter.line_segment([points[0], points[1]], stroke);
                    if cell % 24 == 0 {
                        painter.line_segment([points[0], points[3]], stroke);
                    }
                    if cell % 24 == 23 {
                        painter.line_segment([points[1], points[2]], stroke);
                    }
                }
                painter.line_segment(
                    [points[2], points[3]],
                    egui::Stroke::new(
                        if selected { 3.0 } else { 0.5 },
                        color.gamma_multiply(if selected { 1. } else { 0.3 }),
                    ),
                );
            }
            let side = if zone.side == opencity::FrontageSide::Both {
                opencity::FrontageSide::Left
            } else {
                zone.side
            };
            if let Some(frame) = zone.frame_at(side, (zone.start + zone.end) * 0.5) {
                // Reveal authoring names only when the street segment has
                // enough screen space for them, in either camera projection.
                let segment = zone.segment();
                let endpoint =
                    |t| self.spatial_screen(rect, segment.point_at(t), segment.height_at(t), iso);
                let span = endpoint(zone.start).distance(endpoint(zone.end));
                if span < 180. {
                    continue;
                }
                let label = painter.layout_no_wrap(
                    format!("{} · {}", zone.name, zone.building_use.keyword()),
                    egui::FontId::proportional(11.0),
                    color,
                );
                if span < 1.5 * (label.size().x + 32.) {
                    continue;
                }
                let p = [
                    frame.origin[0] + frame.outward[0] * frame.depth * 0.5,
                    frame.origin[1] + frame.outward[1] * frame.depth * 0.5,
                ];
                let center = self.spatial_screen(
                    rect,
                    WorldPoint { x: p[0], y: p[1] },
                    frame.origin[2] + 0.003,
                    iso,
                );
                painter.galley(center - label.size() * 0.5, label, color);
            }
        }
        if !projection.errors.is_empty() {
            painter.text(
                rect.left_top() + egui::vec2(12.0, 12.0),
                egui::Align2::LEFT_TOP,
                format!("Frontage: {}", projection.errors[0]),
                egui::FontId::proportional(12.0),
                ui.visuals().error_fg_color,
            );
        }
    }
}

type ScaffoldEntry = (Program, Option<PathBuf>, Program);
fn scaffold_cache() -> &'static std::sync::Mutex<Option<ScaffoldEntry>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<ScaffoldEntry>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}
/// Picking never waits on background full-map generation. A completed scene
/// publishes its exact expansion here, and the next frame adopts its geometry.
pub(super) fn cached_scaffold_program(program: &Program, base: Option<&Path>) -> Option<Program> {
    let cache = scaffold_cache().try_lock().ok()?;
    let (source, path, expanded) = cache.as_ref()?;
    (source == program && path.as_deref() == base).then(|| expanded.clone())
}

/// Cached expansion shared by scene rendering and picking. Elevation loading is
/// the same bundled/external loader used by the scene, including browser builds.
pub(super) fn scaffold_program(program: &Program, base: Option<&Path>) -> Result<Program, String> {
    let mut cache = scaffold_cache().lock().unwrap();
    let base = base.map(PathBuf::from);
    if let Some((source, path, expanded)) = cache.as_ref() {
        if source == program && path == &base {
            return Ok(expanded.clone());
        }
    }
    let elevation = super::elevation_map::load(program, base.as_deref())?;
    let expanded = opencity::runtime::scaffold::expand_program(program, |p| {
        opencity::runtime::City::from_program_with_elevation_loader(p, |_| {
            elevation
                .as_deref()
                .cloned()
                .ok_or_else(|| "Missing elevation asset".into())
        })
    })?;
    *cache = Some((program.clone(), base, expanded.clone()));
    Ok(expanded)
}

impl RoadEditor {
    pub(super) fn zone_options_ui(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        iso: bool,
    ) -> bool {
        if self.minimal.input_suspended
            || self.pending.is_some()
            || self.map_tool != MapTool::Select
            || self.minimal.pointer_over_toolbar(ui.ctx())
        {
            return false;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.selected_zone = None;
            return false;
        }
        let projection = self.projected_frontages();
        if response.clicked_by(egui::PointerButton::Primary) {
            let hit = response.interact_pointer_pos().and_then(|pointer| {
                // Generated volumes select their governing zone, never expose
                // properties that would mutate an ephemeral building call.
                let building = self.building_at(rect, pointer, iso);
                let landscape = self.projected_landscape();
                let generated = building.as_ref().and_then(|name| {
                    landscape
                        .features
                        .iter()
                        .find(|f| f.name.as_ref() == Some(name) && name.starts_with("generated_"))
                });
                projection
                    .zones
                    .iter()
                    .find(|zone| {
                        if let Some(feature) = generated {
                            let n = feature.points.len() as f32;
                            let center = feature
                                .points
                                .iter()
                                .fold([0., 0.], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n]);
                            return opencity::runtime::scaffold::strip_footprints(zone)
                                .iter()
                                .any(|p| opencity::runtime::scaffold::contains(p, center));
                        }
                        if building.is_some() {
                            return false;
                        }
                        zone.strip_quads(24).iter().any(|quad| {
                            let p = quad.map(|p| {
                                self.spatial_screen(
                                    rect,
                                    WorldPoint { x: p[0], y: p[1] },
                                    p[2],
                                    iso,
                                )
                            });
                            triangle_contains(p[0], p[1], p[2], pointer)
                                || triangle_contains(p[0], p[2], p[3], pointer)
                        })
                    })
                    .map(|zone| zone.name.clone())
            });
            self.selected_zone = hit;
            if self.selected_zone.is_some() {
                self.selected_landscape = None;
                self.selected_segment = None;
                self.selected_segments.clear();
            }
        }
        let Some(zone) = projection
            .zones
            .iter()
            .find(|z| Some(&z.name) == self.selected_zone.as_ref())
        else {
            return false;
        };
        let mut form = zone.form;
        let mut use_kind = zone.building_use;
        let mut depth = zone.depth;
        let mut close = false;
        let mut changed = false;
        let popup = egui::Area::new(egui::Id::new("zone-properties"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.left_top() + egui::vec2(14., 44.))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(230.);
                    ui.horizontal(|ui| {
                        ui.label("Zone properties");
                        if ui.small_button("×").clicked() {
                            close = true;
                        }
                    });
                    ui.small(&zone.name);
                    egui::ComboBox::from_id_salt("zone-use")
                        .selected_text(use_kind.keyword())
                        .show_ui(ui, |ui| {
                            for value in [
                                opencity::BuildingUse::Residential,
                                opencity::BuildingUse::Commercial,
                                opencity::BuildingUse::MixedUse,
                            ] {
                                changed |= ui
                                    .selectable_value(&mut use_kind, value, value.keyword())
                                    .changed();
                            }
                        });
                    egui::ComboBox::from_id_salt("zone-form")
                        .selected_text(form.form.keyword())
                        .show_ui(ui, |ui| {
                            for value in [
                                opencity::BuildingForm::Street,
                                opencity::BuildingForm::CityCentre,
                            ] {
                                if ui
                                    .selectable_value(&mut form.form, value, value.keyword())
                                    .changed()
                                {
                                    (
                                        form.height_min,
                                        form.height_max,
                                        form.frontage_min,
                                        form.frontage_max,
                                        form.setback,
                                    ) = match value {
                                        opencity::BuildingForm::Street => {
                                            (0.034, 0.21, 0.085, 0.255, 0.004)
                                        }
                                        opencity::BuildingForm::CityCentre => {
                                            (0.24, 0.70, 0.17, 0.34, 0.012)
                                        }
                                    };
                                    changed = true;
                                }
                            }
                        });
                    changed |= ui
                        .add(egui::Slider::new(&mut form.density, 0. ..=1.).text("Density"))
                        .changed();
                    egui::Grid::new("zone-dimensions").show(ui, |ui| {
                        for (label, value) in [
                            ("Height min", &mut form.height_min),
                            ("Height max", &mut form.height_max),
                            ("Frontage min", &mut form.frontage_min),
                            ("Frontage max", &mut form.frontage_max),
                            ("Setback", &mut form.setback),
                            ("Depth", &mut depth),
                        ] {
                            ui.label(label);
                            changed |= ui
                                .add(egui::DragValue::new(value).speed(0.001).range(0. ..=10.))
                                .changed();
                            ui.end_row();
                        }
                    });
                    ui.small("Dimensions in map units. Buildings remain generated previews.");
                });
            });
        if changed
            && form.height_min > 0.
            && form.height_max >= form.height_min
            && form.frontage_min > 0.
            && form.frontage_max >= form.frontage_min
            && depth > form.setback
        {
            self.edit_zone(&zone.name, use_kind, form, depth);
        }
        if close {
            self.selected_zone = None;
        }
        response.clicked_by(egui::PointerButton::Primary)
            || ui
                .ctx()
                .pointer_hover_pos()
                .is_some_and(|p| popup.response.rect.contains(p))
    }

    pub(super) fn edit_zone(
        &mut self,
        name: &str,
        use_kind: opencity::BuildingUse,
        form: opencity::ZoneForm,
        depth: f32,
    ) -> bool {
        let Some(call) = self.program.statements.iter_mut().find_map(|s| match s {
            Statement::Call(c)
                if c.name == "frontage_zone" && c.assigns.first().is_some_and(|n| n == name) =>
            {
                Some(c)
            }
            _ => None,
        }) else {
            return false;
        };
        let Some(Argument::Object(options)) = call.args.get_mut(1) else {
            return false;
        };
        let values = [
            ("use", Argument::Identifier(use_kind.keyword().into())),
            ("form", Argument::Identifier(form.form.keyword().into())),
            ("density", Argument::Number(form.density.to_string())),
            ("height_min", Argument::Number(form.height_min.to_string())),
            ("height_max", Argument::Number(form.height_max.to_string())),
            (
                "frontage_min",
                Argument::Number(form.frontage_min.to_string()),
            ),
            (
                "frontage_max",
                Argument::Number(form.frontage_max.to_string()),
            ),
            ("setback", Argument::Number(form.setback.to_string())),
            ("depth", Argument::Number(depth.to_string())),
        ];
        for (key, value) in values {
            if let Some((_, existing)) = options.iter_mut().find(|(k, _)| k == key) {
                *existing = value;
            } else {
                options.push((key.into(), value));
            }
        }
        self.dirty = true;
        true
    }
}
