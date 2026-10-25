//! Projection transitions share the scene's homogeneous clip coordinates.
use super::*;

pub(super) type ClipMatrix = [[f32; 4]; 4];
const DURATION: f32 = 0.32;

#[derive(Clone, Copy)]
pub(super) struct CameraTransition {
    from: ClipMatrix,
    elapsed: f32,
    from_anchor: [f32; 3],
}

impl RoadEditor {
    pub(super) fn advance_camera_transition(&mut self, ctx: &egui::Context) {
        let keyboard_available = !ctx.egui_wants_keyboard_input()
            && !self.command_palette.is_open()
            && self.pending.is_none();
        let navigation = keyboard_available
            && ctx.input(|input| {
                let over_canvas = input.pointer.hover_pos().is_some_and(|point| {
                    self.map_rect.is_some_and(|rect| rect.contains(point))
                        && !self
                            .minimal
                            .view_switch_rect
                            .is_some_and(|rect| rect.contains(point))
                        && !self
                            .minimal
                            .toolbar_rect
                            .is_some_and(|rect| rect.contains(point))
                });
                (over_canvas
                    && (input.pointer.any_pressed()
                        || input.smooth_scroll_delta != egui::Vec2::ZERO))
                    || [
                        egui::Key::W,
                        egui::Key::S,
                        egui::Key::A,
                        egui::Key::D,
                        egui::Key::ArrowLeft,
                        egui::Key::ArrowRight,
                    ]
                    .iter()
                    .any(|key| input.key_down(*key))
            });
        if navigation {
            self.camera_transition = None;
        }
        if let Some(transition) = &mut self.camera_transition {
            transition.elapsed += ctx.input(|input| input.stable_dt).min(0.05);
            if transition.elapsed >= DURATION {
                self.camera_transition = None;
            } else {
                ctx.request_repaint();
            }
        }
    }

    pub(super) fn camera_transition_matrix(&self, rect: egui::Rect) -> Option<ClipMatrix> {
        let transition = self.camera_transition?;
        let t = (transition.elapsed / DURATION).clamp(0.0, 1.0);
        let t = t * t * (3.0 - 2.0 * t);
        let to = self.camera_clip_matrix(rect);
        let mut matrix = interpolate_clip(transition.from, to, t);
        let anchor = self.camera_visible_anchor();
        for row in [0, 1] {
            matrix[row][3] = -(matrix[row][0] * anchor[0]
                + matrix[row][1] * anchor[1]
                + matrix[row][2] * anchor[2]);
        }
        // Keep the interpolated vanishing plane attached to the moving anchor,
        // including interrupted perspective transitions far from world origin.
        let target_focus = self.camera_focus(rect);
        let target_anchor = [
            target_focus.x,
            target_focus.y,
            if self.camera_view == CameraView::Vehicular {
                self.vehicle_eye_height
            } else {
                0.0
            },
        ];
        for row in [2, 3] {
            let at = |matrix: ClipMatrix, anchor: [f32; 3]| {
                matrix[row][0] * anchor[0]
                    + matrix[row][1] * anchor[1]
                    + matrix[row][2] * anchor[2]
                    + matrix[row][3]
            };
            let anchor_clip =
                at(transition.from, transition.from_anchor) * (1.0 - t) + at(to, target_anchor) * t;
            matrix[row][3] = anchor_clip
                - matrix[row][0] * anchor[0]
                - matrix[row][1] * anchor[1]
                - matrix[row][2] * anchor[2];
        }
        Some(matrix)
    }
}

