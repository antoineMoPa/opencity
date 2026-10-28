//! Dialogs for the city editor.
use super::*;

impl RoadEditor {
    pub(super) fn pending_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending else {
            return;
        };
        if matches!(action, PendingAction::New) {
            let opening = std::mem::take(&mut self.minimal.clear_just_opened);
            let content_width = (ctx.content_rect().width() - 80.0).clamp(160.0, 392.0);
            egui::Modal::new(egui::Id::new("clear-map-confirmation"))
                .frame(egui::Frame::popup(&ctx.global_style()).inner_margin(24.0))
                .show(ctx, |ui| {
                    ui.set_width(content_width);
                    ui.heading("Clear map?");
                    ui.add_space(10.0);
                    ui.label(
                        "Remove all placements. Road and block definitions stay. You can \
                            undo this.",
                    );
                    ui.add_space(20.0);
                    ui.add_enabled_ui(!opening, |ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        ui.spacing_mut().button_padding = egui::vec2(16.0, 9.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let clear = ui.add_sized(
                                [76.0, 36.0],
                                egui::Button::new(
                                    egui::RichText::new("Clear").color(ui.visuals().error_fg_color),
                                ),
                            );
                            let cancel = ui.add_sized([76.0, 36.0], egui::Button::new("Cancel"));
                            if opening || !ctx.memory(|m| m.focused().is_some()) {
                                cancel.request_focus();
                            }
                            if cancel.clicked()
                                || ui.input_mut(|i| {
                                    i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                })
                            {
                                self.pending = None;
                            }
                            if clear.clicked() {
                                self.pending = None;
                                self.cancel_placement();
                                self.program
                                    .statements
                                    .retain(|s| !matches!(s, Statement::Call(_)));
                                self.selected_landscape = None;
                                self.selected_intersection = None;
                                self.minimal.rename_source = None;
                                self.dirty = self.program != self.saved_program;
                                self.status = "Map cleared · Undo restores placements".into();
                            }
                        });
                    });
                });
            return;
        }
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Discard changes to the current document?");
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Discard changes").clicked() {
                        self.pending = None;
                        match action {
                            PendingAction::New => self.new_document(),
                            PendingAction::Open => self.pick_open(),
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending = None;
                    }
                });
            });
    }

    pub(super) fn delete_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_delete_road {
            return;
        }
        egui::Window::new("Delete road?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(
                    "Delete this road definition? Calls that name it will remain in \
                        the document.",
                );
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Delete road").clicked() {
                        self.confirm_delete_road = false;
                        self.remove_road();
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_delete_road = false;
                    }
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rectangles(shape: &egui::Shape, out: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Rect(rect) => out.push(rect.rect),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    rectangles(shape, out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn clear_dialog_wraps_and_aligns_actions_within_wide_and_narrow_viewports() {
        for width in [900.0, 270.0] {
            let ctx = egui::Context::default();
            let mut editor = crate::tests::sample_editor();
            editor.pending = Some(PendingAction::New);
            editor.minimal.clear_just_opened = true;
            let mut output = egui::FullOutput::default();
            for _ in 0..3 {
                output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 500.0),
                        )),
                        ..Default::default()
                    },
                    |_ui| editor.pending_dialog(&ctx),
                );
                output.textures_delta.clear();
            }
            let text = |label: &str| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(t) if t.galley.job.text == label => Some(t),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("missing dialog text: {label}"))
            };
            let title = text("Clear map?").visual_bounding_rect();
            let body = text(
                "Remove all placements. Road and block definitions stay. You can \
                    undo this.",
            );
            let cancel = text("Cancel").visual_bounding_rect();
            let clear = text("Clear").visual_bounding_rect();
            let mut rects = Vec::new();
            for shape in &output.shapes {
                rectangles(&shape.shape, &mut rects);
            }
            let dialog = rects
                .into_iter()
                .filter(|rect| rect.contains_rect(title) && rect.contains_rect(clear))
                .min_by(|a, b| a.area().total_cmp(&b.area()))
                .expect("modal frame");
            assert!(dialog.width() <= 444.0, "{dialog:?}");
            assert!(
                dialog.left() >= 0.0 && dialog.right() <= width,
                "{dialog:?}"
            );
            assert!(title.left() - dialog.left() >= 20.0);
            assert!(body.galley.rows.len() >= 2, "body must wrap at {width}px");
            assert!((title.left() - body.pos.x).abs() < 2.0);
            assert!(body.visual_bounding_rect().top() - title.bottom() >= 8.0);
            assert!(cancel.left() > title.left(), "actions should align right");
            assert!(clear.left() - cancel.right() >= 20.0);
            assert!((cancel.center().y - clear.center().y).abs() < 1.0);
            assert!(cancel.top() - body.visual_bounding_rect().bottom() >= 20.0);
            assert!(dialog.right() - clear.right() >= 30.0);
            assert!(dialog.bottom() - clear.bottom() >= 25.0);
        }
    }
}
