//! Four numeric road arguments describe a straight segment; eight describe a cubic
//! Bézier segment (start, first control, second control, end).

use std::collections::HashSet;

use opencity::{Argument, Call, Program, RoadDefinition, Statement};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldPoint {
    pub x: f32,
    pub y: f32,
}

impl WorldPoint {
    pub fn translated(self, offset: Self) -> Self {
        Self {
            x: self.x + offset.x,
            y: self.y + offset.y,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentSource {
    TopLevel(usize),
    BlockInstance {
        instance: usize,
        block: usize,
        call: usize,
    },
}

impl SegmentSource {
    pub fn editable(self) -> bool {
        matches!(self, Self::TopLevel(_))
    }
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub source: SegmentSource,
    pub name: Option<String>,
    pub road_name: String,
    pub start: WorldPoint,
    pub end: WorldPoint,
    pub controls: Option<[WorldPoint; 2]>,
}

impl Segment {
    pub fn point_at(&self, t: f32) -> WorldPoint {
        let Some([first, second]) = self.controls else {
            return WorldPoint {
                x: self.start.x + (self.end.x - self.start.x) * t,
                y: self.start.y + (self.end.y - self.start.y) * t,
            };
        };
        let u = 1.0 - t;
        WorldPoint {
            x: u.powi(3) * self.start.x
                + 3.0 * u.powi(2) * t * first.x
                + 3.0 * u * t.powi(2) * second.x
                + t.powi(3) * self.end.x,
            y: u.powi(3) * self.start.y
                + 3.0 * u.powi(2) * t * first.y
                + 3.0 * u * t.powi(2) * second.y
                + t.powi(3) * self.end.y,
        }
    }
}

pub struct Projection {
    pub segments: Vec<Segment>,
    pub unresolved: usize,
}

fn number(argument: &Argument) -> Option<f32> {
    match argument {
        Argument::Number(text) => text.parse::<f32>().ok().filter(|value| value.is_finite()),
        Argument::Identifier(_) => None,
    }
}

fn point_args(call: &Call, index: usize) -> Option<WorldPoint> {
    Some(WorldPoint {
        x: number(call.args.get(index)?)?,
        y: number(call.args.get(index + 1)?)?,
    })
}

fn segment_points(call: &Call) -> Option<(WorldPoint, WorldPoint, Option<[WorldPoint; 2]>)> {
    if call.args.len() != 4 && call.args.len() != 8 {
        return None;
    }
    let start = point_args(call, 0)?;
    if call.args.len() == 4 {
        Some((start, point_args(call, 2)?, None))
    } else {
        Some((start, point_args(call, 6)?, Some([point_args(call, 2)?, point_args(call, 4)?])))
    }
}

fn block_origin(call: &Call) -> Option<WorldPoint> {
    if call.args.len() != 2 {
        return None;
    }
    Some(WorldPoint {
        x: number(&call.args[0])?,
        y: number(&call.args[1])?,
    })
}

pub fn project(program: &Program) -> Projection {
    let roads: HashSet<&str> = program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Road(road) => Some(road.name.as_str()),
            _ => None,
        })
        .collect();
    let mut segments = Vec::new();
    let mut unresolved = 0;
    for (index, statement) in program.statements.iter().enumerate() {
        let Statement::Call(call) = statement else {
            continue;
        };
        if roads.contains(call.name.as_str()) {
            if let Some((start, end, controls)) = segment_points(call) {
                segments.push(Segment {
                    source: SegmentSource::TopLevel(index),
                    name: call.assigns.first().cloned(),
                    road_name: call.name.clone(),
                    start,
                    end,
                    controls,
                });
            } else {
                unresolved += 1;
            }
            continue;
        }
        let Some((block_index, block)) =
            program
                .statements
                .iter()
                .enumerate()
                .find_map(|(block_index, statement)| match statement {
                    Statement::Block(block) if block.name == call.name => {
                        Some((block_index, block))
                    }
                    _ => None,
                })
        else {
            continue;
        };
        let Some(origin) = block_origin(call) else {
            unresolved += block
                .calls
                .iter()
                .filter(|call| roads.contains(call.name.as_str()))
                .count();
            continue;
        };
        for (call_index, inner) in block.calls.iter().enumerate() {
            if !roads.contains(inner.name.as_str()) {
                continue;
            }
            if let Some((start, end, controls)) = segment_points(inner) {
                segments.push(Segment {
                    source: SegmentSource::BlockInstance {
                        instance: index,
                        block: block_index,
                        call: call_index,
                    },
                    name: inner.assigns.first().cloned(),
                    road_name: inner.name.clone(),
                    start: start.translated(origin),
                    end: end.translated(origin),
                    controls: controls.map(|[a, b]| [a.translated(origin), b.translated(origin)]),
                });
            } else {
                unresolved += 1;
            }
        }
    }
    Projection {
        segments,
        unresolved,
    }
}

fn number_arg(value: f32) -> Argument {
    let value = if value.abs() < 0.005 { 0.0 } else { value };
    Argument::Number(format!("{value:.2}"))
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
        if !program.statements.iter().any(|statement| match statement {
            Statement::Call(call) => call.assigns.iter().any(|name| name == &candidate),
            Statement::Block(block) => block.calls.iter().any(|call| call.assigns.iter().any(|name| name == &candidate)),
            _ => false,
        }) { break candidate; }
        suffix += 1;
    };
    program.statements.push(Statement::Call(Call {
        assigns: vec![name],
        name: road.name.clone(),
        args: vec![
            number_arg(start.x),
            number_arg(start.y),
            number_arg(end.x),
            number_arg(end.y),
        ],
    }));
    index
}

