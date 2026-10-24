//! Road naming, curve controls, shared endpoints, and removal.

use super::*;

pub fn set_name(program: &mut Program, source: SegmentSource, name: String) -> bool {
    let SegmentSource::TopLevel(index) = source else {
        return false;
    };
    if !name_available(program, &name, Some(index)) {
        return false;
    }
    if !project(program)
        .segments
        .iter()
        .any(|segment| segment.source == source)
    {
        return false;
    }
    let Some(Statement::Call(call)) = program.statements.get_mut(index) else {
        return false;
    };
    if call
        .assigns
        .first()
        .is_some_and(|existing| existing == &name)
    {
        return false;
    }
    let old_name = call.assigns.first().cloned();
    if call.assigns.is_empty() {
        call.assigns.push(name.clone());
    } else {
        call.assigns[0] = name.clone();
    }
    if let Some(old_name) = old_name {
        for statement in &mut program.statements {
            let Statement::Call(call) = statement else {
                continue;
            };
            if call.name == "lane_point" {
                if let Some(Argument::Identifier(parent)) = call.args.first_mut() {
                    if *parent == old_name {
                        *parent = name.clone();
                    }
                }
            } else if call.name == "lane_split" {
                for (index, argument) in call.args.iter_mut().enumerate() {
                    if index == 2 {
                        continue;
                    }
                    if let Argument::Identifier(road) = argument {
                        if *road == old_name {
                            *road = name.clone();
                        }
                    }
                }
            } else if matches!(call.name.as_str(), "elevation" | "grade") {
                if let Some(Argument::Identifier(road)) = call.args.first_mut() {
                    if *road == old_name {
                        *road = name.clone();
                    }
                }
            }
        }
    }
    true
}

pub fn set_road_type(program: &mut Program, source: SegmentSource, road_name: &str) -> bool {
    let SegmentSource::TopLevel(index) = source else {
        return false;
    };
    let Some(road) = program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Road(road) if road.name == road_name => Some(road),
            _ => None,
        })
    else {
        return false;
    };
    let element_count = road.elements.len();
    let Some(segment_name) = project(program)
        .segments
        .into_iter()
        .find(|segment| segment.source == source)
        .and_then(|segment| segment.name)
    else {
        return false;
    };
    if program.statements.iter().any(|statement| match statement {
        Statement::Call(call) if call.name == "lane_point" =>
            matches!(call.args.first(), Some(Argument::Identifier(parent)) if parent == &segment_name)
                && call.args.get(1).and_then(number).is_some_and(|element| element as usize >= element_count),
        _ => false,
    }) {
        return false;
    }
    let Some(Statement::Call(call)) = program.statements.get_mut(index) else {
        return false;
    };
    if call.name == road_name {
        return false;
    }
    call.name = road_name.to_owned();
    true
}

pub fn append_curve(
    program: &mut Program,
    road: &RoadDefinition,
    start: WorldPoint,
    first: WorldPoint,
    second: WorldPoint,
    end: WorldPoint,
) -> usize {
    let index = append(program, road, start, end);
    let Some(Statement::Call(call)) = program.statements.get_mut(index) else {
        unreachable!()
    };
    call.args = [start, first, second, end]
        .into_iter()
        .flat_map(|point| [number_arg(point.x), number_arg(point.y)])
        .collect();
    index
}

pub fn set_control(
    program: &mut Program,
    source: SegmentSource,
    second: bool,
    point: WorldPoint,
) -> bool {
    let projection = project(program);
    let Some(segment) = projection
        .segments
        .iter()
        .find(|segment| segment.source == source && segment.source.editable())
    else {
        return false;
    };
    let junction = if second { segment.end } else { segment.start };
    if !set_control_direct(program, source, second, point) {
        return false;
    }
    let reflected = WorldPoint {
        x: 2.0 * junction.x - point.x,
        y: 2.0 * junction.y - point.y,
    };
    for neighbor in projection
        .segments
        .iter()
        .filter(|neighbor| neighbor.source != source && neighbor.source.editable())
    {
        if second && neighbor.start == junction {
            set_control_direct(program, neighbor.source, false, reflected);
        } else if !second && neighbor.end == junction {
            set_control_direct(program, neighbor.source, true, reflected);
        }
    }
    true
}

