//! Section for the city editor.
use super::*;

impl RoadEditor {
    pub(super) fn road(&self) -> Option<&RoadDefinition> {
        if let Some(road) = &self.minimal.pending_road {
            return Some(road);
        }
        match self.program.statements.get(self.selected_road?)? {
            Statement::Road(road) => Some(road),
            _ => None,
        }
    }

    pub(super) fn road_mut(&mut self) -> Option<&mut RoadDefinition> {
        match self.program.statements.get_mut(self.selected_road?)? {
            Statement::Road(road) => Some(road),
            _ => None,
        }
    }

    pub(super) fn unique_road_name(&self) -> String {
        for number in 1.. {
            let name = format!("road_{number}");
            if !self
                .program
                .statements
                .iter()
                .any(|statement| matches!(statement, Statement::Road(road) if road.name == name))
            {
                return name;
            }
        }
        unreachable!()
    }

    pub(super) fn add_road(&mut self) {
        let name = self.unique_road_name();
        self.program
            .statements
            .push(Statement::Road(RoadDefinition {
                role: opencity::RoadRole::Ordinary,
                name: name.clone(),
                elements: vec![
                    RoadElement {
                        kind: RoadElementKind::Sidewalk,
                        catch: None,
                    },
                    RoadElement {
                        kind: RoadElementKind::CarLane,
                        catch: None,
                    },
                    RoadElement {
                        kind: RoadElementKind::Middle,
                        catch: None,
                    },
                    RoadElement {
                        kind: RoadElementKind::CarLane,
                        catch: None,
                    },
                    RoadElement {
                        kind: RoadElementKind::Sidewalk,
                        catch: None,
                    },
                ],
                direction: RoadDirection::Both,
            }));
        self.selected_road = Some(self.program.statements.len() - 1);
        self.selected_element = None;
        self.road_name_draft = name.clone();
        self.view = View::Section;
        self.dirty = true;
        self.status = format!("Created {name}");
    }

    pub(super) fn add_element(&mut self) {
        let kind = self.new_kind;
        let selected = self.selected_element;
        let before = map::project(&self.program);
        let road_name = self.road().map(|road| road.name.clone());
        let mut inserted = None;
        if let Some(road) = self.road_mut() {
            let at = selected
                .map_or(road.elements.len(), |index| index + 1)
                .min(road.elements.len());
            road.elements.insert(at, RoadElement { kind, catch: None });
            inserted = Some(at);
            self.selected_element = Some(at);
            self.dirty = true;
        }
        if let (Some(name), Some(at)) = (road_name, inserted) {
            map::update_lane_attachments(
                &mut self.program,
                &name,
                map::ElementEdit::Inserted(at),
                &before,
            );
        }
    }

    pub(super) fn move_element(&mut self, offset: isize) {
        let Some(index) = self.selected_element else {
            return;
        };
        let before = map::project(&self.program);
        let road_name = self.road().map(|road| road.name.clone());
        let mut swapped = None;
        if let Some(road) = self.road_mut() {
            let target = index as isize + offset;
            if target >= 0 && (target as usize) < road.elements.len() {
                road.elements.swap(index, target as usize);
                swapped = Some(target as usize);
                self.selected_element = Some(target as usize);
                self.dirty = true;
            }
        }
        if let (Some(name), Some(target)) = (road_name, swapped) {
            map::update_lane_attachments(
                &mut self.program,
                &name,
                map::ElementEdit::Swapped(index, target),
                &before,
            );
        }
    }

    pub(super) fn remove_element(&mut self) {
        let Some(index) = self.selected_element else {
            return;
        };
        let before = map::project(&self.program);
        let road_name = self.road().map(|road| road.name.clone());
        let mut removed = false;
        if let Some(road) = self.road_mut() {
            if road.elements.len() > 1 && index < road.elements.len() {
                road.elements.remove(index);
                removed = true;
                self.selected_element = Some(index.min(road.elements.len() - 1));
                self.dirty = true;
            } else {
                self.status = "A road needs at least one element.".into();
            }
        }
        if let (Some(name), true) = (road_name, removed) {
            map::update_lane_attachments(
                &mut self.program,
                &name,
                map::ElementEdit::Removed(index),
                &before,
            );
        }
    }

    pub(super) fn remove_road(&mut self) {
        let Some(index) = self.selected_road else {
            return;
        };
        if index >= self.program.statements.len() {
            return;
        }
        self.program.statements.remove(index);
        self.selected_road = self
            .program
            .statements
            .iter()
            .position(|statement| matches!(statement, Statement::Road(_)));
        self.selected_element = None;
        self.selected_segment = None;
        self.selected_endpoint = None;
        self.road_name_draft = self
            .road()
            .map(|road| road.name.clone())
            .unwrap_or_default();
        self.dirty = true;
        self.status = "Road removed. Review any calls that used it before saving.".into();
    }

