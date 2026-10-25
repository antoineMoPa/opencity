//! Viewport vehicle for the city editor.
use super::*;

impl RoadEditor {
    pub(super) fn vehicular_canvas(&mut self, root: &mut egui::Ui) {
        const VEHICLE_TRANSLATION_SPEED: f32 = 0.3;
        const VEHICLE_SCROLL_SPEED: f32 = 0.0003;
        egui::Frame::NONE.show(root, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            self.map_rect = Some(rect);
            let palette = editor_palette(ui.ctx().theme());
            let projection = self.projected_map();
            if self.fit_scene_once {
                self.fit_scene_once = false;
                self.place_vehicle_camera();
            }
            let previous_position = self.vehicle_position;
            let keyboard_available = !ui.ctx().egui_wants_keyboard_input();
            if keyboard_available && response.dragged_by(egui::PointerButton::Primary) {
                self.vehicle_heading -= response.drag_delta().x * 0.006;
            }
            if keyboard_available && response.hovered() {
                let scroll = ui.input(|input| input.smooth_scroll_delta.y);
                let step = scroll * VEHICLE_SCROLL_SPEED;
                self.vehicle_position.x += self.vehicle_heading.cos() * step;
                self.vehicle_position.y += self.vehicle_heading.sin() * step;
            }
            if (response.has_focus() || response.hovered()) && keyboard_available {
                let moving = ui.input(|input| {
                    let forward = f32::from(input.key_down(egui::Key::W))
                        - f32::from(input.key_down(egui::Key::S));
                    let side = f32::from(input.key_down(egui::Key::D))
                        - f32::from(input.key_down(egui::Key::A));
                    let turn = f32::from(input.key_down(egui::Key::ArrowRight))
                        - f32::from(input.key_down(egui::Key::ArrowLeft));
                    let dt = input.stable_dt.min(0.05);
                    self.vehicle_heading += turn * 1.5 * dt;
                    let step = VEHICLE_TRANSLATION_SPEED * dt;
                    self.vehicle_position.x += step
                        * (forward * self.vehicle_heading.cos()
                            - side * self.vehicle_heading.sin());
                    self.vehicle_position.y += step
                        * (forward * self.vehicle_heading.sin()
                            + side * self.vehicle_heading.cos());
                    forward != 0.0 || side != 0.0 || turn != 0.0
                });
                if moving {
                    ui.ctx().request_repaint();
                }
                if ui.input(|input| input.key_pressed(egui::Key::R)) {
                    self.place_vehicle_camera();
                }
            }
            if self.vehicle_position != previous_position {
                self.follow_selected_road_height(&projection);
            }
            let painter = ui.painter_at(rect);
            let scene = self.scene_geometry(ui.ctx(), palette, &projection);
            if scene.has_animated_water() {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(33));
            }
            painter.add(egui::Shape::Callback(
                eframe::egui_wgpu::Callback::new_paint_callback(
                    rect,
                    scene3d::SceneCallback {
                        time_seconds: ui.input(|input| (input.time % 4.0) as f32),
                        transition: self.camera_transition_matrix(rect),
                        scene,
                        rect,
                        pan: egui::Vec2::ZERO,
                        zoom: 1.0,
                        target: self.vehicle_position,
                        angle: self.vehicle_heading,
                        elevation: self.orbit_elevation,
                        eye_height: self.vehicle_eye_height,
                        camera: CameraView::Vehicular,
                        background: palette.map_background,
                        target_format: self.render_format,
                    },
                ),
            ));
            self.paint_annotations(ui, rect);
        });
    }
}
