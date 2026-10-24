//! Lane exit construction and parent cross-section transitions.

use super::*;

fn road_with_elements(
    program: &mut Program,
    base: &str,
    elements: Vec<RoadElement>,
    direction: RoadDirection,
) -> RoadDefinition {
    let mut name = base.to_owned();
    let mut suffix = 2;
    loop {
        if let Some(existing) = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == name => Some(road),
                _ => None,
            })
        {
            if existing.elements == elements && existing.direction == direction {
                return existing.clone();
            }
        } else if name_available(program, &name, None) {
            let road = RoadDefinition {
                role: opencity::RoadRole::Ordinary,
                name,
                elements,
                direction,
            };
            program.statements.push(Statement::Road(road.clone()));
            return road;
        }
        name = format!("{base}_{suffix}");
        suffix += 1;
    }
}

fn split_elements(
    parent: &RoadDefinition,
    selected: usize,
    side: ExitSide,
) -> (Vec<RoadElement>, Vec<RoadElement>) {
    match side {
        ExitSide::Right => (
            parent.elements[..selected].to_vec(),
            parent.elements[selected..].to_vec(),
        ),
        ExitSide::Left => (
            parent.elements[selected + 1..].to_vec(),
            parent.elements[..=selected].to_vec(),
        ),
    }
}

fn point_between(first: WorldPoint, second: WorldPoint, t: f32) -> WorldPoint {
    WorldPoint {
        x: first.x + (second.x - first.x) * t,
        y: first.y + (second.y - first.y) * t,
    }
}

fn append_remaining_after_split(
    program: &mut Program,
    parent: &Segment,
    road: &RoadDefinition,
    attachment: &str,
    t: f32,
    shift: f32,
) -> Option<SegmentSource> {
    let SegmentSource::TopLevel(parent_index) = parent.source else {
        return None;
    };
    let (start, start_ref) = link_endpoint(program, parent.source, false)?;
    let (_, end_ref) = link_endpoint(program, parent.source, true)?;
    let a = point_between(start, parent.controls[0], t);
    let b = point_between(parent.controls[0], parent.controls[1], t);
    let c = point_between(parent.controls[1], parent.end, t);
    let d = point_between(a, b, t);
    let e = point_between(b, c, t);
    let split_point = point_between(d, e, t);
    let split_ref = ensure_point(program, split_point, None);
    let Statement::Call(parent_call) = program.statements.get_mut(parent_index)? else {
        return None;
    };
    parent_call.args = vec![
        Argument::Identifier(start_ref),
        number_arg(a.x),
        number_arg(a.y),
        number_arg(d.x),
        number_arg(d.y),
        Argument::Identifier(split_ref),
    ];
    map_attachments_before_split(program, parent, t);
    placement_grade(program, parent.source, parent.start_z, parent.height_at(t));
    for statement in &mut program.statements {
        let Statement::Call(call) = statement else {
            continue;
        };
        if call.name == "lane_point" && call.assigns.first().is_some_and(|name| name == attachment)
        {
            call.args[2] = number_arg(1.0);
            break;
        }
    }
    let tangent = parent.tangent_at(t);
    let length = (tangent.x * tangent.x + tangent.y * tangent.y)
        .sqrt()
        .max(0.0001);
    let offset = WorldPoint {
        x: tangent.y / length * shift,
        y: -tangent.x / length * shift,
    };
    let segment_start = split_point.translated(offset);
    let (index, _) = append_connected(
        program,
        road,
        segment_start,
        None,
        parent.end,
        Some(&end_ref),
    );
    set_control_direct(
        program,
        SegmentSource::TopLevel(index),
        false,
        e.translated(offset),
    );
    set_control_direct(program, SegmentSource::TopLevel(index), true, c);
    placement_grade(
        program,
        SegmentSource::TopLevel(index),
        parent.height_at(t),
        parent.end_z,
    );
    Some(SegmentSource::TopLevel(index))
}

