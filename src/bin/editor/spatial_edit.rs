//! Direct road handles use the same AST mutations as the socket API.
use super::*;

/// Find the closest actual car-lane center in screen space. Sampling first and
/// refining locally handles curved lanes and elevation without ground-plane guesses.
fn lane_at_pointer(
    segment: &map::Segment,
    road: &RoadDefinition,
    pointer: egui::Pos2,
    compatible_with: Option<usize>,
    project: impl Fn(WorldPoint, f32) -> egui::Pos2,
) -> Option<(usize, f32, f32)> {
    road.elements
        .iter()
        .enumerate()
        .filter(|(_, element)| element.kind == RoadElementKind::CarLane)
        .filter(|(lane, _)| {
            compatible_with
                .is_none_or(|original| road.lane_direction(original) == road.lane_direction(*lane))
        })
        .filter_map(|(lane, _)| {
            let distance = |t| {
                map::element_position(segment, road, lane, t)
                    .map(|point| pointer.distance_sq(project(point, segment.height_at(t))))
                    .unwrap_or(f32::INFINITY)
            };
            let step = (0..=128).min_by(|a, b| {
                distance(*a as f32 / 128.0).total_cmp(&distance(*b as f32 / 128.0))
            })?;
            let mut lo = ((step as f32 - 1.0) / 128.0).max(0.0);
            let mut hi = ((step as f32 + 1.0) / 128.0).min(1.0);
            for _ in 0..12 {
                let a = lo + (hi - lo) / 3.0;
                let b = hi - (hi - lo) / 3.0;
                if distance(a) < distance(b) {
                    hi = b;
                } else {
                    lo = a;
                }
            }
            let t = (lo + hi) * 0.5;
            Some((lane, t, distance(t)))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
}

impl RoadEditor {
    pub(super) fn endpoint_attached(&self, source: SegmentSource, is_end: bool) -> bool {
        map::project(&self.program)
            .segments
            .iter()
            .find(|s| s.source == source)
            .and_then(|s| (if is_end { &s.end_ref } else { &s.start_ref }).as_deref())
            .is_some_and(|r| map::lane_attachment(&self.program, r).is_some())
    }
    pub(super) fn spatial_screen(
        &self,
        rect: egui::Rect,
        point: WorldPoint,
        height: f32,
        iso: bool,
    ) -> egui::Pos2 {
        if iso {
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
        } else {
            screen_of(rect, self.map_pan, self.map_zoom, point)
        }
    }

    pub(super) fn drag_lane_attachment(
        &mut self,
        source: SegmentSource,
        is_end: bool,
        pointer: egui::Pos2,
        rect: egui::Rect,
        projection: &map::Projection,
        iso: bool,
    ) -> bool {
        let Some(segment) = projection.segments.iter().find(|s| s.source == source) else {
            return false;
        };
        let reference = if is_end {
            &segment.end_ref
        } else {
            &segment.start_ref
        };
        let Some(reference) = reference.as_deref() else {
            return false;
        };
        let Some((parent_name, original_lane, _)) = map::lane_attachment(&self.program, reference)
        else {
            return false;
        };
        let Some(parent) = projection
            .segments
            .iter()
            .find(|s| s.name.as_deref() == Some(&parent_name))
        else {
            return true;
        };
        let Some(road) = road_for_segment(&self.program, parent) else {
            return true;
        };
        if let Some((lane, t, _)) =
            lane_at_pointer(parent, road, pointer, Some(original_lane), |p, h| {
                self.spatial_screen(rect, p, h, iso)
            })
        {
            let height = parent.height_at(t);
            let mut next = self.program.clone();
            if map::set_lane_attachment(&mut next, reference, lane, t) {
                // lane_point owns shared XY; every placement using it must also
                // meet the new parent height. Leave opposite ends untouched.
                for child in projection.segments.iter().filter(|child| {
                    child.start_ref.as_deref() == Some(reference)
                        || child.end_ref.as_deref() == Some(reference)
                }) {
                    if !child.source.editable() {
                        self.status = "This attachment also belongs to a read-only block".into();
                        return true;
                    }
                    let name = if let Some(name) = &child.name {
                        name.clone()
                    } else {
                        let mut suffix = 1;
                        let name = loop {
                            let candidate = format!("attached_road_{suffix}");
                            if !projection.segments.iter().any(|s| s.name.as_deref() == Some(&candidate))
                                && !next.statements.iter().any(|s| matches!(s, Statement::Call(c) if c.assigns.contains(&candidate))) {
                                break candidate;
                            }
                            suffix += 1;
                        };
                        if !map::set_name(&mut next, child.source, name.clone()) {
                            return true;
                        }
                        name
                    };
                    let start = if child.start_ref.as_deref() == Some(reference) {
                        height
                    } else {
                        child.start_z
                    };
                    let end = if child.end_ref.as_deref() == Some(reference) {
                        height
                    } else {
                        child.end_z
                    };
                    if !set_grade(&mut next, &name, start, end) {
                        return true;
                    }
                }
                self.program = next;
                self.dirty = true;
            }
        }
        // An attached end is always constrained to its parent, including when a
        // malformed parent has no car lanes. Never fall through to free dragging.
        true
    }

    /// Paint direct geometry handles and transient hover cues.
    pub(super) fn spatial_edit_ui(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        projection: &map::Projection,
        iso: bool,
    ) -> bool {
        if self.map_tool != MapTool::Select {
            return false;
        }
        let Some(segment) = projection
            .segments
            .iter()
            .find(|s| Some(s.source) == self.selected_segment && s.source.editable())
        else {
            return false;
        };
        let Some(_road) = road_for_segment(&self.program, segment) else {
            return false;
        };
        let pointer = ui.ctx().pointer_hover_pos();
        let painter = ui
            .ctx()
            .layer_painter(egui::LayerId::new(
                // Above the canvas GPU callback, below foreground popups.
                egui::Order::Middle,
                egui::Id::new("spatial-road-handles"),
            ))
            .with_clip_rect(rect);
        let palette = editor_palette(ui.ctx().theme());
        let mut hover_label = None;
        for (is_end, point, height, reference) in [
            (
                false,
                segment.start,
                segment.start_z,
                segment.start_ref.as_deref(),
            ),
            (true, segment.end, segment.end_z, segment.end_ref.as_deref()),
        ] {
            let p = self.spatial_screen(rect, point, height, iso);
            let attached = reference
                .and_then(|r| map::lane_attachment(&self.program, r))
                .is_some();
            if !attached {
                painter.circle_filled(p, 6.0, palette.map_background);
                painter.circle_stroke(p, 6.0, egui::Stroke::new(2.0, palette.map_selected));
            }
            if attached {
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        p + egui::vec2(0.0, -8.0),
                        p + egui::vec2(8.0, 0.0),
                        p + egui::vec2(0.0, 8.0),
                        p + egui::vec2(-8.0, 0.0),
                    ],
                    palette.map_background,
                    egui::Stroke::new(2.0, palette.map_selected),
                ));
            }
            if pointer.is_some_and(|mouse| mouse.distance(p) < 12.0) {
                hover_label = Some((
                    p,
                    if attached {
                        "Drag along parent car lanes | click for options"
                    } else if is_end {
                        "Drag road end | click for options"
                    } else {
                        "Drag road start | click for options"
                    },
                ));
            }
        }
        for (index, point) in segment.controls.iter().enumerate() {
            let p = self.spatial_screen(
                rect,
                *point,
                segment.height_at((index + 1) as f32 / 3.0),
                iso,
            );
            let anchor = if index == 0 {
                segment.start
            } else {
                segment.end
            };
            let anchor_height = if index == 0 {
                segment.start_z
            } else {
                segment.end_z
            };
            painter.line_segment(
                [self.spatial_screen(rect, anchor, anchor_height, iso), p],
                egui::Stroke::new(1.0, palette.map_selected),
            );
            painter.rect_filled(
                egui::Rect::from_center_size(p, egui::vec2(10.0, 10.0)),
                1.0,
                palette.map_selected,
            );
            painter.rect_stroke(
                egui::Rect::from_center_size(p, egui::vec2(10.0, 10.0)),
                1.0,
                egui::Stroke::new(1.0, palette.map_background),
                egui::StrokeKind::Inside,
            );
            if pointer.is_some_and(|mouse| mouse.distance(p) < 11.0) {
                hover_label = Some((p, "Drag to shape curve"));
            }
        }
        if let Some((p, label)) = hover_label.filter(|_| self.point_options.is_none()) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            painter.text(
                p + egui::vec2(12.0, -14.0),
                egui::Align2::LEFT_BOTTOM,
                label,
                egui::FontId::proportional(12.0),
                palette.map_text,
            );
        }
        // While moving an attachment, highlight its exact parent lane and marker.
        if let Some((source, is_end)) = self.dragging_endpoint {
            if let Some(s) = projection.segments.iter().find(|s| s.source == source) {
                if let Some((parent, lane, t)) = (if is_end { &s.end_ref } else { &s.start_ref })
                    .as_deref()
                    .and_then(|r| map::lane_attachment(&self.program, r))
                {
                    if let Some(parent) = projection
                        .segments
                        .iter()
                        .find(|s| s.name.as_deref() == Some(&parent))
                    {
                        if let Some(road) = road_for_segment(&self.program, parent) {
                            self.paint_lane_preview(
                                &painter, rect, parent, road, lane, t, iso, palette,
                            );
                        }
                    }
                }
            }
        }
        false
    }

    fn paint_lane_preview(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        segment: &map::Segment,
        road: &RoadDefinition,
        lane: usize,
        t: f32,
        iso: bool,
        palette: EditorPalette,
    ) {
        let path: Vec<_> = (0..=128)
            .filter_map(|step| {
                let t = step as f32 / 128.0;
                map::element_position(segment, road, lane, t)
                    .map(|p| self.spatial_screen(rect, p, segment.height_at(t), iso))
            })
            .collect();
        painter.add(egui::Shape::line(
            path,
            egui::Stroke::new(3.0, palette.map_preview),
        ));
        if let Some(point) = map::element_position(segment, road, lane, t) {
            let p = self.spatial_screen(rect, point, segment.height_at(t), iso);
            painter.circle_filled(p, 7.0, palette.map_background);
            painter.circle_stroke(p, 7.0, egui::Stroke::new(2.5, palette.map_preview));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> RoadEditor {
        let mut editor = super::super::tests::sample_editor();
        editor.program = parse(
            "road highway: sidewalk-carlane-carlane; direction forward; end; \
            road link: carlane; direction forward; role merge; end; \
            parent = highway(0,0,2,0,4,0,6,0); grade(parent,0,1); \
            anchor = lane_point(parent,1,0.25); \
            far=map_point(4,2); ramp = link(anchor,2,1,3,2,far); grade(ramp,0.25,0.8); \
            far_shared=map_point(4,-2); shared = link(anchor,2,-1,3,-2,far_shared); grade(shared,0.25,0.9); \
            deck = highway(0,0,2,0,4,0,6,0); elevation(deck,4);",
        )
        .unwrap();
        editor.saved_program = editor.program.clone();
        editor.fit_scene_once = false;
        editor.map_zoom = 80.0;
        editor.orbit_target = WorldPoint { x: 3.0, y: 0.0 };
        editor.map_pan = egui::vec2(-240.0, 0.0);
        editor.selected_segment = Some(
            map::project(&editor.program)
                .segments
                .iter()
                .find(|s| s.name.as_deref() == Some("ramp"))
                .unwrap()
                .source,
        );
        editor.selected_segments = editor.selected_segment.into_iter().collect();
        editor
    }
    fn input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 650.0),
            )),
            events,
            ..Default::default()
        }
    }
    fn button(point: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn canvas_frame(
        editor: &mut RoadEditor,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let before = editor.program.clone();
        let mut output = ctx.run_ui(input(events), |ui| editor.map_canvas(ui));
        output.textures_delta.clear();
        editor.record_history(
            before,
            editor.dragging_endpoint.is_some() || editor.dragging_control.is_some(),
        );
        output
    }
    #[test]
    fn spatial_attachment_pointer_drag_preserves_shared_refs_grade_and_undo_in_both_views() {
        for iso in [false, true] {
            let mut editor = editor();
            editor.camera_view = if iso {
                CameraView::Isometric
            } else {
                CameraView::Top
            };
            if iso {
                editor.map_pan = egui::Vec2::ZERO;
            }
            let ctx = egui::Context::default();
            let _ = canvas_frame(&mut editor, &ctx, vec![]);
            // An unrelated deck endpoint occupies the exact same screen pixel.
            // The selected road's visible handle must win the initial press.
            let snapshot = map::project(&editor.program);
            let selected = snapshot
                .segments
                .iter()
                .find(|s| Some(s.source) == editor.selected_segment)
                .unwrap();
            let rect = editor.map_rect.unwrap();
            let screen = editor.spatial_screen(rect, selected.start, selected.start_z, iso);
            let overlap = if iso {
                iso_world_at_height(
                    rect,
                    editor.map_pan,
                    editor.map_zoom,
                    editor.orbit_target,
                    editor.orbit_angle,
                    editor.orbit_elevation,
                    screen,
                    4.0,
                )
            } else {
                selected.start
            };
            let road = road_for_segment(&editor.program, selected).unwrap().clone();
            let other = map::append(
                &mut editor.program,
                &road,
                overlap,
                WorldPoint { x: 7.0, y: -2.0 },
            );
            let Statement::Call(call) = &editor.program.statements[other] else {
                unreachable!()
            };
            let other_name = call.assigns[0].clone();
            set_grade(&mut editor.program, &other_name, 4.0, 4.0);
            let _ = canvas_frame(&mut editor, &ctx, vec![]);
            let original = editor.program.clone();
            let rect = editor.map_rect.unwrap();
            let projection = map::project(&editor.program);
            let parent = projection
                .segments
                .iter()
                .find(|s| s.name.as_deref() == Some("parent"))
                .unwrap();
            let ramp = projection
                .segments
                .iter()
                .find(|s| s.name.as_deref() == Some("ramp"))
                .unwrap();
            let road = road_for_segment(&editor.program, parent).unwrap();
            let start = editor.spatial_screen(rect, ramp.start, ramp.start_z, iso);
            let target = editor.spatial_screen(
                rect,
                map::element_position(parent, road, 2, 0.72).unwrap(),
                parent.height_at(0.72),
                iso,
            );
            let _ = canvas_frame(
                &mut editor,
                &ctx,
                vec![egui::Event::PointerMoved(start), button(start, true)],
            );
            let _ = canvas_frame(&mut editor, &ctx, vec![egui::Event::PointerMoved(target)]);
            let _ = canvas_frame(&mut editor, &ctx, vec![button(target, false)]);
            assert_eq!(editor.selected_segment, Some(ramp.source));
            assert_eq!(
                opencity::grade_of(&editor.program, &other_name),
                Some((4.0, 4.0))
            );
            let (_, lane, t) = map::lane_attachment(&editor.program, "anchor").unwrap();
            assert_eq!(lane, 2, "iso={iso}");
            assert!((t - 0.72).abs() < 0.002, "iso={iso}: {t}");
            let projection = map::project(&editor.program);
            for name in ["ramp", "shared"] {
                let s = projection
                    .segments
                    .iter()
                    .find(|s| s.name.as_deref() == Some(name))
                    .unwrap();
                assert_eq!(s.start_ref.as_deref(), Some("anchor"));
                assert!((s.start_z - 0.72).abs() < 0.002);
            }
            assert_eq!(opencity::grade_of(&editor.program, "ramp").unwrap().1, 0.8);
            assert_eq!(
                opencity::grade_of(&editor.program, "shared").unwrap().1,
                0.9
            );
            assert_eq!(opencity::elevation_of(&editor.program, "deck"), Some(4.0));
            assert_eq!(editor.undo_stack.len(), 1);
            assert!(editor.restore_history(true));
            assert_eq!(editor.program, original);
            assert!(editor.restore_history(false));
            assert!(
                (map::lane_attachment(&editor.program, "anchor").unwrap().2 - 0.72).abs() < 0.002
            );
        }
    }

    #[test]
    fn spatial_reverse_lane_branch_follows_traffic_and_retains_parent_grade() {
        let mut program=parse("road street: carlane-middle-carlane; end; parent=street(0,0,6,0); grade(parent,0.2,0.8);").unwrap();
        let projection = map::project(&program);
        let parent = &projection.segments[0];
        let road = road_for_segment(&program, parent).unwrap().clone();
        assert_eq!(road.lane_direction(0), Some(RoadDirection::Reverse));
        let (reference, start) =
            map::create_lane_point(&mut program, parent.source, 0, 0.6).unwrap();
        let exit = map::append_exit(
            &mut program,
            &road,
            map::SplitMode::KeepLane,
            start,
            &reference,
            WorldPoint { x: 1.0, y: 1.0 },
            None,
        )
        .unwrap();
        let built = map::project(&program);
        let entry = built
            .segments
            .iter()
            .find(|s| s.source == exit.entry)
            .unwrap();
        assert!(
            entry.controls[0].x < entry.start.x,
            "branch follows reverse lane travel"
        );
        assert!((entry.start_z - parent.height_at(0.6)).abs() < 0.001);
        assert_eq!(opencity::grade_of(&program, "parent"), Some((0.2, 0.8)));
    }

    #[test]
    fn spatial_lane_pick_rejects_sidewalk_opposing_lane_and_invalid_attachment_values() {
        let program=parse("road street: sidewalk-carlane-middle-carlane-sidewalk; end; parent=street(0,0,6,0); p=lane_point(parent,3,0.5);").unwrap();
        let projection = map::project(&program);
        let parent = &projection.segments[0];
        let road = road_for_segment(&program, parent).unwrap();
        let point = map::element_position(parent, road, 1, 0.6).unwrap();
        let project = |p: WorldPoint, _| egui::pos2(p.x * 100.0, p.y * 100.0);
        let (lane, _, _) =
            lane_at_pointer(parent, road, project(point, 0.0), Some(3), project).unwrap();
        assert_eq!(lane, 3);
        for (lane, t) in [(99, 0.5), (3, f32::NAN), (3, 1.1)] {
            let mut next = program.clone();
            assert!(!map::set_lane_attachment(&mut next, "p", lane, t));
            assert_eq!(next, program);
        }
    }
}
