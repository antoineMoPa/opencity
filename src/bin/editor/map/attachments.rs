//! Cross-section element geometry and persistent lane attachments.

use super::*;

pub fn element_world_width(kind: opencity::RoadElementKind) -> f32 {
    use opencity::RoadElementKind;
    match kind {
        RoadElementKind::CarLane | RoadElementKind::BusLane => 0.045,
        RoadElementKind::ParkingLane | RoadElementKind::Sidewalk | RoadElementKind::BikeLane => {
            0.032
        }
        RoadElementKind::BusStop => 0.028,
        RoadElementKind::Middle => 0.025,
        RoadElementKind::Verge => 0.035,
        RoadElementKind::BuildingEdge => 0.012,
        RoadElementKind::Barrier => 0.008,
        RoadElementKind::TrainTrack => 0.055,
        RoadElementKind::TramTrack => 0.048,
        RoadElementKind::SubwayTrack => 0.052,
        RoadElementKind::Runway => 0.18,
        RoadElementKind::Taxiway => 0.075,
        RoadElementKind::Apron => 0.24,
    }
}

pub fn element_position(
    segment: &Segment,
    road: &RoadDefinition,
    element_index: usize,
    t: f32,
) -> Option<WorldPoint> {
    let element = road.elements.get(element_index)?;
    let total: f32 = road
        .elements
        .iter()
        .map(|element| element_world_width(element.kind))
        .sum();
    let before: f32 = road.elements[..element_index]
        .iter()
        .map(|element| element_world_width(element.kind))
        .sum();
    let offset = -total / 2.0 + before + element_world_width(element.kind) / 2.0;
    let tangent = segment.tangent_at(t);
    let dx = tangent.x;
    let dy = tangent.y;
    let length = (dx * dx + dy * dy).sqrt();
    if length <= 0.0001 {
        return None;
    }
    let point = segment.point_at(t);
    Some(WorldPoint {
        x: point.x + dy / length * offset,
        y: point.y - dx / length * offset,
    })
}

pub(super) fn element_direction(
    segment: &Segment,
    road: &RoadDefinition,
    element_index: usize,
    t: f32,
) -> Option<WorldPoint> {
    let before = element_position(segment, road, element_index, (t - 0.01).max(0.0))?;
    let after = element_position(segment, road, element_index, (t + 0.01).min(1.0))?;
    let dx = after.x - before.x;
    let dy = after.y - before.y;
    let length = (dx * dx + dy * dy).sqrt();
    (length > 0.0001).then_some(WorldPoint {
        x: dx / length,
        y: dy / length,
    })
}

pub fn anchored_reference(program: &Program, name: &str) -> bool {
    program.statements.iter().any(|statement| matches!(statement,
        Statement::Call(call) if call.name == "lane_point" && call.assigns.first().is_some_and(|assigned| assigned == name)))
        || intersection_port_direction(program, name).is_some()
}

pub fn lane_attachment(program: &Program, name: &str) -> Option<(String, usize, f32)> {
    program.statements.iter().find_map(|statement| {
        let Statement::Call(call) = statement else {
            return None;
        };
        if call.name != "lane_point" || call.assigns.first().map(String::as_str) != Some(name) {
            return None;
        }
        let Argument::Identifier(parent) = call.args.first()? else {
            return None;
        };
        let index = number(call.args.get(1)?)?;
        let t = number(call.args.get(2)?)?;
        Some((parent.clone(), index as usize, t))
    })
}