    pub(super) fn valid_name(name: &str) -> bool {
        let mut chars = name.chars();
        matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !matches!(name, "road" | "block" | "end")
    }

    pub(super) fn rename_road(&mut self, new_name: &str) -> bool {
        let Some(index) = self.selected_road else {
            return false;
        };
        if !Self::valid_name(new_name)
            || self
                .program
                .statements
                .iter()
                .enumerate()
                .any(|(other, statement)| {
                    other != index
                        && matches!(statement, Statement::Road(road) if road.name == new_name)
                })
        {
            return false;
        }
        let Some(Statement::Road(road)) = self.program.statements.get_mut(index) else {
            return false;
        };
        if road.name == new_name {
            return false;
        }
        let old_name = std::mem::replace(&mut road.name, new_name.to_owned());
        for statement in &mut self.program.statements {
            match statement {
                Statement::Call(call) if call.name == old_name => call.name = new_name.to_owned(),
                Statement::Call(call)
                    if map::IntersectionKind::from_call_name(&call.name).is_some() =>
                {
                    if let Some(Argument::Identifier(name)) = call.args.get_mut(1) {
                        if *name == old_name {
                            *name = new_name.to_owned();
                        }
                    }
                }
                Statement::Block(block) => {
                    for call in &mut block.calls {
                        if call.name == old_name {
                            call.name = new_name.to_owned();
                        }
                    }
                }
                _ => {}
            }
        }
        self.dirty = true;
        true
    }

    pub(super) fn road_name_editor(&mut self, ui: &mut egui::Ui) {
        if self.road().is_none() {
            return;
        }
        ui.label("Road name");
        let response =
            ui.add(egui::TextEdit::singleline(&mut self.road_name_draft).desired_width(180.0));
        let valid = Self::valid_name(&self.road_name_draft);
        let duplicate = self.program.statements.iter().enumerate().any(|(index, statement)| {
            Some(index) != self.selected_road
                && matches!(statement, Statement::Road(road) if road.name == self.road_name_draft)
        });
        if !valid || duplicate {
            ui.colored_label(
                editor_palette(ui.ctx().theme()).warning,
                if duplicate {
                    "A road already has this name."
                } else {
                    "Use letters, digits, underscores; start with a letter or _."
                },
            );
        }
        if response.changed() && valid && !duplicate {
            let name = self.road_name_draft.clone();
            self.rename_road(&name);
        }
    }

    pub(super) fn section_canvas(&mut self, root: &mut egui::Ui) {
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(root, |ui| {
                let palette = editor_palette(ui.ctx().theme());
                let Some(road) = self.road() else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(90.0);
                        ui.add(empty_profile_icon(ui.visuals().text_color()));
                        ui.heading("No road selected");
                    });
                    return;
                };
                ui.label(RichText::new("Preview").strong());
                ui.label(
                    RichText::new("Select an element to edit its properties.")
                        .small()
                        .weak(),
                );
                ui.add_space(12.0);
                let elements = road.elements.clone();
                let mut clicked = None;
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (index, element) in elements.iter().enumerate() {
                            let selected = self.selected_element == Some(index);
                            let width = element_width(element.kind);
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(width, 200.0),
                                egui::Sense::click(),
                            );
                            let fill = kind_color(element.kind, ui.ctx().theme());
                            ui.painter()
                                .rect_filled(rect.shrink2(egui::vec2(2.0, 8.0)), 5.0, fill);
                            if selected {
                                ui.painter().rect_stroke(
                                    rect.shrink(2.0),
                                    5.0,
                                    egui::Stroke::new(3.0, palette.map_selected),
                                    egui::StrokeKind::Outside,
                                );
                            }
                            let center = rect.center();
                            ui.painter().text(
                                egui::pos2(center.x, center.y - 12.0),
                                egui::Align2::CENTER_CENTER,
                                kind_short(element.kind),
                                egui::FontId::proportional(13.0),
                                palette.tile_text,
                            );
                            if let Some(catch) = element.catch {
                                ui.painter().text(
                                    egui::pos2(center.x, center.y + 15.0),
                                    egui::Align2::CENTER_CENTER,
                                    format!("${catch}"),
                                    egui::FontId::monospace(12.0),
                                    palette.tile_text,
                                );
                            }
                            if response.clicked() {
                                clicked = Some(index);
                            }
                            response.on_hover_text(kind_label(element.kind));
                        }
                    });
                });
                if let Some(index) = clicked {
                    self.selected_element = Some(index);
                }
                ui.add_space(16.0);
                if self.show_source {
                    ui.separator();
                    ui.label(RichText::new("SERIALIZED SOURCE").small().strong());
                    let mut source = self.program.to_road_source();
                    egui::ScrollArea::vertical()
                        .max_height(250.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut source)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .interactive(false),
                            );
                        });
                }
            });
    }
}
