//! Building picking and anchored properties do not mutate cached scene geometry.
use super::*;
use opencity::{BuildingMaterial, BuildingStyle, LandscapeFeature};

impl RoadEditor {
    fn building_screen_points(
        &self,
        feature: &LandscapeFeature,
        rect: egui::Rect,
        iso: bool,
        roof: bool,
    ) -> Vec<egui::Pos2> {
        let height = feature.elevation
            + if roof {
                feature.height.unwrap_or(0.0)
            } else {
                0.0
            };
        feature
            .points
            .iter()
            .map(|p| self.spatial_screen(rect, WorldPoint { x: p[0], y: p[1] }, height, iso))
            .collect()
    }

    /// Test actual projected roofs and wall faces. Interpolate camera depth at
    /// the pointer so a tall building wins over buildings hidden behind it.
    pub(super) fn building_at(
        &mut self,
        rect: egui::Rect,
        pointer: egui::Pos2,
        iso: bool,
    ) -> Option<String> {
        let mut nearest: Option<(f32, String)> = None;
        let landscape = self.projected_landscape();
        for feature in landscape
            .features
            .iter()
            .filter(|f| f.kind == LandscapeKind::Building && f.name.is_some())
        {
            let roof = self.building_screen_points(feature, rect, iso, true);
            let base = self.building_screen_points(feature, rect, iso, false);
            let depth = |index: usize, top: bool| {
                let p = feature.points[index];
                let z = feature.elevation
                    + if top {
                        feature.height.unwrap_or(0.0)
                    } else {
                        0.0
                    };
                if iso {
                    (p[0] * self.orbit_angle.sin() - p[1] * self.orbit_angle.cos())
                        * self.orbit_elevation.cos()
                        + z * self.orbit_elevation.sin()
                } else {
                    z
                }
            };
            let mut faces: Vec<Vec<(egui::Pos2, f32)>> = vec![roof
                .iter()
                .enumerate()
                .map(|(i, p)| (*p, depth(i, true)))
                .collect()];
            if iso {
                for i in 0..roof.len() {
                    let j = (i + 1) % roof.len();
                    faces.push(vec![
                        (base[i], depth(i, false)),
                        (base[j], depth(j, false)),
                        (roof[j], depth(j, true)),
                        (roof[i], depth(i, true)),
                    ]);
                }
            }
            for face in faces {
                let points: Vec<_> = face.iter().map(|v| v.0).collect();
                for [a, b, c] in landscape_triangles(&points) {
                    if !triangle_contains(a, b, c, pointer) {
                        continue;
                    }
                    let cross = |u: egui::Vec2, v: egui::Vec2| u.x * v.y - u.y * v.x;
                    let area = cross(b - a, c - a);
                    if area.abs() < 0.00001 {
                        continue;
                    }
                    let wb = cross(pointer - a, c - a) / area;
                    let wc = cross(b - a, pointer - a) / area;
                    let get = |p| face.iter().find(|v| v.0 == p).unwrap().1;
                    let hit_depth = get(a) * (1.0 - wb - wc) + get(b) * wb + get(c) * wc;
                    if nearest.as_ref().is_none_or(|n| hit_depth > n.0) {
                        nearest = Some((hit_depth, feature.name.clone().unwrap()));
                    }
                }
            }
        }
        nearest.map(|n| n.1)
    }

