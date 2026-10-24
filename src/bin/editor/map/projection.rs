//! Projection of road source and block instances into map geometry.

use super::*;

pub(super) fn number(argument: &Argument) -> Option<f32> {
    match argument {
        Argument::Number(text) => text.parse::<f32>().ok().filter(|value| value.is_finite()),
        Argument::Identifier(_) | Argument::String(_) => None,
    }
}

pub(super) fn point_args(call: &Call, index: usize) -> Option<WorldPoint> {
    Some(WorldPoint {
        x: number(call.args.get(index)?)?,
        y: number(call.args.get(index + 1)?)?,
    })
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

fn block_origin(call: &Call) -> Option<WorldPoint> {
    if !matches!(call.args.len(), 2 | 3) {
        return None;
    }
    Some(WorldPoint {
        x: number(&call.args[0])?,
        y: number(&call.args[1])?,
    })
}

pub fn project(program: &Program) -> Projection {
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
    projection.unresolved += defined - intersections(program, &projection).len();
    projection
}

fn project_with_points(program: &Program, points: &[MapPoint]) -> Projection {
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
                let (start_z, end_z) = call.assigns.first().map_or((0.0, 0.0), |name| {
                    grades.get(name.as_str()).copied().unwrap_or_else(|| {
                        let z = elevations.get(name.as_str()).copied().unwrap_or(0.0);
                        (z, z)
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
            call,
            index,
            &blocks,
            &roads,
            &top_calls,
            WorldPoint { x: 0.0, y: 0.0 },
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
    }
}

#[allow(clippy::too_many_arguments)]
fn project_block_road_calls(
    placement: &Call,
    instance: usize,
    blocks: &[(usize, &opencity::BlockDefinition)],
    roads: &HashMap<&str, RoadDirection>,
    parent_calls: &[&Call],
    parent_origin: WorldPoint,
    parent_elevation: f32,
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
    if stack.contains(&block_index) {
        *unresolved += 1;
        return;
    }
    let Some(local_origin) = block_origin(placement) else {
        *unresolved += 1;
        return;
    };
    let local_elevation = if placement.args.len() == 3 {
        let Some(height) = opencity::block_height(&placement.args) else {
            *unresolved += 1;
            return;
        };
        height
    } else {
        0.0
    };
    let origin = parent_origin.translated(local_origin);
    let elevation = parent_elevation
        + local_elevation
        + placement
            .assigns
            .first()
            .and_then(|name| opencity::elevation_in_calls(parent_calls, name))
            .unwrap_or(0.0);
    if !origin.x.is_finite() || !origin.y.is_finite() || !elevation.is_finite() {
        *unresolved += 1;
        return;
    }
    stack.push(block_index);
    let local_calls: Vec<_> = block.calls.iter().collect();
    for (call_index, inner) in block.calls.iter().enumerate() {
        if let Some(&direction) = roads.get(inner.name.as_str()) {
            // Block roads use local numeric coordinates; global map points are not local.
            if let Some((start, end, controls, start_ref, end_ref)) =
                segment_points(inner, &HashMap::new())
            {
                let (local_start_z, local_end_z) =
                    inner.assigns.first().map_or((0.0, 0.0), |name| {
                        opencity::grade_in_calls(&local_calls, name).unwrap_or_else(|| {
                            let z = opencity::elevation_in_calls(&local_calls, name).unwrap_or(0.0);
                            (z, z)
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
                    start: start.translated(origin),
                    end: end.translated(origin),
                    controls: [
                        controls[0].translated(origin),
                        controls[1].translated(origin),
                    ],
                    start_ref,
                    end_ref,
                    elevation: (start_z + end_z) / 2.0,
                    start_z,
                    end_z,
                });
            } else {
                *unresolved += 1;
            }
        } else if blocks.iter().any(|(_, block)| block.name == inner.name) {
            project_block_road_calls(
                inner,
                instance,
                blocks,
                roads,
                &local_calls,
                origin,
                elevation,
                stack,
                next_nested_call,
                segments,
                unresolved,
            );
        }
    }
    stack.pop();
}
