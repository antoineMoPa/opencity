//! Stateless connection discovery and validated precision editing.
use super::*;

#[derive(Clone)]
pub(super) struct Connection {
    id: String,
    label: String,
    center: WorldPoint,
    intersection: Option<map::Intersection>,
    segments: Vec<map::Segment>,
}

fn connections(program: &Program) -> Vec<Connection> {
    let projection = map::project(program);
    let mut result: Vec<_> = map::intersections(program, &projection)
        .into_iter()
        .map(|item| {
            let segments = projection
                .segments
                .iter()
                .filter(|segment| {
                    item.ports.iter().any(|port| {
                        segment.start_ref.as_deref() == Some(&port.point_ref)
                            || segment.end_ref.as_deref() == Some(&port.point_ref)
                    }) || segment.start_ref.as_deref() == Some(&item.point_ref)
                        || segment.end_ref.as_deref() == Some(&item.point_ref)
                })
                .cloned()
                .collect();
            Connection {
                id: format!("intersection:{}", item.point_ref),
                label: format!("Intersection · {}", item.point_ref),
                center: item.position,
                intersection: Some(item),
                segments,
            }
        })
        .collect();
    for segment in &projection.segments {
        let attached = [&segment.start_ref, &segment.end_ref]
            .into_iter()
            .flatten()
            .any(|name| map::lane_attachment(program, name).is_some());
        let merge = program.statements.iter().any(|statement| matches!(statement, Statement::Road(road) if road.name == segment.road_name && road.role == opencity::RoadRole::Merge));
        if !attached && !merge {
            continue;
        }
        // Stable assigned names prevent an old workspace from silently editing a different call.
        let Some(name) = &segment.name else {
            continue;
        };
        let mut segments = vec![segment.clone()];
        for reference in [&segment.start_ref, &segment.end_ref].into_iter().flatten() {
            let parent = map::lane_attachment(program, reference).map(|value| value.0);
            for other in &projection.segments {
                if other.source != segment.source
                    && (other.name == parent && parent.is_some()
                        || other.start_ref.as_ref() == Some(reference)
                        || other.end_ref.as_ref() == Some(reference))
                    && !segments.iter().any(|old| old.source == other.source)
                {
                    segments.push(other.clone());
                }
            }
        }
        result.push(Connection {
            id: format!("merge:{name}"),
            label: format!("Merge · {name}"),
            center: segment.point_at(0.5),
            intersection: None,
            segments,
        });
    }
    result
}

impl RoadEditor {
    pub(super) fn active_connection(&self) -> Option<Connection> {
        let items = connections(&self.program);
        if let Some(id) = &self.connection_id {
            return items.into_iter().find(|item| {
                &item.id == id
                    && (item.intersection.as_ref().is_some_and(|intersection| {
                        self.selected_intersection.as_ref() == Some(&intersection.point_ref)
                    }) || item.segments.iter().any(|road| {
                        Some(road.source) == self.selected_segment
                            && self.segment_name_draft == road.name.as_deref().unwrap_or_default()
                    }))
            });
        }
        items.into_iter().find(|item| {
            item.intersection.as_ref().is_some_and(|intersection| {
                self.selected_intersection.as_ref() == Some(&intersection.point_ref)
            }) || item.intersection.is_none()
                && item.segments.first().is_some_and(|segment| {
                    self.selected_segment == Some(segment.source)
                        && self.segment_name_draft == segment.name.as_deref().unwrap_or_default()
                })
        })
    }

    // Only viewport selection events replace the contextual member/API selection.
    // This deliberately does not focus the camera: clicking geometry must keep it still.
    pub(super) fn connection_selected_in_viewport(&mut self) {
        self.connection_id = None;
        self.connection_member = None;
        if let Some(item) = self.active_connection() {
            self.connection_id = Some(item.id);
        }
    }

    fn focus_connection(&mut self, id: &str) -> Result<(), String> {
        let item = connections(&self.program)
            .into_iter()
            .find(|item| item.id == id)
            .ok_or("Connection no longer exists")?;
        self.connection_id = Some(item.id.clone());
        self.view = View::Map;
        self.map_tool = MapTool::Select;
        self.selected_intersection = item
            .intersection
            .as_ref()
            .map(|item| item.point_ref.clone());
        self.selected_segment = item.segments.first().map(|segment| segment.source);
        self.segment_name_draft = item
            .segments
            .first()
            .and_then(|segment| segment.name.clone())
            .unwrap_or_default();
        self.selected_endpoint = None;

        let member = self
            .connection_member(&item)
            .and_then(|road| road.name.clone());
        if let Some(name) = member {
            let _ = self.select_connection_member(&item, &name);
        } else {
            self.selected_segments = self.selected_segment.into_iter().collect();
        }

        self.orbit_target = item.center;
        self.map_zoom = 600.0;
        self.map_pan = egui::vec2(
            -item.center.x * self.map_zoom,
            item.center.y * self.map_zoom,
        );
        if self.camera_view == CameraView::Isometric {
            self.map_pan = egui::Vec2::ZERO;
        }
        if self.camera_view == CameraView::Vehicular {
            self.place_connection_vehicle_camera(&item);
        }
        Ok(())
    }