    pub(super) fn building_options_ui(
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
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) && self.selected_landscape.is_some() {
            self.selected_landscape = None;
            return true;
        }
        let landscape = self.projected_landscape();
        let mut feature = self.selected_landscape.as_ref().and_then(|name| {
            landscape
                .features
                .iter()
                .find(|f| f.name.as_ref() == Some(name) && f.kind == LandscapeKind::Building)
        });
        // Process viewport clicks before drawing the popup: selection and its
        // property controls appear together on the first selection frame.
        let clicked = response.clicked_by(egui::PointerButton::Primary);
        if clicked {
            if let Some(name) = response
                .interact_pointer_pos()
                .and_then(|p| self.building_at(rect, p, iso))
            {
                self.selected_landscape = Some(name.clone());
                self.selected_segment = None;
                self.selected_segments.clear();
                self.selected_endpoint = None;
                self.selected_intersection = None;
                self.point_options = None;
                feature = landscape
                    .features
                    .iter()
                    .find(|f| f.name.as_ref() == Some(&name));
            } else {
                self.selected_landscape = None;
                return false;
            }
        }
        let Some(feature) = feature else {
            return false;
        };
        let name = feature.name.as_deref().unwrap();
        let roof = self.building_screen_points(&feature, rect, iso, true);
        let anchor = roof
            .iter()
            .copied()
            .reduce(|a, b| if a.x > b.x { a } else { b })
            .unwrap_or(rect.center());
        let mut height = feature.height.unwrap_or(0.25);
        let mut style = feature.building_style.unwrap_or_default();
        let mut changed = false;
        let mut close = false;
        let area = egui::Area::new(egui::Id::new("building-properties"))
            .order(egui::Order::Foreground)
            .fixed_pos((anchor + egui::vec2(14.0, 14.0)).clamp(
                rect.min,
                (rect.max - egui::vec2(230.0, 190.0)).max(rect.min),
            ))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(205.0);
                    ui.label("Building properties");
                    ui.small(name);
                    egui::Grid::new("building-property-fields").show(ui, |ui| {
                        ui.label("Height");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut height)
                                    .speed(0.01)
                                    .range(0.001..=100.0)
                                    .max_decimals(3),
                            )
                            .changed();
                        ui.end_row();
                        ui.label("Material");
                        egui::ComboBox::from_id_salt("building-material")
                            .selected_text(style.material.keyword())
                            .show_ui(ui, |ui| {
                                for material in [
                                    BuildingMaterial::Concrete,
                                    BuildingMaterial::Glass,
                                    BuildingMaterial::Brick,
                                ] {
                                    changed |= ui
                                        .selectable_value(
                                            &mut style.material,
                                            material,
                                            material.keyword(),
                                        )
                                        .changed();
                                }
                            });
                        ui.end_row();
                        ui.label("Floor spacing");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut style.floor_height)
                                    .speed(0.001)
                                    .range(0.001..=100.0)
                                    .max_decimals(3),
                            )
                            .changed();
                        ui.end_row();
                    });
                    ui.small(format!("≈ {:.1} floors", height / style.floor_height));
                    close = ui.button("Close").clicked();
                });
            });
        if changed {
            self.edit_building_properties(name, height, style);
        }
        if close
            || (ui.input(|i| i.pointer.any_pressed())
                && ui
                    .ctx()
                    .pointer_hover_pos()
                    .is_some_and(|p| !area.response.rect.contains(p)))
        {
            self.selected_landscape = None;
        }
        clicked
            || close
            || ui
                .ctx()
                .pointer_hover_pos()
                .is_some_and(|p| area.response.rect.contains(p))
    }

    pub(super) fn edit_building_properties(
        &mut self,
        name: &str,
        height: f32,
        style: BuildingStyle,
    ) -> bool {
        if !height.is_finite()
            || height <= 0.0
            || !style.floor_height.is_finite()
            || style.floor_height <= 0.0
        {
            return false;
        }
        // Change only the height and options on the original call. The model,
        // coordinates, elevation context, and declaration identity stay intact.
        let Some(call) = self
            .program
            .statements
            .iter_mut()
            .find_map(|statement| match statement {
                Statement::Call(call)
                    if call.name == "building"
                        && call.assigns.first().is_some_and(|n| n == name) =>
                {
                    Some(call)
                }
                _ => None,
            })
        else {
            return false;
        };
        call.args[0] = Argument::Number(height.to_string());
        let offset = if matches!(call.args.get(1), Some(Argument::String(_))) {
            2
        } else {
            1
        };
        let options = Argument::Object(vec![
            (
                "material".into(),
                Argument::Identifier(style.material.keyword().into()),
            ),
            (
                "floor_height".into(),
                Argument::Number(style.floor_height.to_string()),
            ),
        ]);
        if matches!(call.args.get(offset), Some(Argument::Object(_))) {
            call.args[offset] = options;
        } else {
            call.args.insert(offset, options);
        }
        self.dirty = true;
        true
    }

    pub(super) fn paint_landscape_selection(
        &mut self,
        painter: &egui::Painter,
        rect: egui::Rect,
        palette: EditorPalette,
        camera: CameraView,
    ) {
        if self.selected_landscape.is_none() {
            return;
        }
        let landscape = self.projected_landscape();
        let Some(feature) = self.selected_landscape.as_ref().and_then(|name| {
            landscape
                .features
                .iter()
                .find(|f| f.name.as_ref() == Some(name))
        }) else {
            return;
        };
        let points =
            self.building_screen_points(&feature, rect, camera == CameraView::Isometric, true);
        painter.add(egui::Shape::closed_line(
            points,
            egui::Stroke::new(3.0, palette.map_selected),
        ));
    }
}