pub fn set_name(program: &mut Program, source: SegmentSource, name: String) -> bool {
    let SegmentSource::TopLevel(index) = source else { return false };
    if program.statements.iter().enumerate().any(|(other, statement)| {
        other != index && matches!(statement, Statement::Call(call) if call.assigns.iter().any(|assigned| assigned == &name))
    }) { return false; }
    let Some(Statement::Call(call)) = program.statements.get_mut(index) else { return false };
    if segment_points(call).is_none() { return false; }
    if call.assigns.first().is_some_and(|existing| existing == &name) { return false; }
    if call.assigns.is_empty() { call.assigns.push(name); } else { call.assigns[0] = name; }
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
    let Some(Statement::Call(call)) = program.statements.get_mut(index) else { unreachable!() };
    call.args = [start, first, second, end]
        .into_iter()
        .flat_map(|point| [number_arg(point.x), number_arg(point.y)])
        .collect();
    index
}

pub fn set_control(program: &mut Program, source: SegmentSource, second: bool, point: WorldPoint) -> bool {
    let SegmentSource::TopLevel(index) = source else { return false };
    let Some(Statement::Call(call)) = program.statements.get_mut(index) else { return false };
    if segment_points(call).and_then(|(_, _, controls)| controls).is_none() { return false; }
    let first = if second { 4 } else { 2 };
    call.args[first] = number_arg(point.x);
    call.args[first + 1] = number_arg(point.y);
    true
}

pub fn set_endpoint(
    program: &mut Program,
    source: SegmentSource,
    is_end: bool,
    point: WorldPoint,
) -> bool {
    let SegmentSource::TopLevel(index) = source else {
        return false;
    };
    let Some(Statement::Call(call)) = program.statements.get_mut(index) else {
        return false;
    };
    let Some((old_start, old_end, controls)) = segment_points(call) else {
        return false;
    };
    let first = if is_end { if controls.is_some() { 6 } else { 2 } } else { 0 };
    if controls.is_some() {
        let old = if is_end { old_end } else { old_start };
        let control_index = if is_end { 4 } else { 2 };
        let control = point_args(call, control_index).unwrap();
        call.args[control_index] = number_arg(control.x + point.x - old.x);
        call.args[control_index + 1] = number_arg(control.y + point.y - old.y);
    }
    call.args[first] = number_arg(point.x);
    call.args[first + 1] = number_arg(point.y);
    true
}

pub fn set_connected_endpoint(program: &mut Program, source: SegmentSource, is_end: bool, point: WorldPoint) -> bool {
    let projection = project(program);
    let Some(selected) = projection.segments.iter().find(|segment| segment.source == source && segment.source.editable()) else { return false };
    let original = if is_end { selected.end } else { selected.start };
    let connected: Vec<_> = projection.segments.iter().filter(|segment| segment.source.editable()).flat_map(|segment| {
        [(segment.source, false, segment.start), (segment.source, true, segment.end)]
            .into_iter().filter(move |(_, _, endpoint)| *endpoint == original)
    }).collect();
    for (source, is_end, _) in connected {
        set_endpoint(program, source, is_end, point);
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
    program.statements.remove(index);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencity::parse;

    #[test]
    fn sample_block_instance_is_visible_without_changing_it() {
        let program = parse(include_str!("../../../examples/city.road")).unwrap();
        let projection = project(&program);
        assert_eq!(projection.segments.len(), 2);
        assert_eq!(projection.segments[0].start, WorldPoint { x: 0.0, y: 1.0 });
        assert!(!projection.segments[0].source.editable());
    }

    #[test]
    fn placed_segment_round_trips_and_moves_with_negative_coordinates() {
        let mut program = parse("road lane: shoulder-carlane-verge; end;").unwrap();
        let road = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let index = append(
            &mut program,
            &road,
            WorldPoint { x: -2.0, y: 1.0 },
            WorldPoint { x: 3.0, y: 1.0 },
        );
        assert!(set_endpoint(
            &mut program,
            SegmentSource::TopLevel(index),
            true,
            WorldPoint { x: 4.0, y: -1.0 }
        ));
        let reparsed = parse(&program.to_road_source()).unwrap();
        assert_eq!(reparsed, program);
        assert_eq!(
            project(&program).segments[0].end,
            WorldPoint { x: 4.0, y: -1.0 }
        );
        assert!(remove(&mut program, SegmentSource::TopLevel(index)));
        assert!(project(&program).segments.is_empty());
    }
}