fn end_parent_at_split(
    program: &mut Program,
    parent: &Segment,
    attachment: &str,
    t: f32,
) -> Option<()> {
    let SegmentSource::TopLevel(parent_index) = parent.source else {
        return None;
    };
    let (_, start_ref) = link_endpoint(program, parent.source, false)?;
    let a = point_between(parent.start, parent.controls[0], t);
    let b = point_between(parent.controls[0], parent.controls[1], t);
    let c = point_between(parent.controls[1], parent.end, t);
    let d = point_between(a, b, t);
    let e = point_between(b, c, t);
    let split_ref = ensure_point(program, point_between(d, e, t), None);
    let Statement::Call(call) = program.statements.get_mut(parent_index)? else {
        return None;
    };
    call.args = vec![
        Argument::Identifier(start_ref),
        number_arg(a.x),
        number_arg(a.y),
        number_arg(d.x),
        number_arg(d.y),
        Argument::Identifier(split_ref),
    ];
    map_attachments_before_split(program, parent, t);
    placement_grade(program, parent.source, parent.start_z, parent.height_at(t));
    for statement in &mut program.statements {
        let Statement::Call(call) = statement else {
            continue;
        };
        if call.name == "lane_point" && call.assigns.first().is_some_and(|name| name == attachment)
        {
            call.args[2] = number_arg(1.0);
            break;
        }
    }
    Some(())
}