pub fn lane_splits(program: &Program) -> Vec<LaneSplit> {
    let projection = project(program);
    let mut splits: Vec<_> = program
        .statements
        .iter()
        .filter_map(|statement| {
            let Statement::Call(call) = statement else {
                return None;
            };
            if call.name != "lane_split" || !matches!(call.args.len(), 3 | 4) {
                return None;
            }
            let (
                Argument::Identifier(parent),
                Argument::Identifier(ramp),
                Argument::Identifier(mode),
            ) = (&call.args[0], &call.args[1], &call.args[2])
            else {
                return None;
            };
            let ramp_segment = projection
                .segments
                .iter()
                .find(|segment| segment.name.as_deref() == Some(ramp))?;
            let (attached_parent, element_index, t) =
                lane_attachment(program, ramp_segment.start_ref.as_deref()?)?;
            if attached_parent != *parent {
                return None;
            }
            Some(LaneSplit {
                parent: parent.clone(),
                ramp: ramp.clone(),
                full_ramp: if call.args.len() == 4 {
                    let Argument::Identifier(full) = &call.args[3] else {
                        return None;
                    };
                    Some(full.clone())
                } else {
                    None
                },
                element_index,
                t,
                mode: SplitMode::from_identifier(mode)?,
            })
        })
        .collect();
    for segment in &projection.segments {
        let Some(ramp) = segment.name.as_ref() else {
            continue;
        };
        if splits.iter().any(|split| split.ramp == *ramp) {
            continue;
        }
        let Some((parent, element_index, t)) = segment
            .start_ref
            .as_deref()
            .and_then(|reference| lane_attachment(program, reference))
        else {
            continue;
        };
        if parent == *ramp {
            continue;
        }
        splits.push(LaneSplit {
            parent,
            ramp: ramp.clone(),
            full_ramp: None,
            element_index,
            t,
            mode: SplitMode::Dedicated,
        });
    }
    splits
}

pub fn create_lane_split(
    program: &mut Program,
    reference: &str,
    ramp_source: SegmentSource,
    mode: SplitMode,
) -> bool {
    let Some((parent, _, _)) = lane_attachment(program, reference) else {
        return false;
    };
    let Some(ramp) = project(program)
        .segments
        .into_iter()
        .find(|segment| segment.source == ramp_source)
        .and_then(|segment| segment.name)
    else {
        return false;
    };
    program.statements.push(Statement::Call(Call {
        assigns: vec![],
        name: "lane_split".into(),
        args: vec![
            Argument::Identifier(parent),
            Argument::Identifier(ramp),
            Argument::Identifier(mode.identifier().into()),
        ],
    }));
    true
}

pub fn create_two_segment_split(
    program: &mut Program,
    reference: &str,
    entry_source: SegmentSource,
    full_source: SegmentSource,
    mode: SplitMode,
) -> bool {
    let Some((parent, _, _)) = lane_attachment(program, reference) else {
        return false;
    };
    let projection = project(program);
    let Some(entry) = projection
        .segments
        .iter()
        .find(|segment| segment.source == entry_source)
        .and_then(|segment| segment.name.clone())
    else {
        return false;
    };
    let Some(full) = projection
        .segments
        .iter()
        .find(|segment| segment.source == full_source)
        .and_then(|segment| segment.name.clone())
    else {
        return false;
    };
    program.statements.push(Statement::Call(Call {
        assigns: vec![],
        name: "lane_split".into(),
        args: vec![
            Argument::Identifier(parent),
            Argument::Identifier(entry),
            Argument::Identifier(mode.identifier().into()),
            Argument::Identifier(full),
        ],
    }));
    true
}

pub fn set_split_mode(program: &mut Program, ramp: &str, mode: SplitMode) -> bool {
    for statement in &mut program.statements {
        let Statement::Call(call) = statement else {
            continue;
        };
        if call.name == "lane_split"
            && matches!(call.args.len(), 3 | 4)
            && matches!(call.args.get(1), Some(Argument::Identifier(name)) if name == ramp)
        {
            call.args[2] = Argument::Identifier(mode.identifier().into());
            return true;
        }
    }
    let Some(split) = lane_splits(program)
        .into_iter()
        .find(|split| split.ramp == ramp)
    else {
        return false;
    };
    program.statements.push(Statement::Call(Call {
        assigns: vec![],
        name: "lane_split".into(),
        args: vec![
            Argument::Identifier(split.parent),
            Argument::Identifier(ramp.into()),
            Argument::Identifier(mode.identifier().into()),
        ],
    }));
    true
}

pub fn set_lane_attachment(
    program: &mut Program,
    name: &str,
    element_index: usize,
    t: f32,
) -> bool {
    let Some((parent_name, _, _)) = lane_attachment(program, name) else {
        return false;
    };
    let projection = project(program);
    let Some(parent) = projection
        .segments
        .iter()
        .find(|segment| segment.name.as_deref() == Some(&parent_name))
    else {
        return false;
    };
    let Some(road) = program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Road(road) if road.name == parent.road_name => Some(road),
            _ => None,
        })
    else {
        return false;
    };
    if !t.is_finite() || !(0.0..=1.0).contains(&t) {
        return false;
    }
    if element_position(parent, road, element_index, t).is_none() {
        return false;
    }
    for statement in &mut program.statements {
        let Statement::Call(call) = statement else {
            continue;
        };
        if call.name == "lane_point" && call.assigns.first().map(String::as_str) == Some(name) {
            call.args[1] = number_arg(element_index as f32);
            call.args[2] = number_arg(t);
            return true;
        }
    }
    false
}

