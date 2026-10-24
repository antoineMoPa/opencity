//! Frame for the city editor.
use super::*;

impl eframe::App for RoadEditor {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.editor_frame_ui(root);
    }
}

impl RoadEditor {
    pub(super) fn editor_frame_ui(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        #[cfg(target_arch = "wasm32")]
        while let Ok(event) = self.web_events.try_recv() {
            match event {
                WebFileEvent::Opened { name, source } => match parse(&source) {
                    Ok(program) => {
                        self.load_program(program, Some(PathBuf::from(&name)));
                        self.status = format!("Opened {name}");
                    }
                    Err(error) => self.status = format!("Could not open {name}: {error}"),
                },
                WebFileEvent::Saved { name, program } => {
                    self.path = Some(PathBuf::from(&name));
                    self.saved_program = program;
                    self.dirty = self.program != self.saved_program;
                    self.status = format!("Saved {name}");
                }
                WebFileEvent::Message(message) => self.status = message,
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.mcp.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.handle_mcp(&ctx);
        let before = self.program.clone();
        // Native menu events arrive outside egui's input stream.
        #[cfg(target_os = "macos")]
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        #[cfg(target_os = "macos")]
        if let Some(menu) = &self.native_menu {
            let actions = menu.drain();
            for action in actions {
                self.file_action(action);
            }
        }
        if self.pending.is_none()
            && ctx.input_mut(|input| {
                input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::P,
                )
            })
        {
            self.minimal.palette_rename_focus =
                ctx.memory(|m| m.has_focus(egui::Id::new("placed-road-name")));
            self.command_palette.open();
        }
        let palette_active = self.command_palette.is_open();
        self.advance_camera_transition(&ctx);
        let switch_owns_pointer = self.view_switch_ui(root);
        if palette_active && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
        {
            self.set_camera_view(match self.camera_view {
                CameraView::Top => CameraView::Isometric,
                CameraView::Isometric => CameraView::Vehicular,
                CameraView::Vehicular => CameraView::Top,
            });
        }
        // The independent switch must not dismiss the palette as an outside click.
        let switch_input = switch_owns_pointer.then(|| ctx.input(|i| i.clone()));
        if switch_owns_pointer {
            ctx.input_mut(|i| {
                i.events.retain(|event| {
                    !matches!(
                        event,
                        egui::Event::PointerButton { .. } | egui::Event::PointerMoved(_)
                    )
                });
                i.pointer = Default::default();
            });
        }
        let palette_command = self.command_palette.show(&ctx, &self.palette_commands());
        if let Some(input) = switch_input {
            ctx.input_mut(|i| *i = input);
        }
        let shortcut = ctx.input_mut(|input| {
            let command = egui::Modifiers::COMMAND;
            if input.consume_key(command | egui::Modifiers::SHIFT, egui::Key::S) {
                Some(FileAction::SaveAs)
            } else if input.consume_key(command, egui::Key::S) {
                Some(FileAction::Save)
            } else if input.consume_key(command, egui::Key::O) {
                Some(FileAction::Open)
            } else if input.consume_key(command, egui::Key::N) {
                Some(FileAction::New)
            } else {
                None
            }
        });
        if let Some(action) = shortcut
            .filter(|_| !palette_active && self.pending.is_none())
            .filter(|_| {
                #[cfg(target_os = "macos")]
                {
                    self.native_menu.is_none()
                }
                #[cfg(not(target_os = "macos"))]
                {
                    true
                }
            })
        {
            self.file_action(action);
        }
        let text_editing =
            palette_active || self.pending.is_some() || ctx.egui_wants_keyboard_input();
        let history_shortcut = ctx.input_mut(|input| {
            if text_editing {
                return None;
            }
            let command = egui::Modifiers::COMMAND;
            if input.consume_key(command | egui::Modifiers::SHIFT, egui::Key::Z)
                || (cfg!(not(target_os = "macos")) && input.consume_key(command, egui::Key::Y))
            {
                Some(false)
            } else if input.consume_key(command, egui::Key::Z) {
                Some(true)
            } else {
                None
            }
        });
        if let Some(undo) = history_shortcut {
            self.restore_history(undo);
        }
        if !palette_active
            && self.pending.is_none()
            && self.view == View::Map
            && !ctx.egui_wants_keyboard_input()
        {
            let copy =
                ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::C));
            let paste =
                ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::V));
            if copy {
                self.copy_selection();
            }
            if paste {
                self.paste_selection();
            }
        }
        // The palette owns the entire gesture, including its dismissal frame.
        // Raw canvas handlers must not observe the overlay's keys or pointer.
        self.minimal.palette_pointer_gesture |=
            palette_active && ctx.input(|i| i.pointer.any_down());
        let palette_blocks_canvas = palette_active
            || self.pending.is_some()
            || self.minimal.palette_pointer_gesture
            || switch_owns_pointer;
        let saved_input = palette_blocks_canvas.then(|| ctx.input(|input| input.clone()));
        if palette_blocks_canvas {
            ctx.input_mut(|input| {
                input.events.clear();
                input.keys_down.clear();
                input.pointer = Default::default();

                input.smooth_scroll_delta = egui::Vec2::ZERO;
            });
        }
        self.minimal_editor_ui(root, palette_blocks_canvas);
        if let Some(input) = saved_input {
            ctx.input_mut(|current| *current = input);
        }
        if !ctx.input(|i| i.pointer.any_down()) {
            self.minimal.palette_pointer_gesture = false;
        }
        if palette_active && !self.command_palette.is_open() && self.minimal.palette_rename_focus {
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("placed-road-name")));
            self.minimal.palette_rename_focus = false;
        }
        if let Some(command) = palette_command {
            self.execute_palette_command(&command, &ctx);
        }

        self.pending_dialog(&ctx);
        self.delete_dialog(&ctx);
        let dragging = ctx.input(|input| input.pointer.primary_down())
            && (self.dragging_endpoint.is_some()
                || self.dragging_control.is_some()
                || self.dragging_intersection.is_some());
        self.record_history(before, dragging);
    }
}