    fn place_connection_vehicle_camera(&mut self, item: &Connection) {
        let Some(segment) = item.segments.first() else {
            return;
        };
        let start_is_connection = if let Some(intersection) = &item.intersection {
            segment.start_ref.as_ref().is_some_and(|reference| {
                reference == &intersection.point_ref
                    || intersection
                        .ports
                        .iter()
                        .any(|port| &port.point_ref == reference)
            })
        } else {
            segment
                .start_ref
                .as_deref()
                .is_some_and(|reference| map::lane_attachment(&self.program, reference).is_some())
        };
        let endpoint = if start_is_connection {
            segment.start
        } else {
            segment.end
        };
        // Stand a few lane widths back along the approach, even on a long parent road.
        let lane_width = map::element_world_width(RoadElementKind::CarLane);
        let mut t = if start_is_connection { 0.3 } else { 0.7 };
        for step in 1..=128 {
            let candidate = if start_is_connection {
                step as f32 / 128.0
            } else {
                1.0 - step as f32 / 128.0
            };
            t = candidate;
            let position = segment.point_at(t);
            if (position.x - endpoint.x).hypot(position.y - endpoint.y) >= lane_width * 3.0 {
                break;
            }
        }
        let profile = self
            .program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == segment.road_name => Some(road),
                _ => None,
            });
        self.vehicle_position = profile
            .and_then(|road| {
                road.elements
                    .iter()
                    .position(|element| element.kind == RoadElementKind::CarLane)
                    .and_then(|index| map::element_position(segment, road, index, t))
            })
            .unwrap_or_else(|| segment.point_at(t));
        let tangent = segment.tangent_at(t);
        let sign = if start_is_connection { -1.0 } else { 1.0 };
        self.vehicle_heading = (sign * tangent.y).atan2(sign * tangent.x);
        // Existing pavement/marking layers sit within 0.005 units of the surface.
        self.vehicle_eye_height = segment.height_at(t) + (lane_width * 0.6).max(0.01);
    }

    fn edit_connection(&mut self, id: &str, edit: ConnectionEdit) -> Result<(), String> {
        let item = connections(&self.program)
            .into_iter()
            .find(|item| item.id == id)
            .ok_or("Connection no longer exists")?;
        let mut next = self.program.clone();
        match edit {
            ConnectionEdit::Point {
                segment,
                point,
                position,
            } => {
                finite(&[position.x, position.y])?;
                let road = item
                    .segments
                    .iter()
                    .find(|road| road.name.as_deref() == Some(&segment))
                    .ok_or("Road is not part of this connection")?;
                if !road.source.editable() {
                    return Err("Block instances are read-only; edit the block definition".into());
                }
                let ok = match point {
                    ConnectionPoint::Start => {
                        map::set_connected_endpoint(&mut next, road.source, false, position)
                    }
                    ConnectionPoint::End => {
                        map::set_connected_endpoint(&mut next, road.source, true, position)
                    }
                    ConnectionPoint::First => {
                        map::set_control(&mut next, road.source, false, position)
                    }
                    ConnectionPoint::Second => {
                        map::set_control(&mut next, road.source, true, position)
                    }
                };
                if !ok {
                    return Err(
                        "Anchored endpoint: edit its lane attachment or intersection instead"
                            .into(),
                    );
                }
            }
            ConnectionEdit::Grade {
                segment,
                start,
                end,
            } => {
                finite(&[start, end])?;
                let road = item
                    .segments
                    .iter()
                    .find(|road| road.name.as_deref() == Some(&segment))
                    .ok_or("Road is not part of this connection")?;
                if !road.source.editable() || !set_grade(&mut next, &segment, start, end) {
                    return Err("Grade cannot be edited".into());
                }
            }
            ConnectionEdit::Attachment { reference, lane, t } => {
                finite(&[t])?;
                if !(0.0..=1.0).contains(&t) {
                    return Err("t must be between 0 and 1".into());
                }
                if !item.segments.iter().any(|road| {
                    road.start_ref.as_ref() == Some(&reference)
                        || road.end_ref.as_ref() == Some(&reference)
                }) {
                    return Err("Attachment is not part of this connection".into());
                }
                let (parent, _, _) =
                    map::lane_attachment(&next, &reference).ok_or("Not a lane attachment")?;
                let projection = map::project(&next);
                let parent = projection
                    .segments
                    .iter()
                    .find(|road| road.name.as_deref() == Some(&parent))
                    .ok_or("Missing parent")?;
                let valid = next.statements.iter().any(|statement| matches!(statement, Statement::Road(road) if road.name == parent.road_name && road.elements.get(lane).is_some_and(|element| element.kind == RoadElementKind::CarLane)));
                if !valid || !map::set_lane_attachment(&mut next, &reference, lane, t) {
                    return Err("Choose a parent car lane and valid t".into());
                }
            }
            ConnectionEdit::Intersection {
                position,
                kind,
                profile,
            } => {
                finite(&[position.x, position.y])?;
                let intersection = item.intersection.ok_or("Select an intersection")?;
                let offset = WorldPoint {
                    x: position.x - intersection.position.x,
                    y: position.y - intersection.position.y,
                };
                if (offset.x != 0.0 || offset.y != 0.0)
                    && !map::translate_intersection(&mut next, &intersection.point_ref, offset)
                {
                    return Err("Intersection cannot be moved".into());
                }
                if intersection.ports.is_empty() {
                    map::set_intersection(
                        &mut next,
                        &intersection.point_ref,
                        Some(kind),
                        Some(&profile),
                    )?;
                } else {
                    if kind != intersection.kind {
                        return Err("Port-based intersection type changes require rebuilding its ports; existing ports are preserved".into());
                    }
                    if !next.statements.iter().any(|statement| matches!(statement, Statement::Road(road) if road.name == profile)) { return Err("Unknown intersection profile".into()); }
                    let call = next.statements.iter_mut().find_map(|statement| match statement { Statement::Call(call) if call.name == kind.call_name() && matches!(call.args.first(), Some(Argument::Identifier(reference)) if reference == &intersection.point_ref) => Some(call), _ => None }).ok_or("Missing intersection definition")?;
                    call.args[1] = Argument::Identifier(profile);
                }
            }
        }
        if map::project(&next).unresolved > map::project(&self.program).unresolved {
            return Err("Edit would introduce unresolved geometry".into());
        }
        self.program = next;
        self.dirty = self.program != self.saved_program;
        Ok(())
    }

    fn select_connection_member(&mut self, item: &Connection, name: &str) -> Result<(), String> {
        let road = item
            .segments
            .iter()
            .find(|road| road.name.as_deref() == Some(name))
            .ok_or("Road is not part of this connection")?;
        self.connection_id = Some(item.id.clone());
        self.connection_member = Some((item.id.clone(), name.into()));
        self.selected_segment = Some(road.source);
        self.selected_segments = vec![road.source];
        self.segment_name_draft = name.into();
        self.selected_endpoint = None;
        Ok(())
    }

    fn connection_member<'a>(&self, item: &'a Connection) -> Option<&'a map::Segment> {
        self.connection_member
            .as_ref()
            .filter(|(id, _)| id == &item.id)
            .and_then(|(_, name)| {
                item.segments
                    .iter()
                    .find(|road| road.name.as_ref() == Some(name))
            })
            .or_else(|| {
                item.segments
                    .iter()
                    .find(|road| Some(road.source) == self.selected_segment)
            })
            .or_else(|| item.segments.first())
    }
}

