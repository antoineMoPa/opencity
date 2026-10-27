//! Projection of road source and block instances into map geometry.

use super::*;

pub(super) fn number(argument: &Argument) -> Option<f32> {
    match argument {
        Argument::Number(text) => text.parse::<f32>().ok().filter(|value| value.is_finite()),
        Argument::Identifier(_) | Argument::String(_) | Argument::Object(_) => None,
    }
}

pub(super) fn point_args(call: &Call, index: usize) -> Option<WorldPoint> {
    Some(WorldPoint {
        x: number(call.args.get(index)?)?,
        y: number(call.args.get(index + 1)?)?,
    })
}

fn component_ports(program: &Program) -> Vec<(MapPoint, f32)> {
    opencity::network::project(program)
        .instances
        .iter()
        .flat_map(|instance| {
            instance
                .geometry
                .ports
                .iter()
                .map(|port| {
                    let point = instance.world_point(port.position);
                    let heading = instance.world_direction(port.heading);
                    (
                        MapPoint {
                            name: format!(
                                "{}.{}.{}",
                                instance.name,
                                port.name,
                                port.role.keyword()
                            ),
                            position: WorldPoint {
                                x: point.x,
                                y: point.y,
                            },
                            base: None,
                            base_direction: None,
                            direction: Some(WorldPoint {
                                x: heading[0],
                                y: heading[1],
                            }),
                        },
                        point.z,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(super) fn line_controls(start: WorldPoint, end: WorldPoint) -> [WorldPoint; 2] {
    [
        WorldPoint {
            x: start.x + (end.x - start.x) / 3.0,
            y: start.y + (end.y - start.y) / 3.0,
        },
        WorldPoint {
            x: start.x + 2.0 * (end.x - start.x) / 3.0,
            y: start.y + 2.0 * (end.y - start.y) / 3.0,
        },
    ]
}

fn point_reference<'a>(
    argument: &Argument,
    points: &HashMap<&str, &'a MapPoint>,
) -> Option<(&'a MapPoint, String)> {
    let Argument::Identifier(name) = argument else {
        return None;
    };
    points
        .get(name.as_str())
        .map(|point| (*point, name.clone()))
}

fn segment_points(
    call: &Call,
    points: &HashMap<&str, &MapPoint>,
) -> Option<(
    WorldPoint,
    WorldPoint,
    [WorldPoint; 2],
    Option<String>,
    Option<String>,
)> {
    match call.args.len() {
        4 => {
            let start = point_args(call, 0)?;
            let end = point_args(call, 2)?;
            Some((start, end, line_controls(start, end), None, None))
        }
        8 => Some((
            point_args(call, 0)?,
            point_args(call, 6)?,
            [point_args(call, 2)?, point_args(call, 4)?],
            None,
            None,
        )),
        6 => {
            let (start_point, start_ref) = point_reference(&call.args[0], points)?;
            let (end_point, end_ref) = point_reference(&call.args[5], points)?;
            Some((
                start_point.position,
                end_point.position,
                [
                    start_point.control_from_stored(point_args(call, 1)?),
                    end_point.control_from_stored(point_args(call, 3)?),
                ],
                Some(start_ref),
                Some(end_ref),
            ))
        }
        _ => None,
    }
}

pub fn project(program: &Program) -> Projection {
    let (expanded, errors) = opencity::junction_layouts::expand(program);
    let mut result = project_layout(&expanded);
    result.unresolved += errors.len();
    result
}

fn project_layout(program: &Program) -> Projection {
    let mut points: Vec<MapPoint> = program
        .statements
        .iter()
        .filter_map(|statement| {
            let Statement::Call(call) = statement else {
                return None;
            };
            if call.name != "map_point" || call.args.len() != 2 {
                return None;
            }
            Some(MapPoint {
                name: call.assigns.first()?.clone(),
                position: point_args(call, 0)?,
                base: None,
                base_direction: None,
                direction: None,
            })
        })
        .collect();
    points.extend(component_ports(program).into_iter().map(|(point, _)| point));
    let lane_calls: Vec<_> = program
        .statements
        .iter()
        .filter_map(|statement| {
            let Statement::Call(call) = statement else {
                return None;
            };
            if call.name != "lane_point" || !matches!(call.args.len(), 3 | 5 | 7) {
                return None;
            }
            let Argument::Identifier(parent) = &call.args[0] else {
                return None;
            };
            let index = number(&call.args[1])?;
            let t = number(&call.args[2])?;
            if index < 0.0 || index.fract() != 0.0 || !(0.0..=1.0).contains(&t) {
                return None;
            }
            Some((
                call.assigns.first()?.clone(),
                parent.clone(),
                index as usize,
                t,
                if call.args.len() >= 5 {
                    point_args(call, 3)
                } else {
                    None
                },
                if call.args.len() == 7 {
                    point_args(call, 5)
                } else {
                    None
                },
            ))
        })
        .collect();
    for _ in 0..lane_calls.len() {
        let provisional = project_with_points(program, &points);
        let mut added = false;
        for (name, parent_name, element_index, t, base, base_direction) in &lane_calls {
            if points.iter().any(|point| point.name == *name) {
                continue;
            }
            let Some(parent) = provisional
                .segments
                .iter()
                .find(|segment| segment.name.as_deref() == Some(parent_name))
            else {
                continue;
            };
            let Some(road) = program
                .statements
                .iter()
                .find_map(|statement| match statement {
                    Statement::Road(road) if road.name == parent.road_name => Some(road),
                    _ => None,
                })
            else {
                continue;
            };
            if let Some(position) = element_position(parent, road, *element_index, *t) {
                points.push(MapPoint {
                    name: name.clone(),
                    position,
                    base: *base,
                    base_direction: *base_direction,
                    direction: element_direction(parent, road, *element_index, *t),
                });
                added = true;
            }
        }
        if !added {
            break;
        }
    }
    let mut projection = project_with_points(program, &points);
    let defined = program
        .statements
        .iter()
        .filter(|statement| {
            matches!(statement,
        Statement::Call(call) if IntersectionKind::from_call_name(&call.name).is_some())
        })
        .count();
    projection.intersections = super::intersections::from_layout(program, &projection);
    projection.unresolved += defined - projection.intersections.len();
    projection
}

fn project_with_points(program: &Program, points: &[MapPoint]) -> Projection {
    let mut port_heights: HashMap<_, _> = component_ports(program)
        .into_iter()
        .map(|(point, z)| (point.name, z))
        .collect();
    let mut point_by_name = HashMap::with_capacity(points.len());
    for point in points {
        point_by_name.entry(point.name.as_str()).or_insert(point);
    }
    let mut elevations = HashMap::new();
    let mut grades = HashMap::new();
    for statement in &program.statements {
        let Statement::Call(call) = statement else {
            continue;
        };
        match (call.name.as_str(), call.args.as_slice()) {
            ("elevation", [Argument::Identifier(name), Argument::Number(value)]) => {
                if let Some(height) = value
                    .parse::<f32>()
                    .ok()
                    .filter(|height| height.is_finite())
                {
                    elevations.insert(name.as_str(), height);
                }
            }
            (
                "grade",
                [Argument::Identifier(name), Argument::Number(start), Argument::Number(end)],
            ) => {
                if let (Some(start), Some(end)) = (
                    start.parse::<f32>().ok().filter(|value| value.is_finite()),
                    end.parse::<f32>().ok().filter(|value| value.is_finite()),
                ) {
                    grades.insert(name.as_str(), (start, end));
                }
            }
            _ => {}
        }
    }
    for point in points {
        if let Some(height) = elevations.get(point.name.as_str()) {
            port_heights.entry(point.name.clone()).or_insert(*height);
        }
    }
    let roads: HashMap<&str, RoadDirection> = program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Road(road) => Some((road.name.as_str(), road.direction)),
            _ => None,
        })
        .collect();
    let mut segments = Vec::new();
    let mut unresolved = 0;
    let blocks: Vec<_> = program
        .statements
        .iter()
        .enumerate()
        .filter_map(|(index, statement)| match statement {
            Statement::Block(block) => Some((index, block)),
            _ => None,
        })
        .collect();
    for (index, statement) in program.statements.iter().enumerate() {
        let Statement::Call(call) = statement else {
            continue;
        };
        if let Some(&direction) = roads.get(call.name.as_str()) {
            if let Some((start, end, controls, start_ref, end_ref)) =
                segment_points(call, &point_by_name)
            {
                let port_z = (
                    start_ref
                        .as_ref()
                        .and_then(|name| port_heights.get(name))
                        .copied()
                        .unwrap_or(0.),
                    end_ref
                        .as_ref()
                        .and_then(|name| port_heights.get(name))
                        .copied()
                        .unwrap_or(0.),
                );
                let (start_z, end_z) = call.assigns.first().map_or(port_z, |name| {
                    grades.get(name.as_str()).copied().unwrap_or_else(|| {
                        elevations
                            .get(name.as_str())
                            .map(|z| (*z, *z))
                            .unwrap_or(port_z)
                    })
                });
                segments.push(Segment {
                    source: SegmentSource::TopLevel(index),
                    name: call.assigns.first().cloned(),
                    road_name: call.name.clone(),
                    direction,
                    start,
                    end,
                    controls,
                    start_ref,
                    end_ref,
                    elevation: (start_z + end_z) / 2.0,
                    start_z,
                    end_z,
                });
            } else {
                unresolved += 1;
            }
            continue;
        }
        if !blocks.iter().any(|(_, block)| block.name == call.name) {
            continue;
        }
        let top_calls: Vec<_> = program
            .statements
            .iter()
            .filter_map(|statement| match statement {
                Statement::Call(call) => Some(call),
                _ => None,
            })
            .collect();
        let mut stack = Vec::new();
        let mut next_nested_call = 0;
        project_block_road_calls(
            program,
            "",
            call,
            index,
            &blocks,
            &roads,
            &top_calls,
            WorldPoint { x: 0.0, y: 0.0 },
            0.0,
            0.0,
            &mut stack,
            &mut next_nested_call,
            &mut segments,
            &mut unresolved,
        );
    }
    Projection {
        segments,
        points: points.to_vec(),
        unresolved,
        intersections: Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn project_block_road_calls(
    program: &Program,
    namespace: &str,
    placement: &Call,
    instance: usize,
    blocks: &[(usize, &opencity::BlockDefinition)],
    roads: &HashMap<&str, RoadDirection>,
    parent_calls: &[&Call],
    parent_origin: WorldPoint,
    parent_elevation: f32,
    parent_rotation: f32,
    stack: &mut Vec<usize>,
    next_nested_call: &mut usize,
    segments: &mut Vec<Segment>,
    unresolved: &mut usize,
) {
    let Some(&(block_index, block)) = blocks
        .iter()
        .find(|(_, block)| block.name == placement.name)
    else {
        return;
    };
    let local_name = placement.assigns.first().unwrap_or(&placement.name);
    let qualified = if namespace.is_empty() {
        local_name.clone()
    } else {
        format!("{namespace}.{local_name}")
    };
    if stack.contains(&block_index) {
        *unresolved += 1;
        return;
    }
    let Ok(bound) = block.instantiate(&placement.args) else {
        *unresolved += 1;
        return;
    };
    let rotated_origin = opencity::blocks::rotate_vector(bound.origin, parent_rotation);
    let rotation = parent_rotation + bound.rotation;
    let transform = |point: WorldPoint| {
        let [x, y] = opencity::blocks::rotate_vector([point.x, point.y], rotation);
        WorldPoint { x, y }
    };
    let local_origin = WorldPoint {
        x: rotated_origin[0],
        y: rotated_origin[1],
    };
    let origin = parent_origin.translated(local_origin);
    let elevation = parent_elevation
        + bound.elevation
        + placement
            .assigns
            .first()
            .and_then(|name| opencity::elevation_in_calls(parent_calls, name))
            .unwrap_or(0.0);
    if !origin.x.is_finite()
        || !origin.y.is_finite()
        || !elevation.is_finite()
        || !rotation.is_finite()
    {
        *unresolved += 1;
        return;
    }
    stack.push(block_index);
    let local_calls: Vec<_> = bound.calls.iter().collect();
    let local_program = Program {
        statements: program
            .statements
            .iter()
            .filter(|s| {
                !matches!(s, Statement::Call(_))
                    && !matches!(s,Statement::Network(d) if d.origin.is_some())
            })
            .cloned()
            .chain(bound.calls.iter().cloned().map(Statement::Call))
            .collect(),
    };
    let mut local_ports = component_ports(&local_program);
    let (local_layout, _) = opencity::junction_layouts::expand(&local_program);
    let layout_calls: Vec<_> = local_layout
        .statements
        .iter()
        .filter_map(|s| match s {
            Statement::Call(c) => Some(c),
            _ => None,
        })
        .collect();
    for call in &layout_calls {
        if call.name != "map_point" {
            continue;
        }
        if let (Some(name), Some(position)) = (call.assigns.first(), point_args(call, 0)) {
            local_ports.push((
                MapPoint {
                    name: name.clone(),
                    position,
                    base: None,
                    base_direction: None,
                    direction: None,
                },
                opencity::elevation_in_calls(&layout_calls, name).unwrap_or(0.),
            ));
        }
    }
    let local_points: HashMap<_, _> = local_ports
        .iter()
        .map(|(point, _)| (point.name.as_str(), point))
        .collect();
    let local_heights: HashMap<_, _> = local_ports
        .iter()
        .map(|(point, z)| (point.name.as_str(), *z))
        .collect();
    for (call_index, inner) in bound.calls.iter().enumerate() {
        if let Some(&direction) = roads.get(inner.name.as_str()) {
            // Numeric coordinates and component ports are scoped to this block.
            if let Some((start, end, controls, start_ref, end_ref)) =
                segment_points(inner, &local_points)
            {
                let port_z = (
                    start_ref
                        .as_deref()
                        .and_then(|name| local_heights.get(name))
                        .copied()
                        .unwrap_or(0.),
                    end_ref
                        .as_deref()
                        .and_then(|name| local_heights.get(name))
                        .copied()
                        .unwrap_or(0.),
                );
                let (local_start_z, local_end_z) = inner.assigns.first().map_or(port_z, |name| {
                    opencity::grade_in_calls(&local_calls, name).unwrap_or_else(|| {
                        opencity::elevation_in_calls(&local_calls, name)
                            .map(|z| (z, z))
                            .unwrap_or(port_z)
                    })
                });
                let start_z = elevation + local_start_z;
                let end_z = elevation + local_end_z;
                // Direct calls retain their lexical index. Nested calls use a unique
                // projection index so repeated placements have distinct sources.
                let source_call = if stack.len() == 1 {
                    call_index
                } else {
                    let projected_call = *next_nested_call;
                    *next_nested_call += 1;
                    projected_call
                };
                segments.push(Segment {
                    source: SegmentSource::BlockInstance {
                        instance,
                        block: block_index,
                        call: source_call,
                    },
                    name: inner.assigns.first().cloned(),
                    road_name: inner.name.clone(),
                    direction,
                    start: transform(start).translated(origin),
                    end: transform(end).translated(origin),
                    controls: [
                        transform(controls[0]).translated(origin),
                        transform(controls[1]).translated(origin),
                    ],
                    start_ref: start_ref.map(|name| format!("{qualified}.{name}")),
                    end_ref: end_ref.map(|name| format!("{qualified}.{name}")),
                    elevation: (start_z + end_z) / 2.0,
                    start_z,
                    end_z,
                });
            } else {
                *unresolved += 1;
            }
        } else if blocks.iter().any(|(_, block)| block.name == inner.name) {
            project_block_road_calls(
                program,
                &qualified,
                inner,
                instance,
                blocks,
                roads,
                &local_calls,
                origin,
                elevation,
                rotation,
                stack,
                next_nested_call,
                segments,
                unresolved,
            );
        }
    }
    stack.pop();
}
