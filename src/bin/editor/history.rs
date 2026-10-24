//! History for the city editor.
use super::*;

impl RoadEditor {
    pub(super) fn restore_history(&mut self, undo: bool) -> bool {
        self.camera_transition = None;
        self.cancel_placement();
        let target = if undo {
            &mut self.undo_stack
        } else {
            &mut self.redo_stack
        };
        if let Some(previous) = target.pop() {
            self.cancel_placement();
            let vehicle_initialized = self.minimal.vehicle_initialized;
            let last_edit_view = self.minimal.last_edit_view;
            self.minimal = minimal::MinimalEditor::default();
            self.minimal.vehicle_initialized = vehicle_initialized;
            self.minimal.last_edit_view = last_edit_view;
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.connection_member = None;
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.connection_id = None;
            }
            let current = std::mem::replace(&mut self.program, previous);
            let destination = if undo {
                &mut self.redo_stack
            } else {
                &mut self.undo_stack
            };
            destination.push(current);
            self.selected_segment = None;
            self.selected_segments.clear();
            self.box_start = None;
            self.iso_drag_pan = false;
            self.iso_drag_orbit = false;
            self.selected_intersection = None;
            self.selected_endpoint = None;
            self.map_start = None;
            self.map_start_ref = None;
            self.selected_road = self.selected_road.filter(|index| {
                matches!(
                    self.program.statements.get(*index),
                    Some(Statement::Road(_))
                )
            });
            self.road_name_draft = self
                .road()
                .map(|road| road.name.clone())
                .unwrap_or_default();
            self.dirty = self.program != self.saved_program;
            self.status = if undo { "Undid edit" } else { "Redid edit" }.into();
            self.skip_history = true;
            true
        } else {
            false
        }
    }

    pub(super) fn record_history(&mut self, before: Program, dragging: bool) {
        if self.skip_history {
            self.skip_history = false;
        } else if self.program != before {
            if !self.history_drag_active {
                self.undo_stack.push(before);
                if self.undo_stack.len() > 100 {
                    self.undo_stack.remove(0);
                }
                self.redo_stack.clear();
            }
            self.history_drag_active = dragging;
        }
        if !dragging {
            self.history_drag_active = false;
        }
        self.dirty = self.program != self.saved_program;
    }
}
