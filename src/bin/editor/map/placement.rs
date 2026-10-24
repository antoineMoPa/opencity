//! Road placement, endpoint connections, and curve splitting.

use super::*;

pub(super) fn number_arg(value: f32) -> Argument {
    let value = if value.abs() < 0.005 { 0.0 } else { value };
    Argument::Number(format!("{value:.4}"))
}

pub(super) fn name_available(program: &Program, name: &str, except: Option<usize>) -> bool {
    !program
        .statements
        .iter()
        .enumerate()
        .any(|(index, statement)| match statement {
            Statement::Call(call) if Some(index) != except => {
                call.assigns.iter().any(|assigned| assigned == name)
            }
            Statement::Block(block) => block
                .calls
                .iter()
                .any(|call| call.assigns.iter().any(|assigned| assigned == name)),
            _ => false,
        })
}

pub fn append(
    program: &mut Program,
    road: &RoadDefinition,
    start: WorldPoint,
    end: WorldPoint,
) -> usize {
    let index = program.statements.len();
    let mut suffix = 1;
    let name = loop {
        let candidate = format!("road_{suffix}");
        if name_available(program, &candidate, None) {
            break candidate;
        }
        suffix += 1;
    };
    let [first, second] = line_controls(start, end);
    program.statements.push(Statement::Call(Call {
        assigns: vec![name],
        name: road.name.clone(),
        args: vec![
            number_arg(start.x),
            number_arg(start.y),
            number_arg(first.x),
            number_arg(first.y),
            number_arg(second.x),
            number_arg(second.y),
            number_arg(end.x),
            number_arg(end.y),
        ],
    }));
    index
}

pub(super) fn ensure_point(
    program: &mut Program,
    position: WorldPoint,
    existing: Option<&str>,
) -> String {
    if let Some(name) = existing {
        if project(program)
            .points
            .iter()
            .any(|point| point.name == name)
        {
            return name.to_owned();
        }
    }
    let mut suffix = 1;
    let name = loop {
        let candidate = format!("junction_{suffix}");
        if name_available(program, &candidate, None) {
            break candidate;
        }
        suffix += 1;
    };
    program.statements.push(Statement::Call(Call {
        assigns: vec![name.clone()],
        name: "map_point".into(),
        args: vec![number_arg(position.x), number_arg(position.y)],
    }));
    name
}

pub(super) fn attachment_direction(program: &Program, reference: &str) -> Option<WorldPoint> {
    let (parent_name, element_index, t) = lane_attachment(program, reference)?;
    let projection = project(program);
    let parent = projection
        .segments
        .iter()
        .find(|segment| segment.name.as_deref() == Some(&parent_name))?;
    let road = program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Road(road) if road.name == parent.road_name => Some(road),
            _ => None,
        })?;
    let direction = element_direction(parent, road, element_index, t)?;
    Some(
        if road.lane_direction(element_index) == Some(RoadDirection::Reverse) {
            WorldPoint {
                x: -direction.x,
                y: -direction.y,
            }
        } else {
            direction
        },
    )
}

pub(super) fn junction_direction(
    program: &Program,
    reference: Option<&str>,
    point: WorldPoint,
    for_start: bool,
) -> Option<WorldPoint> {
    let projection = project(program);
    let preferred = projection.segments.iter().rev().find_map(|segment| {
        let matches_end =
            segment.end_ref.as_deref() == reference && reference.is_some() || segment.end == point;
        let matches_start = segment.start_ref.as_deref() == reference && reference.is_some()
            || segment.start == point;
        if (for_start && matches_end) || (!for_start && matches_start) {
            Some(if for_start {
                WorldPoint {
                    x: segment.end.x - segment.controls[1].x,
                    y: segment.end.y - segment.controls[1].y,
                }
            } else {
                WorldPoint {
                    x: segment.controls[0].x - segment.start.x,
                    y: segment.controls[0].y - segment.start.y,
                }
            })
        } else {
            None
        }
    });
    let vector = preferred.or_else(|| {
        projection.segments.iter().rev().find_map(|segment| {
            if segment.start == point {
                Some(WorldPoint {
                    x: segment.start.x - segment.controls[0].x,
                    y: segment.start.y - segment.controls[0].y,
                })
            } else if segment.end == point {
                Some(WorldPoint {
                    x: segment.controls[1].x - segment.end.x,
                    y: segment.controls[1].y - segment.end.y,
                })
            } else {
                None
            }
        })
    })?;
    let length = (vector.x * vector.x + vector.y * vector.y).sqrt();
    (length > 0.0001).then_some(WorldPoint {
        x: vector.x / length,
        y: vector.y / length,
    })
}