pub fn append_exit(
    program: &mut Program,
    _full_road: &RoadDefinition,
    mode: SplitMode,
    start: WorldPoint,
    start_ref: &str,
    end: WorldPoint,
    end_ref: Option<&str>,
) -> Option<ExitPlacement> {
    let (parent_name, element_index, mut split_t) = lane_attachment(program, start_ref)?;
    let projection = project(program);
    let parent = projection
        .segments
        .iter()
        .find(|segment| segment.name.as_deref() == Some(&parent_name))?;
    let parent_road = program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Road(road) if road.name == parent.road_name => Some(road.clone()),
            _ => None,
        })?;
    let branch_start_height = parent.height_at(split_t.max(if mode == SplitMode::Dedicated {
        0.002
    } else {
        0.0
    }));
    let branch_end_height =
        reference_height(program, &projection, end_ref).unwrap_or(branch_start_height);
    let start = if mode == SplitMode::Dedicated && split_t <= 0.001 {
        split_t = 0.002;
        let shifted = element_position(parent, &parent_road, element_index, split_t)?;
        let shifted_direction = element_direction(parent, &parent_road, element_index, split_t)?;
        for statement in &mut program.statements {
            let Statement::Call(call) = statement else {
                continue;
            };
            if call.name == "lane_point"
                && call.assigns.first().is_some_and(|name| name == start_ref)
            {
                call.args[2] = number_arg(split_t);
                call.args[3] = number_arg(shifted.x);
                call.args[4] = number_arg(shifted.y);
                call.args[5] = number_arg(shifted_direction.x);
                call.args[6] = number_arg(shifted_direction.y);
                break;
            }
        }
        shifted
    } else {
        start
    };
    let direction = attachment_direction(program, start_ref)?;
    // Profile bands are ordered relative to the parent curve, even when the
    // selected lane carries traffic against that curve's direction.
    let profile_direction = element_direction(parent, &parent_road, element_index, split_t)?;
    let cross = profile_direction.x * (end.y - start.y) - profile_direction.y * (end.x - start.x);
    let side = if cross < -0.0001 {
        ExitSide::Right
    } else if cross > 0.0001 {
        ExitSide::Left
    } else if element_index >= parent_road.elements.len() / 2 {
        ExitSide::Right
    } else {
        ExitSide::Left
    };
    let (remaining, carried) = split_elements(&parent_road, element_index, side);
    let entry_road = road_with_elements(
        program,
        &format!("{}_exit_{}", parent_road.name, side.label()),
        carried.clone(),
        RoadDirection::Forward,
    );
    let through_road = if mode == SplitMode::Dedicated && !remaining.is_empty() {
        let mut elements = remaining;
        match side {
            ExitSide::Right => elements.push(RoadElement {
                kind: RoadElementKind::Barrier,
                catch: None,
            }),
            ExitSide::Left => elements.insert(
                0,
                RoadElement {
                    kind: RoadElementKind::Barrier,
                    catch: None,
                },
            ),
        }
        Some(road_with_elements(
            program,
            &format!("{}_after_exit_{}", parent_road.name, side.label()),
            elements,
            parent_road.direction,
        ))
    } else if mode == SplitMode::KeepLane {
        Some(parent_road.clone())
    } else {
        None
    };
    let distance = ((end.x - start.x).powi(2) + (end.y - start.y).powi(2)).sqrt();
    let control1 = WorldPoint {
        x: start.x + direction.x * distance / 3.0,
        y: start.y + direction.y * distance / 3.0,
    };
    let end_direction = junction_direction(program, end_ref, end, false);
    let control2 = if let Some(direction) = end_direction {
        WorldPoint {
            x: end.x - direction.x * distance / 3.0,
            y: end.y - direction.y * distance / 3.0,
        }
    } else {
        point_between(start, end, 2.0 / 3.0)
    };
    let split = 0.38;
    let a = point_between(start, control1, split);
    let b = point_between(control1, control2, split);
    let c = point_between(control2, end, split);
    let d = point_between(a, b, split);
    let e = point_between(b, c, split);
    let middle = point_between(d, e, split);
    let (entry_index, middle_ref) =
        append_connected(program, &entry_road, start, Some(start_ref), middle, None);
    set_control_direct(program, SegmentSource::TopLevel(entry_index), false, a);
    set_control_direct(program, SegmentSource::TopLevel(entry_index), true, d);
    let (full_index, final_ref) = append_connected(
        program,
        &entry_road,
        middle,
        Some(&middle_ref),
        end,
        end_ref,
    );
    set_control_direct(program, SegmentSource::TopLevel(full_index), false, e);
    set_control_direct(program, SegmentSource::TopLevel(full_index), true, c);
    let through = if split_t >= 0.999 {
        through_road.as_ref().map(|road| {
            let tangent = parent.tangent_at(1.0);
            let length = (tangent.x * tangent.x + tangent.y * tangent.y)
                .sqrt()
                .max(0.0001);
            let normal = WorldPoint {
                x: tangent.y / length,
                y: -tangent.x / length,
            };
            let carried_width: f32 = carried
                .iter()
                .map(|element| element_world_width(element.kind))
                .sum();
            let barrier_width = element_world_width(RoadElementKind::Barrier);
            let shift = match mode {
                SplitMode::KeepLane => 0.0,
                SplitMode::Dedicated => match side {
                    ExitSide::Right => -(carried_width - barrier_width) / 2.0,
                    ExitSide::Left => (carried_width - barrier_width) / 2.0,
                },
            };
            let through_start = WorldPoint {
                x: parent.end.x + normal.x * shift,
                y: parent.end.y + normal.y * shift,
            };
            let through_end = WorldPoint {
                x: through_start.x + tangent.x / length * distance * 0.65,
                y: through_start.y + tangent.y / length * distance * 0.65,
            };
            SegmentSource::TopLevel(append(program, road, through_start, through_end))
        })
    } else if mode == SplitMode::Dedicated && split_t > 0.001 {
        if through_road.is_none() {
            end_parent_at_split(program, parent, start_ref, split_t);
        }
        through_road.as_ref().and_then(|road| {
            let carried_width: f32 = carried
                .iter()
                .map(|element| element_world_width(element.kind))
                .sum();
            let barrier_width = element_world_width(RoadElementKind::Barrier);
            let shift = match side {
                ExitSide::Right => -(carried_width - barrier_width) / 2.0,
                ExitSide::Left => (carried_width - barrier_width) / 2.0,
            };
            append_remaining_after_split(program, parent, road, start_ref, split_t, shift)
        })
    } else {
        None
    };
    let middle_height = branch_start_height + (branch_end_height - branch_start_height) * split;
    placement_grade(
        program,
        SegmentSource::TopLevel(entry_index),
        branch_start_height,
        middle_height,
    );
    placement_grade(
        program,
        SegmentSource::TopLevel(full_index),
        middle_height,
        branch_end_height,
    );
    if split_t >= 0.999 {
        if let Some(through) = through {
            placement_grade(program, through, parent.end_z, parent.end_z);
        }
    }
    Some(ExitPlacement {
        through,
        entry: SegmentSource::TopLevel(entry_index),
        full: SegmentSource::TopLevel(full_index),
        end_ref: final_ref,
        side,
    })
}

fn map_attachments_before_split(program: &mut Program, parent: &Segment, t: f32) {
    let Some(parent_name) = parent.name.as_deref() else {
        return;
    };
    for statement in &mut program.statements {
        let Statement::Call(call) = statement else {
            continue;
        };
        if call.name != "lane_point"
            || !matches!(call.args.first(), Some(Argument::Identifier(name)) if name == parent_name)
        {
            continue;
        }
        let Some(previous) = call.args.get(2).and_then(number) else {
            continue;
        };
        call.args[2] = number_arg((previous / t).clamp(0.0, 1.0));
    }
}