/// Map camera heading/elevation explicitly so opposite headings never collapse
/// the view axes, as a direct interpolation of matrix coefficients would.
fn interpolate_clip(from: ClipMatrix, to: ClipMatrix, t: f32) -> ClipMatrix {
    if t <= 0.0 {
        return from;
    }
    if t >= 1.0 {
        return to;
    }
    let mix = |a: f32, b: f32| a * (1.0 - t) + b * t;
    let yaw = |matrix: ClipMatrix| matrix[0][1].atan2(matrix[0][0]);
    let elevation = |matrix: ClipMatrix| matrix[1][0].hypot(matrix[1][1]).atan2(matrix[1][2]);
    let start = yaw(from);
    let delta = (yaw(to) - start).sin().atan2((yaw(to) - start).cos());
    let angle = start + delta * t;
    let elevation = mix(elevation(from), elevation(to));
    let magnitude = |row: [f32; 4]| (row[0] * row[0] + row[1] * row[1] + row[2] * row[2]).sqrt();
    let horizontal = mix(magnitude(from[0]), magnitude(to[0]));
    let vertical = mix(magnitude(from[1]), magnitude(to[1]));
    let perspective = mix(magnitude(from[3]), magnitude(to[3]));
    let mut matrix = std::array::from_fn(|row| {
        std::array::from_fn(|column| mix(from[row][column], to[row][column]))
    });
    matrix[0][0] = horizontal * angle.cos();
    matrix[0][1] = horizontal * angle.sin();
    matrix[0][2] = 0.0;
    matrix[1][0] = -vertical * angle.sin() * elevation.sin();
    matrix[1][1] = vertical * angle.cos() * elevation.sin();
    matrix[1][2] = vertical * elevation.cos();
    matrix[3][0] = perspective * angle.sin() * elevation.cos();
    matrix[3][1] = -perspective * angle.cos() * elevation.cos();
    matrix[3][2] = -perspective * elevation.sin();
    matrix
}

impl RoadEditor {
    pub(super) fn start_camera_transition(&mut self, from: ClipMatrix, from_anchor: [f32; 3]) {
        self.camera_transition = Some(CameraTransition {
            from,
            elapsed: 0.0,
            from_anchor,
        });
    }