pub fn append_connected(
    program: &mut Program,
    road: &RoadDefinition,
    start: WorldPoint,
    start_ref: Option<&str>,
    end: WorldPoint,
    end_ref: Option<&str>,
) -> (usize, String) {
    let projection = project(program);
    let start_height = reference_height(program, &projection, start_ref);
    let end_height = reference_height(program, &projection, end_ref);
    let start_name = ensure_point(program, start, start_ref);
    let end_name = ensure_point(program, end, end_ref);
    let [mut first, mut second] = line_controls(start, end);
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let handle_length = (dx * dx + dy * dy).sqrt() / 3.0;
    if let Some(direction) = start_ref
        .and_then(|reference| attachment_direction(program, reference))
        .or_else(|| junction_direction(program, start_ref, start, true))
    {
        first = WorldPoint {
            x: start.x + direction.x * handle_length,
            y: start.y + direction.y * handle_length,
        };
    }
    if let Some(direction) = end_ref
        .and_then(|reference| attachment_direction(program, reference))
        .or_else(|| junction_direction(program, end_ref, end, false))
    {
        second = WorldPoint {
            x: end.x - direction.x * handle_length,
            y: end.y - direction.y * handle_length,
        };
    }
    if let Some(direction) =
        start_ref.and_then(|reference| intersection_port_direction(program, reference))
    {
        first = WorldPoint {
            x: start.x + direction.x * handle_length,
            y: start.y + direction.y * handle_length,
        };
    }
    if let Some(direction) =
        end_ref.and_then(|reference| intersection_port_direction(program, reference))
    {
        second = WorldPoint {
            x: end.x + direction.x * handle_length,
            y: end.y + direction.y * handle_length,
        };
    }
    let index = append_curve(program, road, start, first, second, end);
    let Some(Statement::Call(call)) = program.statements.get_mut(index) else {
        unreachable!()
    };
    call.args = vec![
        Argument::Identifier(start_name),
        number_arg(first.x),
        number_arg(first.y),
        number_arg(second.x),
        number_arg(second.y),
        Argument::Identifier(end_name.clone()),
    ];
    if start_height.is_some() || end_height.is_some() {
        let start = start_height.or(end_height).unwrap_or(0.0);
        let end = end_height.unwrap_or(start);
        placement_grade(program, SegmentSource::TopLevel(index), start, end);
    }
    (index, end_name)
}

pub(super) fn reference_height(
    program: &Program,
    projection: &Projection,
    reference: Option<&str>,
) -> Option<f32> {
    let reference = reference?;
    if let Some((parent, _, t)) = lane_attachment(program, reference) {
        return projection
            .segments
            .iter()
            .find(|s| s.name.as_deref() == Some(&parent))
            .map(|s| s.height_at(t));
    }
    projection.segments.iter().find_map(|s| {
        if s.start_ref.as_deref() == Some(reference) {
            Some(s.start_z)
        } else if s.end_ref.as_deref() == Some(reference) {
            Some(s.end_z)
        } else {
            None
        }
    })
}

pub(super) fn placement_grade(program: &mut Program, source: SegmentSource, start: f32, end: f32) {
    let SegmentSource::TopLevel(index) = source else {
        return;
    };
    let Some(Statement::Call(call)) = program.statements.get(index) else {
        return;
    };
    if let Some(name) = call.assigns.first().cloned() {
        opencity::set_grade(program, &name, start, end);
    }
}

// A truncated parent retains its original parameter interval [0,t]. Keep
// earlier attachments at their world positions by mapping that interval to [0,1].
pub fn append_connected_curve(
    program: &mut Program,
    road: &RoadDefinition,
    start: WorldPoint,
    start_ref: Option<&str>,
    first: WorldPoint,
    second: WorldPoint,
    end: WorldPoint,
    end_ref: Option<&str>,
) -> (usize, String) {
    let (index, end_name) = append_connected(program, road, start, start_ref, end, end_ref);
    let source = SegmentSource::TopLevel(index);
    set_control_direct(program, source, false, first);
    set_control_direct(program, source, true, second);
    (index, end_name)
}