pub(super) fn set_control_direct(
    program: &mut Program,
    source: SegmentSource,
    second: bool,
    point: WorldPoint,
) -> bool {
    let SegmentSource::TopLevel(index) = source else {
        return false;
    };
    let projection = project(program);
    let Some(segment) = projection
        .segments
        .iter()
        .find(|segment| segment.source == source)
    else {
        return false;
    };
    let Some(Statement::Call(call)) = program.statements.get_mut(index) else {
        return false;
    };
    if call.args.len() == 4 {
        call.args = [
            segment.start,
            segment.controls[0],
            segment.controls[1],
            segment.end,
        ]
        .into_iter()
        .flat_map(|point| [number_arg(point.x), number_arg(point.y)])
        .collect();
    }
    let first = match (call.args.len(), second) {
        (6, false) => 1,
        (6, true) => 3,
        (8, false) => 2,
        (8, true) => 4,
        _ => return false,
    };
    let reference = if second {
        segment.end_ref.as_deref()
    } else {
        segment.start_ref.as_deref()
    };
    let stored = reference
        .and_then(|name| {
            projection
                .points
                .iter()
                .find(|candidate| candidate.name == name)
        })
        .map(|reference| reference.stored_from_control(point))
        .unwrap_or(point);
    call.args[first] = number_arg(stored.x);
    call.args[first + 1] = number_arg(stored.y);
    true
}

pub fn set_connected_endpoint(
    program: &mut Program,
    source: SegmentSource,
    is_end: bool,
    point: WorldPoint,
) -> bool {
    let projection = project(program);
    let Some(selected) = projection
        .segments
        .iter()
        .find(|segment| segment.source == source && segment.source.editable())
    else {
        return false;
    };
    let original = if is_end { selected.end } else { selected.start };
    let reference = if is_end {
        selected.end_ref.as_deref()
    } else {
        selected.start_ref.as_deref()
    };
    if reference.is_some_and(|name| anchored_reference(program, name)) {
        return false;
    }
    let connected: Vec<_> = projection
        .segments
        .iter()
        .filter(|segment| segment.source.editable())
        .flat_map(|segment| {
            [
                (segment, false, segment.start, segment.start_ref.as_deref()),
                (segment, true, segment.end, segment.end_ref.as_deref()),
            ]
            .into_iter()
            .filter(move |(_, _, endpoint, endpoint_ref)| match reference {
                Some(_) => *endpoint_ref == reference,
                None => endpoint_ref.is_none() && *endpoint == original,
            })
        })
        .collect();
    let mut references = HashSet::new();
    for (segment, is_end, old, endpoint_ref) in connected {
        if endpoint_ref.is_some_and(|name| anchored_reference(program, name)) {
            continue;
        }
        let SegmentSource::TopLevel(index) = segment.source else {
            continue;
        };
        let Some(Statement::Call(call)) = program.statements.get_mut(index) else {
            continue;
        };
        if call.args.len() == 4 {
            call.args = [
                segment.start,
                segment.controls[0],
                segment.controls[1],
                segment.end,
            ]
            .into_iter()
            .flat_map(|p| [number_arg(p.x), number_arg(p.y)])
            .collect();
        }
        let control_index = match (call.args.len(), is_end) {
            (6, false) => 1,
            (6, true) => 3,
            (8, false) => 2,
            (8, true) => 4,
            _ => continue,
        };
        let control = point_args(call, control_index).unwrap();
        call.args[control_index] = number_arg(control.x + point.x - old.x);
        call.args[control_index + 1] = number_arg(control.y + point.y - old.y);
        if let Some(name) = endpoint_ref {
            references.insert(name.to_owned());
        } else {
            let endpoint_index = if is_end { 6 } else { 0 };
            call.args[endpoint_index] = number_arg(point.x);
            call.args[endpoint_index + 1] = number_arg(point.y);
        }
    }
    for statement in &mut program.statements {
        let Statement::Call(call) = statement else {
            continue;
        };
        if call.name == "map_point"
            && call
                .assigns
                .first()
                .is_some_and(|name| references.contains(name))
            && call.args.len() == 2
        {
            call.args[0] = number_arg(point.x);
            call.args[1] = number_arg(point.y);
        }
    }
    true
}

