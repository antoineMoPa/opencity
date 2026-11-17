use super::*;
impl SignDesign {
    /// Shared controls; callers own undo, validation, saving and preview revisions.
    pub fn edit(&mut self, ui: &mut egui::Ui) -> bool {
        let before = self.clone();
        match self {
            Self::Stop { cluster } => {
                ui.checkbox(cluster, "Parking cluster");
            }
            Self::Speed { limit } => {
                ui.label("Maximum");
                ui.add(egui::DragValue::new(limit).range(10..=130).speed(10.));
            }
            Self::Yield => {
                ui.label("Cédez le passage");
            }
            Self::OneWay { arrow } | Self::Curve { arrow } => {
                egui::ComboBox::from_id_salt("sign-arrow")
                    .selected_text(arrow.keyword())
                    .show_ui(ui, |ui| {
                        for a in [SignArrow::Left, SignArrow::Right] {
                            ui.selectable_value(arrow, a, a.keyword());
                        }
                    });
            }
            Self::Highway {
                destination,
                secondary,
                exit,
                arrow,
            } => {
                ui.label("Destination");
                ui.add(
                    egui::TextEdit::singleline(destination)
                        .desired_width(180.)
                        .char_limit(32),
                );
                ui.label("Second line");
                ui.add(
                    egui::TextEdit::singleline(secondary)
                        .desired_width(180.)
                        .char_limit(32),
                );
                ui.label("Exit");
                ui.add(
                    egui::TextEdit::singleline(exit)
                        .desired_width(65.)
                        .char_limit(12),
                );
                egui::ComboBox::from_id_salt("sign-arrow")
                    .selected_text(arrow.keyword())
                    .show_ui(ui, |ui| {
                        for a in SignArrow::ALL {
                            ui.selectable_value(arrow, a, a.keyword());
                        }
                    });
            }
        }
        *self != before
    }
}