    pub(super) fn camera_visible_anchor(&self) -> [f32; 3] {
        let rect = self.map_rect.unwrap_or_else(|| {
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))
        });
        let focus = self.camera_focus(rect);
        let anchor = [
            focus.x,
            focus.y,
            if self.camera_view == CameraView::Vehicular {
                self.vehicle_eye_height
            } else {
                0.0
            },
        ];
        if let Some(transition) = self.camera_transition {
            let t = (transition.elapsed / DURATION).clamp(0.0, 1.0);
            let t = t * t * (3.0 - 2.0 * t);
            std::array::from_fn(|axis| transition.from_anchor[axis] * (1.0 - t) + anchor[axis] * t)
        } else {
            anchor
        }
    }

    pub(super) fn camera_focus(&self, rect: egui::Rect) -> WorldPoint {
        match self.camera_view {
            CameraView::Top => world_of(rect, self.map_pan, self.map_zoom, rect.center()),
            CameraView::Isometric => iso_ground_point(
                rect,
                self.map_pan,
                self.map_zoom,
                self.orbit_target,
                self.orbit_angle,
                self.orbit_elevation,
                rect.center(),
            ),
            CameraView::Vehicular => self.vehicle_position,
        }
    }

    pub(super) fn camera_clip_matrix(&self, rect: egui::Rect) -> ClipMatrix {
        let project = |x: f32, y: f32, height: f32| {
            if self.camera_view == CameraView::Vehicular {
                let dx = x - self.vehicle_position.x;
                let dy = y - self.vehicle_position.y;
                let forward = dx * self.vehicle_heading.cos() + dy * self.vehicle_heading.sin();
                let right = -dx * self.vehicle_heading.sin() + dy * self.vehicle_heading.cos();
                let focal = rect.height() * 0.714;
                let scale = self.map_zoom / focal;
                [
                    right * focal * 2.0 / rect.width() * scale,
                    (height - self.vehicle_eye_height) * 1.428 * scale,
                    0.04 * scale,
                    forward * scale,
                ]
            } else {
                let point = WorldPoint { x, y };
                let screen = if self.camera_view == CameraView::Top {
                    screen_of(rect, self.map_pan, self.map_zoom, point)
                } else {
                    iso_screen(
                        rect,
                        self.map_pan,
                        self.map_zoom,
                        self.orbit_target,
                        self.orbit_angle,
                        self.orbit_elevation,
                        point,
                        height,
                    )
                } - rect.center();
                let distance = if self.camera_view == CameraView::Top {
                    -height * 8.0
                } else {
                    -(x - self.orbit_target.x) * self.orbit_angle.sin()
                        + (y - self.orbit_target.y) * self.orbit_angle.cos()
                        - height * self.orbit_elevation.tan()
                };
                [
                    2.0 * screen.x / rect.width(),
                    -2.0 * screen.y / rect.height(),
                    0.5 - distance / 2000.0,
                    1.0,
                ]
            }
        };
        let origin = project(0.0, 0.0, 0.0);
        let axes = [
            project(1.0, 0.0, 0.0),
            project(0.0, 1.0, 0.0),
            project(0.0, 0.0, 1.0),
        ];
        std::array::from_fn(|row| {
            [
                axes[0][row] - origin[row],
                axes[1][row] - origin[row],
                axes[2][row] - origin[row],
                origin[row],
            ]
        })
    }

    pub(super) fn camera_transition_canvas(&mut self, root: &mut egui::Ui) {
        egui::Frame::NONE.show(root, |ui| {
            let (rect, _) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            self.map_rect = Some(rect);
            let palette = editor_palette(ui.ctx().theme());
            let projection = self.projected_map();
            let scene = self.scene_geometry(ui.ctx(), palette, &projection);
            ui.painter_at(rect).add(egui::Shape::Callback(
                eframe::egui_wgpu::Callback::new_paint_callback(
                    rect,
                    scene3d::SceneCallback {
                        time_seconds: ui.input(|input| (input.time % 4.0) as f32),
                        transition: self.camera_transition_matrix(rect),
                        scene,
                        rect,
                        pan: self.map_pan,
                        zoom: self.map_zoom,
                        target: self.orbit_target,
                        angle: self.orbit_angle,
                        elevation: self.orbit_elevation,
                        eye_height: self.vehicle_eye_height,
                        camera: self.camera_view,
                        background: palette.map_background,
                        target_format: self.render_format,
                    },
                ),
            ));
            ui.ctx().request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(matrix: ClipMatrix, point: [f32; 3]) -> [f32; 4] {
        std::array::from_fn(|row| {
            matrix[row][0] * point[0]
                + matrix[row][1] * point[1]
                + matrix[row][2] * point[2]
                + matrix[row][3]
        })
    }

    #[test]
    fn distant_focus_stays_centered_through_rotation_and_interruption() {
        let mut editor = crate::tests::sample_editor();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        editor.map_rect = Some(rect);
        editor.map_zoom = 120.0;
        editor.map_pan = egui::vec2(-12000.0, -8400.0);
        for destination in [
            CameraView::Isometric,
            CameraView::Top,
            CameraView::Isometric,
        ] {
            editor.set_camera_view(destination);
            for fraction in [0.25, 0.5, 0.75] {
                editor.camera_transition.as_mut().unwrap().elapsed = DURATION * fraction;
                let matrix = editor.camera_transition_matrix(rect).unwrap();
                let point = clip(matrix, [100.0, -70.0, 0.0]);
                assert!(
                    point[0].abs() < 0.001 && point[1].abs() < 0.001,
                    "focus drift {point:?}"
                );
                let right = matrix[0][0].hypot(matrix[0][1]);
                assert!(right > 0.1, "rotation must not collapse axes");
            }
            // Switch again before finishing, preserving the visible anchor.
        }
    }

    #[test]
    fn vehicular_matrix_matches_perspective_including_layer_depth() {
        let mut editor = crate::tests::sample_editor();
        editor.camera_view = CameraView::Vehicular;
        editor.vehicle_position = WorldPoint { x: 2.0, y: 3.0 };
        editor.vehicle_heading = 0.7;
        editor.vehicle_eye_height = 0.127;
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let matrix = editor.camera_clip_matrix(rect);
        let scale = editor.map_zoom / (rect.height() * 0.714);
        for forward in [0.05, 0.3, 20.0] {
            let point = [
                2.0 + forward * 0.7_f32.cos(),
                3.0 + forward * 0.7_f32.sin(),
                0.1,
            ];
            let projected = clip(matrix, point);
            let bias = 0.005;
            let actual_depth = (projected[2]
                - 0.04 * bias * scale * scale / projected[3].abs().max(0.04 * scale))
                / projected[3];
            let expected = 0.04 * (1.0 - bias / forward.max(0.04)) / forward;
            assert!((actual_depth - expected).abs() < 0.0001);
        }
    }
}