pub fn remove(program: &mut Program, source: SegmentSource) -> bool {
    let SegmentSource::TopLevel(index) = source else {
        return false;
    };
    if !matches!(program.statements.get(index), Some(Statement::Call(_))) {
        return false;
    }
    let projection = project(program);
    let removed_name = projection
        .segments
        .iter()
        .find(|segment| segment.source == source)
        .and_then(|segment| segment.name.clone());
    if let Some(parent_name) = removed_name.as_deref() {
        let mut detached = HashSet::new();
        for statement in &mut program.statements {
            let Statement::Call(call) = statement else {
                continue;
            };
            if call.name != "lane_point"
                || !matches!(call.args.first(), Some(Argument::Identifier(name)) if name == parent_name)
            {
                continue;
            }
            let Some(position) = projection
                .points
                .iter()
                .find(|point| call.assigns.first() == Some(&point.name))
                .map(|point| point.position)
            else {
                continue;
            };
            call.name = "map_point".into();
            call.args = vec![number_arg(position.x), number_arg(position.y)];
            if let Some(name) = call.assigns.first() {
                detached.insert(name.clone());
            }
        }
        for segment in &projection.segments {
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
    }
    program.statements.remove(index);
    if let Some(name) = removed_name {
        program.statements.retain(|statement| {
            !matches!(statement,
            Statement::Call(call) if (call.name == "lane_split" && call.args.iter().enumerate()
                .any(|(index, arg)| index != 2 && matches!(arg, Argument::Identifier(road) if road == &name)))
                || (matches!(call.name.as_str(), "elevation" | "grade")
                    && matches!(call.args.first(), Some(Argument::Identifier(road)) if road == &name)))
        });
    }
    prune_unused_points(program);
    true
}

pub fn prune_unused_points(program: &mut Program) {
    let mut referenced = HashSet::new();
    for statement in &program.statements {
        let calls: Vec<&Call> = match statement {
            Statement::Call(call) => vec![call],
            Statement::Block(block) => block.calls.iter().collect(),
            Statement::Road(_) => Vec::new(),
        };
        for call in calls {
            if call.args.len() == 6 {
                for argument in [&call.args[0], &call.args[5]] {
                    if let Argument::Identifier(name) = argument {
                        referenced.insert(name.clone());
                    }
                }
            }
            if let Some(kind) = IntersectionKind::from_call_name(&call.name) {
                if call.args.len() == kind.arm_count() * 2 + 2 {
                    for argument in &call.args {
                        if let Argument::Identifier(name) = argument {
                            referenced.insert(name.clone());
                        }
                    }
                } else if let Some(Argument::Identifier(name)) = call.args.first() {
                    referenced.insert(name.clone());
                }
            }
        }
    }
    program.statements.retain(|statement| match statement {
        Statement::Call(call) if matches!(call.name.as_str(), "map_point" | "lane_point") => call
            .assigns
            .first()
            .is_some_and(|name| referenced.contains(name)),
        _ => true,
    });
}

pub fn remove_unused_point(program: &mut Program, name: &str) {
    if intersection_port_direction(program, name).is_some()
        || program.statements.iter().any(|statement| matches!(statement,
            Statement::Call(call) if IntersectionKind::from_call_name(&call.name).is_some()
                && matches!(call.args.first(), Some(Argument::Identifier(center)) if center == name))) {
        return;
    }
    let used = program.statements.iter().any(|statement| match statement {
        Statement::Call(call) if call.args.len() == 6 => {
            matches!((&call.args[0], &call.args[5]),
                (Argument::Identifier(start), _) if start == name)
                || matches!((&call.args[0], &call.args[5]),
                (_, Argument::Identifier(end)) if end == name)
        }
        Statement::Block(block) => block.calls.iter().any(|call| {
            call.args.len() == 6
                && (matches!(call.args.first(), Some(Argument::Identifier(start)) if start == name)
                    || matches!(call.args.last(), Some(Argument::Identifier(end)) if end == name))
        }),
        _ => false,
    });
    if used {
        return;
    }
    program.statements.retain(|statement| {
        !matches!(statement,
        Statement::Call(call) if matches!(call.name.as_str(), "map_point" | "lane_point")
            && call.assigns.first().is_some_and(|assigned| assigned == name))
    });
}