pub fn link_endpoint(
    program: &mut Program,
    source: SegmentSource,
    is_end: bool,
) -> Option<(WorldPoint, String)> {
    let SegmentSource::TopLevel(index) = source else {
        return None;
    };
    let segment = project(program)
        .segments
        .into_iter()
        .find(|segment| segment.source == source)?;
    let point = if is_end { segment.end } else { segment.start };
    let reference = if is_end {
        segment.end_ref.as_deref()
    } else {
        segment.start_ref.as_deref()
    };
    if let Some(reference) = reference {
        return Some((point, reference.to_owned()));
    }
    let start_ref = ensure_point(program, segment.start, segment.start_ref.as_deref());
    let end_ref = ensure_point(program, segment.end, segment.end_ref.as_deref());
    let Statement::Call(call) = program.statements.get_mut(index)? else {
        return None;
    };
    call.args = vec![
        Argument::Identifier(start_ref.clone()),
        number_arg(segment.controls[0].x),
        number_arg(segment.controls[0].y),
        number_arg(segment.controls[1].x),
        number_arg(segment.controls[1].y),
        Argument::Identifier(end_ref.clone()),
    ];
    Some((point, if is_end { end_ref } else { start_ref }))
}

pub fn attach_endpoint_to_port(
    program: &mut Program,
    source: SegmentSource,
    is_end: bool,
    port_ref: &str,
) -> bool {
    let projection = project(program);
    let Some(segment) = projection
        .segments
        .iter()
        .find(|segment| segment.source == source && source.editable())
    else {
        return false;
    };
    if !port_faces_road(
        program,
        port_ref,
        if is_end { segment.start } else { segment.end },
    ) {
        return false;
    }
    if projection.segments.iter().any(|other| {
        other.source != source
            && (other.start_ref.as_deref() == Some(port_ref)
                || other.end_ref.as_deref() == Some(port_ref))
    }) {
        return false;
    }
    let Some(intersection) = intersections(program, &projection)
        .into_iter()
        .find(|intersection| {
            intersection
                .ports
                .iter()
                .any(|port| port.point_ref == port_ref)
                && intersection.ports.iter().any(|port| {
                    port.point_ref == port_ref
                        && intersection.road_name_for_direction(port.direction) == segment.road_name
                })
        })
    else {
        return false;
    };
    let port = intersection
        .ports
        .iter()
        .find(|port| port.point_ref == port_ref)
        .unwrap();
    let old_control = if is_end {
        segment.controls[1]
    } else {
        segment.controls[0]
    };
    let old_end = if is_end { segment.end } else { segment.start };
    let length = ((old_control.x - old_end.x).powi(2) + (old_control.y - old_end.y).powi(2))
        .sqrt()
        .max(0.02);
    if link_endpoint(program, source, is_end).is_none() {
        return false;
    }
    let SegmentSource::TopLevel(index) = source else {
        return false;
    };
    let Some(Statement::Call(call)) = program.statements.get_mut(index) else {
        return false;
    };
    if call.args.len() != 6 {
        return false;
    }
    let control = WorldPoint {
        x: port.position.x + port.direction.x * length,
        y: port.position.y + port.direction.y * length,
    };
    if is_end {
        call.args[3] = number_arg(control.x);
        call.args[4] = number_arg(control.y);
        call.args[5] = Argument::Identifier(port_ref.into());
    } else {
        call.args[0] = Argument::Identifier(port_ref.into());
        call.args[1] = number_arg(control.x);
        call.args[2] = number_arg(control.y);
    }
    true
}

pub fn rotate_endpoint(
    program: &mut Program,
    source: SegmentSource,
    is_end: bool,
    angle_degrees: f32,
) -> bool {
    let Some(segment) = project(program)
        .segments
        .into_iter()
        .find(|segment| segment.source == source && segment.source.editable())
    else {
        return false;
    };
    let anchor = if is_end { segment.end } else { segment.start };
    let control = if is_end {
        segment.controls[1]
    } else {
        segment.controls[0]
    };
    let vector = WorldPoint {
        x: control.x - anchor.x,
        y: control.y - anchor.y,
    };
    let angle = angle_degrees.to_radians();
    let rotated = WorldPoint {
        x: anchor.x + vector.x * angle.cos() - vector.y * angle.sin(),
        y: anchor.y + vector.x * angle.sin() + vector.y * angle.cos(),
    };
    set_control(program, source, is_end, rotated)
}