fn finite(values: &[f32]) -> Result<(), String> {
    if values
        .iter()
        .all(|value| value.is_finite() && value.abs() <= 1_000_000.0)
    {
        Ok(())
    } else {
        Err("Values must be finite and within ±1000000".into())
    }
}
#[derive(Clone, Copy)]
enum ConnectionPoint {
    Start,
    First,
    Second,
    End,
}
enum ConnectionEdit {
    Point {
        segment: String,
        point: ConnectionPoint,
        position: WorldPoint,
    },
    Grade {
        segment: String,
        start: f32,
        end: f32,
    },
    Attachment {
        reference: String,
        lane: usize,
        t: f32,
    },
    Intersection {
        position: WorldPoint,
        kind: map::IntersectionKind,
        profile: String,
    },
}

#[cfg(not(target_arch = "wasm32"))]
impl RoadEditor {
    pub(super) fn connection_api(
        &mut self,
        action: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        use serde_json::json;
        let string = |key: &str| args[key].as_str().ok_or_else(|| format!("Missing {key}"));
        let number = |key: &str| -> Result<f32, String> {
            let value = args[key].as_f64().ok_or_else(|| format!("Missing {key}"))?;
            let value = value as f32;
            finite(&[value])?;
            Ok(value)
        };
        if action == "open_connection_editor" {
            if let Some(id) = args["connection_id"].as_str() {
                self.focus_connection(id)?;
            }
            self.camera_view = CameraView::Isometric;
            self.view = View::Map;
            self.map_tool = MapTool::Select;
        } else if action == "edit_connection" {
            let id = string("connection_id")?;
            let edit = match string("edit")? {
                "start" | "handle1" | "handle2" | "end" => ConnectionEdit::Point {
                    segment: string("segment")?.into(),
                    point: match string("edit")? {
                        "start" => ConnectionPoint::Start,
                        "handle1" => ConnectionPoint::First,
                        "handle2" => ConnectionPoint::Second,
                        "end" => ConnectionPoint::End,
                        _ => unreachable!(),
                    },
                    position: WorldPoint {
                        x: number("x")?,
                        y: number("y")?,
                    },
                },
                "grade" => ConnectionEdit::Grade {
                    segment: string("segment")?.into(),
                    start: number("start_height")?,
                    end: number("end_height")?,
                },
                "attachment" => ConnectionEdit::Attachment {
                    reference: string("reference")?.into(),
                    lane: usize::try_from(args["lane"].as_u64().ok_or("Missing lane index")?)
                        .map_err(|_| "Lane index too large")?,
                    t: number("t")?,
                },
                "intersection" => ConnectionEdit::Intersection {
                    position: WorldPoint {
                        x: number("x")?,
                        y: number("y")?,
                    },
                    kind: match string("kind")? {
                        "right_turn" => map::IntersectionKind::RightTurn,
                        "three_way" => map::IntersectionKind::ThreeWay,
                        "four_way" => map::IntersectionKind::FourWay,
                        _ => return Err("Invalid intersection kind".into()),
                    },
                    profile: string("profile")?.into(),
                },
                _ => return Err("Unknown connection edit".into()),
            };
            self.edit_connection(id, edit)?;
        }
        let items: Vec<_> = connections(&self.program).into_iter().map(|item| json!({"id":item.id,"label":item.label,"center":{"x":item.center.x,"y":item.center.y},"kind":item.intersection.as_ref().map(|intersection| match intersection.kind { map::IntersectionKind::RightTurn => "right_turn", map::IntersectionKind::ThreeWay => "three_way", map::IntersectionKind::FourWay => "four_way" }),"profile":item.intersection.as_ref().map(|intersection| &intersection.road_name),"ports":item.intersection.as_ref().map(|intersection| intersection.ports.iter().map(|port| json!({"reference":port.point_ref,"x":port.position.x,"y":port.position.y})).collect::<Vec<_>>()),"roads":item.segments.iter().map(|road| json!({"name":road.name,"profile":road.road_name,"editable":road.source.editable(),"start":{"x":road.start.x,"y":road.start.y,"reference":road.start_ref},"handle1":{"x":road.controls[0].x,"y":road.controls[0].y},"handle2":{"x":road.controls[1].x,"y":road.controls[1].y},"end":{"x":road.end.x,"y":road.end.y,"reference":road.end_ref},"start_height":road.start_z,"end_height":road.end_z,"attachments":[road.start_ref.as_deref().and_then(|reference| map::lane_attachment(&self.program, reference).map(|(parent,lane,t)| json!({"reference":reference,"parent":parent,"lane":lane,"t":t}))),road.end_ref.as_deref().and_then(|reference| map::lane_attachment(&self.program, reference).map(|(parent,lane,t)| json!({"reference":reference,"parent":parent,"lane":lane,"t":t})))]})).collect::<Vec<_>>()})).collect();
        let analysis = self
            .design_analysis_cache
            .as_ref()
            .filter(|cache| cache.program == self.program);
        Ok(
            json!({"workspace":serde_json::Value::Null,"active":self.active_connection().map(|item| item.id),"connections":items,"design_checks":if analysis.is_some() { "current" } else { "pending" },"issues":analysis.map(|cache| cache.issues.iter().map(|issue| json!({"code":issue.code,"message":issue.message,"x":issue.at.x,"y":issue.at.y})).collect::<Vec<_>>())}),
        )
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "connection/tests.rs"]
mod tests;
