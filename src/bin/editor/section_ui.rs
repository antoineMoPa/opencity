//! Full viewport cross-section editing, sharing the map document and history.
use super::*;

impl RoadEditor {
    pub(super) fn enter_section_editor(&mut self) {
        let selected = self
            .projected_map()
            .segments
            .iter()
            .find(|segment| Some(segment.source) == self.selected_segment)
            .cloned();
        self.cancel_placement();
        let selected = selected.filter(|road| {
            self.projected_map()
                .segments
                .iter()
                .any(|current| current.source == road.source && current.name == road.name)
        });
        self.selected_segment = selected.as_ref().map(|road| road.source);
        self.selected_segments = self.selected_segment.into_iter().collect();
        if let Some(name) = selected.map(|segment| segment.road_name) {
            self.selected_road = self.program.statements.iter().position(
                |statement| matches!(statement, Statement::Road(road) if road.name == name),
            );
        }
        self.selected_road = self
            .selected_road
            .filter(|index| {
                matches!(
                    self.program.statements.get(*index),
                    Some(Statement::Road(_))
                )
            })
            .or_else(|| {
                self.program
                    .statements
                    .iter()
                    .position(|s| matches!(s, Statement::Road(_)))
            });
        self.road_name_draft = self
            .road()
            .map(|road| road.name.clone())
            .unwrap_or_default();
        self.selected_element = None;
        self.view = View::Section;
    }

    pub(super) fn section_editor_ui(&mut self, root: &mut egui::Ui, input_suspended: bool) {
        self.map_rect = None;
        self.minimal.toolbar_rect = None;
        egui::Frame::new().inner_margin(24).show(root, |root| {
            egui::ScrollArea::vertical().show(root, |root| {
                root.add_enabled_ui(!input_suspended, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(12.0, 10.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Back to map").clicked() {
                            self.view = View::Map;
                        }
                        ui.heading("Cross section");
                    });
                    ui.label(
                        RichText::new("Changes apply to every placement of this road profile.")
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Profile").strong());
                        let roads: Vec<_> = self
                            .program
                            .statements
                            .iter()
                            .enumerate()
                            .filter_map(|(index, statement)| match statement {
                                Statement::Road(road) => Some((index, road.name.clone())),
                                _ => None,
                            })
                            .collect();
                        let old = self.selected_road;
                        egui::ComboBox::from_id_salt("section-profile")
                            .selected_text(
                                self.road()
                                    .map(|r| r.name.as_str())
                                    .unwrap_or("Choose a profile"),
                            )
                            .show_ui(ui, |ui| {
                                for (index, name) in roads {
                                    ui.selectable_value(&mut self.selected_road, Some(index), name);
                                }
                            });
                        if old != self.selected_road {
                            self.selected_element = None;
                            self.road_name_draft =
                                self.road().map(|r| r.name.clone()).unwrap_or_default();
                        }
                        if ui.button("New profile").clicked() {
                            self.add_road();
                        }
                        if ui.button("Save…").clicked() {
                            self.file_action(FileAction::Save);
                        }
                        if ui
                            .add_enabled(
                                self.road().is_some(),
                                egui::Button::new("Delete profile…"),
                            )
                            .clicked()
                        {
                            self.confirm_delete_road = true;
                        }
                    });
                    if self.road().is_none() {
                        return;
                    }
                    ui.horizontal_wrapped(|ui| {
                        self.road_name_editor(ui);
                        ui.label("Direction");
                        if let Some(road) = self.road_mut() {
                            egui::ComboBox::from_id_salt("section-direction")
                                .selected_text(road.direction.keyword())
                                .show_ui(ui, |ui| {
                                    for direction in [
                                        RoadDirection::Both,
                                        RoadDirection::Forward,
                                        RoadDirection::Reverse,
                                    ] {
                                        ui.selectable_value(
                                            &mut road.direction,
                                            direction,
                                            direction.keyword(),
                                        );
                                    }
                                });
                        }
                    });
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        egui::ComboBox::from_id_salt("section-add-kind")
                            .selected_text(kind_label(self.new_kind))
                            .show_ui(ui, |ui| {
                                for kind in KINDS {
                                    ui.selectable_value(&mut self.new_kind, kind, kind_label(kind));
                                }
                            });
                        if ui.button("Add element").clicked() {
                            self.add_element();
                        }
                    });
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut self.show_source, "Show source");
                    });
                    ui.add_space(6.0);
                    self.section_canvas(ui);
                    self.section_element_controls(ui);
                });
            });
        });
    }

    fn section_element_controls(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.selected_element else {
            return;
        };
        let before = map::project(&self.program);
        let mut changed_kind = false;
        if let Some(road) = self.road_mut() {
            let Some(element) = road.elements.get_mut(index) else {
                return;
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Element {}", index + 1));
                let old = element.kind;
                egui::ComboBox::from_id_salt("section-element-kind")
                    .selected_text(kind_label(element.kind))
                    .show_ui(ui, |ui| {
                        for kind in KINDS {
                            ui.selectable_value(&mut element.kind, kind, kind_label(kind));
                        }
                    });
                changed_kind = old != element.kind;
                let mut capture = element.catch.is_some();
                if ui.checkbox(&mut capture, "Capture parameter").changed() {
                    element.catch = capture.then_some(1);
                }
                if let Some(catch) = &mut element.catch {
                    ui.add(egui::DragValue::new(catch).prefix("$"));
                }
            });
        }
        if changed_kind {
            if let Some(name) = self.road().map(|road| road.name.clone()) {
                map::update_lane_attachments(
                    &mut self.program,
                    &name,
                    map::ElementEdit::Swapped(index, index),
                    &before,
                );
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Move left").clicked() {
                self.move_element(-1);
            }
            if ui.button("Move right").clicked() {
                self.move_element(1);
            }
            if ui.button("Remove element").clicked() {
                self.remove_element();
            }
        });
    }
}