pub fn update_lane_attachments(
    program: &mut Program,
    road_name: &str,
    edit: ElementEdit,
    before: &Projection,
) {
    let parent_names: HashSet<_> = before
        .segments
        .iter()
        .filter(|segment| segment.road_name == road_name)
        .filter_map(|segment| segment.name.as_deref())
        .collect();
    let mut detached = HashSet::new();
    for statement in &mut program.statements {
        let Statement::Call(call) = statement else {
            continue;
        };
        if call.name != "lane_point"
            || !matches!(call.args.first(),
            Some(Argument::Identifier(parent)) if parent_names.contains(parent.as_str()))
        {
            continue;
        }
        let Some(index) = call
            .args
            .get(1)
            .and_then(number)
            .map(|value| value as usize)
        else {
            continue;
        };
        let next = match edit {
            ElementEdit::Inserted(at) => Some(if index >= at { index + 1 } else { index }),
            ElementEdit::Swapped(a, b) => Some(if index == a {
                b
            } else if index == b {
                a
            } else {
                index
            }),
            ElementEdit::Removed(at) => {
                if index == at {
                    None
                } else {
                    Some(if index > at { index - 1 } else { index })
                }
            }
        };
        if let Some(next) = next {
            call.args[1] = number_arg(next as f32);
        } else {
            let Some(name) = call.assigns.first() else {
                continue;
            };
            let Some(position) = before
                .points
                .iter()
                .find(|point| point.name == *name)
                .map(|point| point.position)
            else {
                continue;
            };
            detached.insert(name.clone());
            call.name = "map_point".into();
            call.args = vec![number_arg(position.x), number_arg(position.y)];
        }
    }
    for segment in &before.segments {
        if segment
            .start_ref
            .as_ref()
            .is_some_and(|name| detached.contains(name))
        {
            set_control_direct(program, segment.source, false, segment.controls[0]);
        }
        if segment
            .end_ref
            .as_ref()
            .is_some_and(|name| detached.contains(name))
        {
            set_control_direct(program, segment.source, true, segment.controls[1]);
        }
    }
    if !detached.is_empty() {
        let detached_ramps: HashSet<_> = before
            .segments
            .iter()
            .filter(|segment| {
                segment
                    .start_ref
                    .as_ref()
                    .is_some_and(|name| detached.contains(name))
            })
            .filter_map(|segment| segment.name.as_deref())
            .collect();
        program.statements.retain(|statement| !matches!(statement,
            Statement::Call(call) if call.name == "lane_split" &&
                matches!(call.args.get(1), Some(Argument::Identifier(ramp)) if detached_ramps.contains(ramp.as_str()))));
    }
}

pub fn create_lane_point(
    program: &mut Program,
    source: SegmentSource,
    element_index: usize,
    t: f32,
) -> Option<(String, WorldPoint)> {
    let segment = project(program)
        .segments
        .into_iter()
        .find(|segment| segment.source == source && segment.source.editable())?;
    let road = program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Road(road) if road.name == segment.road_name => Some(road),
            _ => None,
        })?;
    let position = element_position(&segment, road, element_index, t)?;
    let direction = element_direction(&segment, road, element_index, t)?;
    let parent_name = if let Some(name) = segment.name {
        name
    } else {
        let mut suffix = 1;
        let name = loop {
            let candidate = format!("road_{suffix}");
            if name_available(program, &candidate, None) {
                break candidate;
            }
            suffix += 1;
        };
        set_name(program, source, name.clone());
        name
    };
    let mut suffix = 1;
    let point_name = loop {
        let candidate = format!("junction_{suffix}");
        if name_available(program, &candidate, None) {
            break candidate;
        }
        suffix += 1;
    };
    program.statements.push(Statement::Call(Call {
        assigns: vec![point_name.clone()],
        name: "lane_point".into(),
        args: vec![
            Argument::Identifier(parent_name),
            number_arg(element_index as f32),
            number_arg(t),
            number_arg(position.x),
            number_arg(position.y),
            number_arg(direction.x),
            number_arg(direction.y),
        ],
    }));
    Some((point_name, position))
}
