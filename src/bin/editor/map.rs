//! Roads use cubic Bézier control points. A junction call gives connected roads
//! a persistent shared endpoint; older numeric road calls remain readable.

use std::collections::HashSet;

use opencity::{Argument, Call, Program, RoadDefinition, RoadElement, RoadElementKind, Statement};

#[derive(Clone, Copy)]
pub enum ElementEdit {
    Inserted(usize),
    Swapped(usize, usize),
    Removed(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitMode {
    Dedicated,
    KeepLane,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitSide {
    Left,
    Right,
}

impl ExitSide {
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

pub struct ExitPlacement {
    pub entry: SegmentSource,
    pub full: SegmentSource,
    pub through: Option<SegmentSource>,
    pub end_ref: String,
    pub side: ExitSide,
}

impl SplitMode {
    fn identifier(self) -> &'static str {
        match self {
            Self::Dedicated => "dedicated",
            Self::KeepLane => "keep_lane",
        }
    }

    fn from_identifier(value: &str) -> Option<Self> {
        match value {
            "dedicated" => Some(Self::Dedicated),
            "keep_lane" => Some(Self::KeepLane),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LaneSplit {
    pub parent: String,
    pub ramp: String,
    pub full_ramp: Option<String>,
    pub element_index: usize,
    pub t: f32,
    pub mode: SplitMode,
}

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
    pub controls: [WorldPoint; 2],
    pub start_ref: Option<String>,
    pub end_ref: Option<String>,
}

impl Segment {
    pub fn point_at(&self, t: f32) -> WorldPoint {
        let [first, second] = self.controls;
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

    pub fn tangent_at(&self, t: f32) -> WorldPoint {
        let u = 1.0 - t;
        WorldPoint {
            x: 3.0 * u * u * (self.controls[0].x - self.start.x)
                + 6.0 * u * t * (self.controls[1].x - self.controls[0].x)
                + 3.0 * t * t * (self.end.x - self.controls[1].x),
            y: 3.0 * u * u * (self.controls[0].y - self.start.y)
                + 6.0 * u * t * (self.controls[1].y - self.controls[0].y)
                + 3.0 * t * t * (self.end.y - self.controls[1].y),
        }
    }
}

pub struct Projection {
    pub segments: Vec<Segment>,
    pub points: Vec<MapPoint>,
    pub unresolved: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntersectionKind {
    RightTurn,
    ThreeWay,
    FourWay,
}

impl IntersectionKind {
    pub fn call_name(self) -> &'static str {
        match self {
            Self::RightTurn => "intersection2",
            Self::ThreeWay => "intersection3",
            Self::FourWay => "intersection4",
        }
    }

    pub fn from_call_name(name: &str) -> Option<Self> {
        match name {
            "intersection2" => Some(Self::RightTurn),
            "intersection3" => Some(Self::ThreeWay),
            "intersection4" => Some(Self::FourWay),
            _ => None,
        }
    }

    pub fn arm_count(self) -> usize {
        match self {
            Self::RightTurn => 2,
            Self::ThreeWay => 3,
            Self::FourWay => 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Intersection {
    pub point_ref: String,
    pub position: WorldPoint,
    pub kind: IntersectionKind,
    pub road_name: String,
    pub ports: Vec<IntersectionPort>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IntersectionPort {
    pub point_ref: String,
    pub position: WorldPoint,
    pub direction: WorldPoint,
}

fn port_directions(kind: IntersectionKind) -> &'static [WorldPoint] {
    const NORTH: WorldPoint = WorldPoint { x: 0.0, y: 1.0 };
    const EAST: WorldPoint = WorldPoint { x: 1.0, y: 0.0 };
    const SOUTH: WorldPoint = WorldPoint { x: 0.0, y: -1.0 };
    const WEST: WorldPoint = WorldPoint { x: -1.0, y: 0.0 };
    match kind {
        IntersectionKind::RightTurn => &[NORTH, EAST],
        IntersectionKind::ThreeWay => &[NORTH, EAST, WEST],
        IntersectionKind::FourWay => &[NORTH, EAST, SOUTH, WEST],
    }
}

fn port_direction_name(direction: WorldPoint) -> &'static str {
    match (direction.x as i32, direction.y as i32) {
        (0, 1) => "north",
        (1, 0) => "east",
        (0, -1) => "south",
        (-1, 0) => "west",
        _ => unreachable!(),
    }
}

fn port_direction_from_name(name: &str) -> Option<WorldPoint> {
    Some(match name {
        "north" => WorldPoint { x: 0.0, y: 1.0 },
        "east" => WorldPoint { x: 1.0, y: 0.0 },
        "south" => WorldPoint { x: 0.0, y: -1.0 },
        "west" => WorldPoint { x: -1.0, y: 0.0 },
        _ => return None,
    })
}

pub fn right_angle(projection: &Projection, point_ref: &str) -> bool {
    let arms = intersection_arms(projection, point_ref);
    let [(first, first_end), (second, second_end)] = arms.as_slice() else {
        return false;
    };
    let a = projection.segments[*first].tangent_at(if *first_end { 1.0 } else { 0.0 });
    let b = projection.segments[*second].tangent_at(if *second_end { 1.0 } else { 0.0 });
    let dot = a.x * b.x + a.y * b.y;
    let lengths = (a.x * a.x + a.y * a.y).sqrt() * (b.x * b.x + b.y * b.y).sqrt();
    lengths > 0.000001 && dot.abs() <= lengths * 0.35
}

pub fn intersection_arms(projection: &Projection, point_ref: &str) -> Vec<(usize, bool)> {
    projection
        .segments
        .iter()
        .enumerate()
        .flat_map(|(index, segment)| {
            [
                (segment.start_ref.as_deref() == Some(point_ref)).then_some((index, false)),
                (segment.end_ref.as_deref() == Some(point_ref)).then_some((index, true)),
            ]
            .into_iter()
            .flatten()
        })
        .collect()
}

pub fn intersections(program: &Program, projection: &Projection) -> Vec<Intersection> {
    program.statements.iter().filter_map(|statement| {
        let Statement::Call(call) = statement else { return None };
        let kind = IntersectionKind::from_call_name(&call.name)?;
        let point_ref = match call.args.first()? { Argument::Identifier(name) => name, _ => return None };
        let placed = call.args.len() == kind.arm_count() * 2 + 2;
        if !placed && !matches!(call.args.len(), 1 | 2) { return None; }
        let point = projection.points.iter().find(|point| point.name == *point_ref)?;
        if !program.statements.iter().any(|statement| matches!(statement,
            Statement::Call(call) if call.name == "map_point" && call.assigns.first() == Some(point_ref)))
            || (!placed && intersection_arms(projection, point_ref).len() != kind.arm_count())
            || (!placed && kind == IntersectionKind::RightTurn && !right_angle(projection, point_ref)) {
            return None;
        }
        let road_name = match call.args.get(1) {
            Some(Argument::Identifier(name)) => name.clone(),
            None => projection.segments.get(intersection_arms(projection, point_ref)[0].0)?.road_name.clone(),
            _ => return None,
        };
        if !program.statements.iter().any(|statement| matches!(statement,
            Statement::Road(road) if road.name == road_name)) { return None; }
        let ports = if placed {
            call.args[2..].chunks_exact(2).map(|pair| {
                let (Argument::Identifier(side), Argument::Identifier(name)) = (&pair[0], &pair[1]) else { return None };
                let direction = port_direction_from_name(side)?;
                let port = projection.points.iter().find(|point| point.name == *name)?;
                Some(IntersectionPort {
                    point_ref: name.clone(), position: port.position, direction,
                })
            }).collect::<Option<Vec<_>>>()?
        } else { Vec::new() };
        Some(Intersection { point_ref: point_ref.clone(), position: point.position, kind, road_name, ports })
    }).collect()
}

pub fn place_intersection(
    program: &mut Program,
    kind: IntersectionKind,
    road: &RoadDefinition,
    position: WorldPoint,
) -> String {
    let mut suffix = 1;
    let center = loop {
        let candidate = format!("intersection_{suffix}");
        if name_available(program, &candidate, None)
            && (1..=kind.arm_count())
                .all(|index| name_available(program, &format!("{candidate}_port_{index}"), None))
        {
            break candidate;
        }
        suffix += 1;
    };
    let half = road
        .elements
        .iter()
        .map(|element| element_world_width(element.kind))
        .sum::<f32>()
        * 0.5;
    program.statements.push(Statement::Call(Call {
        assigns: vec![center.clone()],
        name: "map_point".into(),
        args: vec![number_arg(position.x), number_arg(position.y)],
    }));
    let mut args = vec![
        Argument::Identifier(center.clone()),
        Argument::Identifier(road.name.clone()),
    ];
    for (index, direction) in port_directions(kind).iter().enumerate() {
        let name = format!("{center}_port_{}", index + 1);
        let port = WorldPoint {
            x: position.x + direction.x * half,
            y: position.y + direction.y * half,
        };
        program.statements.push(Statement::Call(Call {
            assigns: vec![name.clone()],
            name: "map_point".into(),
            args: vec![number_arg(port.x), number_arg(port.y)],
        }));
        args.push(Argument::Identifier(port_direction_name(*direction).into()));
        args.push(Argument::Identifier(name));
    }
    program.statements.push(Statement::Call(Call {
        assigns: vec![],
        name: kind.call_name().into(),
        args,
    }));
    center
}

pub fn intersection_port_direction(program: &Program, point_ref: &str) -> Option<WorldPoint> {
    program.statements.iter().find_map(|statement| {
        let Statement::Call(call) = statement else {
            return None;
        };
        let kind = IntersectionKind::from_call_name(&call.name)?;
        if call.args.len() != kind.arm_count() * 2 + 2 {
            return None;
        }
        call.args[2..].chunks_exact(2).find_map(|pair| {
            let (Argument::Identifier(side), Argument::Identifier(name)) = (&pair[0], &pair[1])
            else {
                return None;
            };
            (name == point_ref)
                .then(|| port_direction_from_name(side))
                .flatten()
        })
    })
}

pub fn port_in_use(projection: &Projection, point_ref: &str) -> bool {
    projection.segments.iter().any(|segment| {
        segment.start_ref.as_deref() == Some(point_ref)
            || segment.end_ref.as_deref() == Some(point_ref)
    })
}

pub fn port_faces_road(program: &Program, point_ref: &str, other: WorldPoint) -> bool {
    let Some(direction) = intersection_port_direction(program, point_ref) else {
        return true;
    };
    let Some(position) = project(program)
        .points
        .into_iter()
        .find(|point| point.name == point_ref)
        .map(|point| point.position)
    else {
        return false;
    };
    (other.x - position.x) * direction.x + (other.y - position.y) * direction.y > 0.001
}

pub fn translate_intersection(program: &mut Program, point_ref: &str, offset: WorldPoint) -> bool {
    let projection = project(program);
    let Some(intersection) = intersections(program, &projection)
        .into_iter()
        .find(|intersection| intersection.point_ref == point_ref && !intersection.ports.is_empty())
    else {
        return false;
    };
    let port_refs: HashSet<_> = intersection
        .ports
        .iter()
        .map(|port| port.point_ref.as_str())
        .collect();
    for segment in &projection.segments {
        let SegmentSource::TopLevel(index) = segment.source else {
            continue;
        };
        let Some(Statement::Call(call)) = program.statements.get_mut(index) else {
            continue;
        };
        if call.args.len() != 6 {
            continue;
        }
        if segment
            .start_ref
            .as_deref()
            .is_some_and(|name| port_refs.contains(name))
        {
            call.args[1] = number_arg(segment.controls[0].x + offset.x);
            call.args[2] = number_arg(segment.controls[0].y + offset.y);
        }
        if segment
            .end_ref
            .as_deref()
            .is_some_and(|name| port_refs.contains(name))
        {
            call.args[3] = number_arg(segment.controls[1].x + offset.x);
            call.args[4] = number_arg(segment.controls[1].y + offset.y);
        }
    }
    let mut refs = port_refs;
    refs.insert(point_ref);
    for statement in &mut program.statements {
        let Statement::Call(call) = statement else {
            continue;
        };
        if call.name == "map_point"
            && call
                .assigns
                .first()
                .is_some_and(|name| refs.contains(name.as_str()))
        {
            let Some(point) = point_args(call, 0) else {
                continue;
            };
            call.args = vec![
                number_arg(point.x + offset.x),
                number_arg(point.y + offset.y),
            ];
        }
    }
    true
}

pub fn rotate_intersection(program: &mut Program, point_ref: &str) -> bool {
    let projection = project(program);
    let Some(intersection) = intersections(program, &projection)
        .into_iter()
        .find(|intersection| intersection.point_ref == point_ref && !intersection.ports.is_empty())
    else {
        return false;
    };
    for port in &intersection.ports {
        let direction = WorldPoint {
            x: port.direction.y,
            y: -port.direction.x,
        };
        let relative = WorldPoint {
            x: port.position.x - intersection.position.x,
            y: port.position.y - intersection.position.y,
        };
        let next = WorldPoint {
            x: intersection.position.x + relative.y,
            y: intersection.position.y - relative.x,
        };
        let offset = WorldPoint {
            x: next.x - port.position.x,
            y: next.y - port.position.y,
        };
        for segment in &projection.segments {
            let SegmentSource::TopLevel(index) = segment.source else {
                continue;
            };
            let Some(Statement::Call(call)) = program.statements.get_mut(index) else {
                continue;
            };
            if call.args.len() != 6 {
                continue;
            }
            if segment.start_ref.as_deref() == Some(&port.point_ref) {
                call.args[1] = number_arg(segment.controls[0].x + offset.x);
                call.args[2] = number_arg(segment.controls[0].y + offset.y);
            }
            if segment.end_ref.as_deref() == Some(&port.point_ref) {
                call.args[3] = number_arg(segment.controls[1].x + offset.x);
                call.args[4] = number_arg(segment.controls[1].y + offset.y);
            }
        }
        for statement in &mut program.statements {
            let Statement::Call(call) = statement else {
                continue;
            };
            if call.name == "map_point" && call.assigns.first() == Some(&port.point_ref) {
                call.args = vec![number_arg(next.x), number_arg(next.y)];
            } else if IntersectionKind::from_call_name(&call.name).is_some()
                && matches!(call.args.first(), Some(Argument::Identifier(name)) if name == point_ref)
            {
                for pair in call.args[2..].chunks_exact_mut(2) {
                    if matches!(&pair[1], Argument::Identifier(name) if name == &port.point_ref) {
                        pair[0] = Argument::Identifier(port_direction_name(direction).into());
                    }
                }
            }
        }
    }
    true
}

pub fn remove_placed_intersection(program: &mut Program, point_ref: &str) -> bool {
    let projection = project(program);
    let Some(intersection) = intersections(program, &projection)
        .into_iter()
        .find(|intersection| intersection.point_ref == point_ref && !intersection.ports.is_empty())
    else {
        return false;
    };
    let unused: HashSet<_> = intersection
        .ports
        .iter()
        .filter(|port| {
            !projection.segments.iter().any(|segment| {
                segment.start_ref.as_deref() == Some(&port.point_ref)
                    || segment.end_ref.as_deref() == Some(&port.point_ref)
            })
        })
        .map(|port| port.point_ref.as_str())
        .collect();
    program.statements.retain(|statement| match statement {
        Statement::Call(call) if IntersectionKind::from_call_name(&call.name).is_some() => {
            !matches!(call.args.first(), Some(Argument::Identifier(name)) if name == point_ref)
        }
        Statement::Call(call) if call.name == "map_point" => !call
            .assigns
            .first()
            .is_some_and(|name| name == point_ref || unused.contains(name.as_str())),
        _ => true,
    });
    true
}

pub fn set_intersection(
    program: &mut Program,
    point_ref: &str,
    kind: Option<IntersectionKind>,
    road_name: Option<&str>,
) -> Result<(), String> {
    let projection = project(program);
    if !program.statements.iter().any(|statement| matches!(statement,
        Statement::Call(call) if call.name == "map_point" && call.assigns.first().is_some_and(|name| name == point_ref))) {
        return Err(format!("Unknown map point: {point_ref}"));
    }
    let arms = intersection_arms(&projection, point_ref).len();
    if let Some(kind) = kind {
        if arms != kind.arm_count() {
            return Err(format!(
                "{point_ref} has {arms} road ends; {} required",
                kind.arm_count()
            ));
        }
        if kind == IntersectionKind::RightTurn && !right_angle(&projection, point_ref) {
            return Err(format!(
                "{point_ref} needs two road ends meeting at 90 degrees"
            ));
        }
        let name = road_name.ok_or("Intersection edge cross section is required")?;
        if !program.statements.iter().any(|statement| {
            matches!(statement,
            Statement::Road(road) if road.name == name)
        }) {
            return Err(format!("Unknown road type: {name}"));
        }
    }
    program.statements.retain(|statement| {
        !matches!(statement,
        Statement::Call(call) if IntersectionKind::from_call_name(&call.name).is_some()
            && matches!(call.args.first(), Some(Argument::Identifier(name)) if name == point_ref))
    });
    if let Some(kind) = kind {
        program.statements.push(Statement::Call(Call {
            assigns: Vec::new(),
            name: kind.call_name().into(),
            args: vec![
                Argument::Identifier(point_ref.into()),
                Argument::Identifier(road_name.unwrap().into()),
            ],
        }));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct MapPoint {
    pub name: String,
    pub position: WorldPoint,
    pub base: Option<WorldPoint>,
    pub base_direction: Option<WorldPoint>,
    pub direction: Option<WorldPoint>,
}

impl MapPoint {
    fn control_from_stored(&self, stored: WorldPoint) -> WorldPoint {
        let base = self.base.unwrap_or(self.position);
        let dx = stored.x - base.x;
        let dy = stored.y - base.y;
        if let (Some(from), Some(to)) = (self.base_direction, self.direction) {
            let cosine = from.x * to.x + from.y * to.y;
            let sine = from.x * to.y - from.y * to.x;
            WorldPoint {
                x: self.position.x + dx * cosine - dy * sine,
                y: self.position.y + dx * sine + dy * cosine,
            }
        } else {
            WorldPoint {
                x: self.position.x + dx,
                y: self.position.y + dy,
            }
        }
    }

    fn stored_from_control(&self, control: WorldPoint) -> WorldPoint {
        let base = self.base.unwrap_or(self.position);
        let dx = control.x - self.position.x;
        let dy = control.y - self.position.y;
        if let (Some(from), Some(to)) = (self.base_direction, self.direction) {
            let cosine = from.x * to.x + from.y * to.y;
            let sine = from.x * to.y - from.y * to.x;
            WorldPoint {
                x: base.x + dx * cosine + dy * sine,
                y: base.y - dx * sine + dy * cosine,
            }
        } else {
            WorldPoint {
                x: base.x + dx,
                y: base.y + dy,
            }
        }
    }
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

fn line_controls(start: WorldPoint, end: WorldPoint) -> [WorldPoint; 2] {
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
    points: &'a [MapPoint],
) -> Option<(&'a MapPoint, String)> {
    let Argument::Identifier(name) = argument else {
        return None;
    };
    points
        .iter()
        .find(|point| point.name == *name)
        .map(|point| (point, name.clone()))
}

fn segment_points(
    call: &Call,
    points: &[MapPoint],
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
    if call.args.len() != 2 {
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
            if let Some((start, end, controls, start_ref, end_ref)) = segment_points(call, &points)
            {
                segments.push(Segment {
                    source: SegmentSource::TopLevel(index),
                    name: call.assigns.first().cloned(),
                    road_name: call.name.clone(),
                    start,
                    end,
                    controls,
                    start_ref,
                    end_ref,
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
            if let Some((start, end, controls, start_ref, end_ref)) = segment_points(inner, &points)
            {
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
                    controls: [
                        controls[0].translated(origin),
                        controls[1].translated(origin),
                    ],
                    start_ref,
                    end_ref,
                });
            } else {
                unresolved += 1;
            }
        }
    }
    Projection {
        segments,
        points: points.to_vec(),
        unresolved,
    }
}

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

fn element_direction(
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

fn number_arg(value: f32) -> Argument {
    let value = if value.abs() < 0.005 { 0.0 } else { value };
    Argument::Number(format!("{value:.4}"))
}

fn name_available(program: &Program, name: &str, except: Option<usize>) -> bool {
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

fn ensure_point(program: &mut Program, position: WorldPoint, existing: Option<&str>) -> String {
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

#[cfg(test)]
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

fn attachment_direction(program: &Program, reference: &str) -> Option<WorldPoint> {
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
    element_direction(parent, road, element_index, t)
}

fn junction_direction(
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
    (index, end_name)
}

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
                && intersection.road_name == segment.road_name
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

fn road_with_elements(
    program: &mut Program,
    base: &str,
    elements: Vec<RoadElement>,
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
            if existing.elements == elements {
                return existing.clone();
            }
        } else if name_available(program, &name, None) {
            let road = RoadDefinition { name, elements };
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
    let cross = direction.x * (end.y - start.y) - direction.y * (end.x - start.x);
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
    Some(ExitPlacement {
        through,
        entry: SegmentSource::TopLevel(entry_index),
        full: SegmentSource::TopLevel(full_index),
        end_ref: final_ref,
        side,
    })
}

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

fn set_control_direct(
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
            .filter(move |(_, _, endpoint, endpoint_ref)| {
                *endpoint == original || (reference.is_some() && *endpoint_ref == reference)
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
            Statement::Call(call) if call.name == "lane_split" && call.args.iter().enumerate()
                .any(|(index, arg)| index != 2 && matches!(arg, Argument::Identifier(road) if road == &name)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use opencity::parse;

    #[test]
    fn standalone_intersection_has_edge_ports_and_roads_stop_at_them() {
        let mut program = parse("road street: sidewalk-carlane-sidewalk; end;").unwrap();
        let Statement::Road(road) = &program.statements[0] else {
            unreachable!()
        };
        let road = road.clone();
        let center = place_intersection(
            &mut program,
            IntersectionKind::FourWay,
            &road,
            WorldPoint { x: 2.0, y: 3.0 },
        );
        let junction = intersections(&program, &project(&program)).pop().unwrap();
        assert_eq!(junction.ports.len(), 4);
        assert_eq!(intersection_arms(&project(&program), &center).len(), 0);
        let east = &junction.ports[1];
        let half = road
            .elements
            .iter()
            .map(|element| element_world_width(element.kind))
            .sum::<f32>()
            / 2.0;
        assert!((east.position.x - (2.0 + half)).abs() < 0.0001);
        assert_eq!(east.position.y, 3.0);
        assert!(port_faces_road(
            &program,
            &east.point_ref,
            WorldPoint { x: 4.0, y: 3.0 }
        ));
        assert!(!port_faces_road(
            &program,
            &east.point_ref,
            WorldPoint { x: 2.0, y: 3.0 }
        ));
        let (index, _) = append_connected(
            &mut program,
            &road,
            east.position,
            Some(&east.point_ref),
            WorldPoint { x: 4.0, y: 3.0 },
            None,
        );
        let segment = project(&program)
            .segments
            .into_iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(index))
            .unwrap();
        assert_eq!(segment.start_ref.as_deref(), Some(east.point_ref.as_str()));
        assert_eq!(segment.start, east.position);
        assert!(segment.controls[0].x > segment.start.x);
        assert!(!set_connected_endpoint(
            &mut program,
            SegmentSource::TopLevel(index),
            false,
            WorldPoint { x: 8.0, y: 3.0 }
        ));
        assert!(translate_intersection(
            &mut program,
            &center,
            WorldPoint { x: 1.0, y: -1.0 }
        ));
        let moved = project(&program)
            .segments
            .into_iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(index))
            .unwrap();
        assert!((moved.start.x - (east.position.x + 1.0)).abs() < 0.0002);
        assert!((moved.controls[0].x - (segment.controls[0].x + 1.0)).abs() < 0.0002);
        let (approach_index, _) = append_connected(
            &mut program,
            &road,
            WorldPoint { x: 0.0, y: 2.0 },
            None,
            WorldPoint { x: 1.0, y: 2.0 },
            None,
        );
        let west_ref = intersections(&program, &project(&program))[0].ports[3]
            .point_ref
            .clone();
        assert!(attach_endpoint_to_port(
            &mut program,
            SegmentSource::TopLevel(approach_index),
            true,
            &west_ref
        ));
        let approach = project(&program)
            .segments
            .into_iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(approach_index))
            .unwrap();
        assert_eq!(approach.end_ref.as_deref(), Some(west_ref.as_str()));
        assert!(approach.controls[1].x < approach.end.x);
        prune_unused_points(&mut program);
        assert_eq!(intersections(&program, &project(&program)).len(), 1);
        assert!(remove_placed_intersection(&mut program, &center));
        assert!(intersections(&program, &project(&program)).is_empty());
        assert_eq!(project(&program).segments.len(), 2);
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    }

    #[test]
    fn rotating_a_three_way_intersection_moves_its_ports() {
        let mut program = parse("road street: sidewalk-carlane-sidewalk; end;").unwrap();
        let Statement::Road(road) = &program.statements[0] else {
            unreachable!()
        };
        let road = road.clone();
        let center = place_intersection(
            &mut program,
            IntersectionKind::ThreeWay,
            &road,
            WorldPoint { x: 0.0, y: 0.0 },
        );
        let before = intersections(&program, &project(&program)).pop().unwrap();
        let north_ref = before.ports[0].point_ref.clone();
        assert_eq!(before.ports[0].direction, WorldPoint { x: 0.0, y: 1.0 });
        assert!(rotate_intersection(&mut program, &center));
        let after = intersections(&program, &project(&program)).pop().unwrap();
        let rotated = after
            .ports
            .iter()
            .find(|port| port.point_ref == north_ref)
            .unwrap();
        assert_eq!(rotated.direction, WorldPoint { x: 1.0, y: 0.0 });
        assert!(rotated.position.x > 0.0);
        assert_eq!(after.ports.len(), 3);
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    }

    #[test]
    fn three_and_four_way_intersections_require_matching_shared_road_ends() {
        let mut program = parse(
            "road street: sidewalk-carlane-sidewalk; end; \
            center = map_point(0, 0); west = map_point(-1, 0); east = map_point(1, 0); \
            north = map_point(0, 1); south = map_point(0, -1); \
            a = street(west, -0.7, 0, -0.3, 0, center); \
            b = street(center, 0.3, 0, 0.7, 0, east); \
            c = street(center, 0, 0.3, 0, 0.7, north);",
        )
        .unwrap();
        assert_eq!(intersection_arms(&project(&program), "center").len(), 3);
        assert!(set_intersection(
            &mut program,
            "center",
            Some(IntersectionKind::FourWay),
            Some("street")
        )
        .is_err());
        set_intersection(
            &mut program,
            "center",
            Some(IntersectionKind::ThreeWay),
            Some("street"),
        )
        .unwrap();
        assert_eq!(intersections(&program, &project(&program)).len(), 1);
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
        program.statements.push(Statement::Call(Call {
            assigns: vec!["d".into()],
            name: "street".into(),
            args: vec![
                Argument::Identifier("south".into()),
                Argument::Number("0".into()),
                Argument::Number("-0.7".into()),
                Argument::Number("0".into()),
                Argument::Number("-0.3".into()),
                Argument::Identifier("center".into()),
            ],
        }));
        assert_eq!(project(&program).unresolved, 1);
        set_intersection(
            &mut program,
            "center",
            Some(IntersectionKind::FourWay),
            Some("street"),
        )
        .unwrap();
        assert_eq!(project(&program).unresolved, 0);
        assert_eq!(
            intersections(&program, &project(&program))[0].kind,
            IntersectionKind::FourWay
        );
        set_intersection(&mut program, "center", None, None).unwrap();
        assert!(intersections(&program, &project(&program)).is_empty());
    }

    #[test]
    fn right_turn_requires_two_perpendicular_arms() {
        let mut program = parse(
            "road lane: sidewalk-carlane-sidewalk; end; \
            center = map_point(0, 0); west = map_point(-1, 0); north = map_point(0, 1); \
            a = lane(west, -0.7, 0, -0.3, 0, center); \
            b = lane(center, 0, 0.3, 0, 0.7, north);",
        )
        .unwrap();
        set_intersection(
            &mut program,
            "center",
            Some(IntersectionKind::RightTurn),
            Some("lane"),
        )
        .unwrap();
        assert_eq!(
            intersections(&program, &project(&program))[0].kind,
            IntersectionKind::RightTurn
        );
        let Statement::Call(b) = program
            .statements
            .iter_mut()
            .find(|statement| {
                matches!(statement,
            Statement::Call(call) if call.assigns.first().is_some_and(|name| name == "b"))
            })
            .unwrap()
        else {
            unreachable!()
        };
        b.args[1] = Argument::Number("0.3".into());
        b.args[2] = Argument::Number("0".into());
        assert_eq!(project(&program).unresolved, 1);
    }

    #[test]
    fn extending_and_joining_old_endpoints_creates_shared_junctions() {
        let mut program = parse("road street: verge-carlane-verge; end;").unwrap();
        let road = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let first = append(
            &mut program,
            &road,
            WorldPoint { x: 0.0, y: 0.0 },
            WorldPoint { x: 2.0, y: 0.0 },
        );
        let second = append(
            &mut program,
            &road,
            WorldPoint { x: 4.0, y: 1.0 },
            WorldPoint { x: 6.0, y: 1.0 },
        );
        let (start, start_ref) =
            link_endpoint(&mut program, SegmentSource::TopLevel(first), true).unwrap();
        let (end, end_ref) =
            link_endpoint(&mut program, SegmentSource::TopLevel(second), false).unwrap();
        let (bridge, _) = append_connected(
            &mut program,
            &road,
            start,
            Some(&start_ref),
            end,
            Some(&end_ref),
        );
        let projection = project(&program);
        let first_segment = projection
            .segments
            .iter()
            .find(|s| s.source == SegmentSource::TopLevel(first))
            .unwrap();
        let bridge_segment = projection
            .segments
            .iter()
            .find(|s| s.source == SegmentSource::TopLevel(bridge))
            .unwrap();
        let second_segment = projection
            .segments
            .iter()
            .find(|s| s.source == SegmentSource::TopLevel(second))
            .unwrap();
        assert_eq!(first_segment.end_ref, bridge_segment.start_ref);
        assert_eq!(second_segment.start_ref, bridge_segment.end_ref);
        assert_eq!(bridge_segment.road_name, road.name);
        let first_tangent = WorldPoint {
            x: first_segment.end.x - first_segment.controls[1].x,
            y: first_segment.end.y - first_segment.controls[1].y,
        };
        let bridge_tangent = WorldPoint {
            x: bridge_segment.controls[0].x - bridge_segment.start.x,
            y: bridge_segment.controls[0].y - bridge_segment.start.y,
        };
        assert!(
            (first_tangent.x * bridge_tangent.y - first_tangent.y * bridge_tangent.x).abs()
                < 0.0001
        );
        assert!(set_connected_endpoint(
            &mut program,
            SegmentSource::TopLevel(first),
            true,
            WorldPoint { x: 2.0, y: 0.5 }
        ));
        let projection = project(&program);
        let bridge_segment = projection
            .segments
            .iter()
            .find(|s| s.source == SegmentSource::TopLevel(bridge))
            .unwrap();
        assert_eq!(bridge_segment.start, WorldPoint { x: 2.0, y: 0.5 });
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    }

    #[test]
    fn rotating_an_endpoint_preserves_handle_length_and_connected_tangent() {
        let mut program = parse("road street: verge-carlane-verge; end;").unwrap();
        let road = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let first = append(
            &mut program,
            &road,
            WorldPoint { x: 0.0, y: 0.0 },
            WorldPoint { x: 2.0, y: 0.0 },
        );
        let (start, reference) =
            link_endpoint(&mut program, SegmentSource::TopLevel(first), true).unwrap();
        let (second, _) = append_connected(
            &mut program,
            &road,
            start,
            Some(&reference),
            WorldPoint { x: 4.0, y: 0.0 },
            None,
        );
        assert!(rotate_endpoint(
            &mut program,
            SegmentSource::TopLevel(first),
            true,
            15.0
        ));
        let projection = project(&program);
        let first_segment = projection
            .segments
            .iter()
            .find(|s| s.source == SegmentSource::TopLevel(first))
            .unwrap();
        let second_segment = projection
            .segments
            .iter()
            .find(|s| s.source == SegmentSource::TopLevel(second))
            .unwrap();
        let incoming = WorldPoint {
            x: first_segment.end.x - first_segment.controls[1].x,
            y: first_segment.end.y - first_segment.controls[1].y,
        };
        let outgoing = WorldPoint {
            x: second_segment.controls[0].x - second_segment.start.x,
            y: second_segment.controls[0].y - second_segment.start.y,
        };
        assert!((incoming.x - outgoing.x).abs() < 0.0001);
        assert!((incoming.y - outgoing.y).abs() < 0.0001);
        assert!((incoming.x.powi(2) + incoming.y.powi(2) - (2.0 / 3.0_f32).powi(2)).abs() < 0.0001);
    }

    #[test]
    fn exits_carry_selected_elements_and_leave_a_divided_through_road() {
        for (element_index, target_y, side) in
            [(3, -2.0, ExitSide::Right), (1, 2.0, ExitSide::Left)]
        {
            let mut program = parse("road highway: verge-carlane-middle-carlane-verge; end; road ramp: verge-carlane-verge; end;").unwrap();
            let highway = match &program.statements[0] {
                Statement::Road(road) => road.clone(),
                _ => unreachable!(),
            };
            let ramp = match &program.statements[1] {
                Statement::Road(road) => road.clone(),
                _ => unreachable!(),
            };
            let parent = append(
                &mut program,
                &highway,
                WorldPoint { x: 0.0, y: 0.0 },
                WorldPoint { x: 4.0, y: 0.0 },
            );
            let (attachment, start) = create_lane_point(
                &mut program,
                SegmentSource::TopLevel(parent),
                element_index,
                0.5,
            )
            .unwrap();
            let exit = append_exit(
                &mut program,
                &ramp,
                SplitMode::Dedicated,
                start,
                &attachment,
                WorldPoint {
                    x: 6.0,
                    y: target_y,
                },
                None,
            )
            .unwrap();
            assert_eq!(exit.side, side);
            let projection = project(&program);
            let entry = projection
                .segments
                .iter()
                .find(|segment| segment.source == exit.entry)
                .unwrap();
            let full = projection
                .segments
                .iter()
                .find(|segment| segment.source == exit.full)
                .unwrap();
            let entry_road = program
                .statements
                .iter()
                .find_map(|statement| match statement {
                    Statement::Road(road) if road.name == entry.road_name => Some(road),
                    _ => None,
                })
                .unwrap();
            assert_eq!(full.road_name, entry.road_name);
            match side {
                ExitSide::Right => assert_eq!(entry_road.elements, highway.elements[3..]),
                ExitSide::Left => assert_eq!(entry_road.elements, highway.elements[..=1]),
            }
            let through = projection
                .segments
                .iter()
                .find(|segment| Some(segment.source) == exit.through)
                .unwrap();
            let through_road = program
                .statements
                .iter()
                .find_map(|statement| match statement {
                    Statement::Road(road) if road.name == through.road_name => Some(road),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                through_road
                    .elements
                    .iter()
                    .filter(|element| element.kind == RoadElementKind::Barrier)
                    .count(),
                1
            );
            assert_eq!(
                entry_road.elements.len() + through_road.elements.len(),
                highway.elements.len() + 1
            );
            assert_eq!(
                projection
                    .segments
                    .iter()
                    .find(|segment| segment.source == SegmentSource::TopLevel(parent))
                    .unwrap()
                    .end,
                WorldPoint { x: 2.0, y: 0.0 }
            );
            assert_eq!(entry.end_ref, full.start_ref);
            assert_eq!(entry.end, full.start);
            let incoming = WorldPoint {
                x: entry.end.x - entry.controls[1].x,
                y: entry.end.y - entry.controls[1].y,
            };
            let outgoing = WorldPoint {
                x: full.controls[0].x - full.start.x,
                y: full.controls[0].y - full.start.y,
            };
            let cross = incoming.x * outgoing.y - incoming.y * outgoing.x;
            assert!(cross.abs() < 0.00001);
            assert!(create_two_segment_split(
                &mut program,
                &attachment,
                exit.entry,
                exit.full,
                SplitMode::Dedicated
            ));
            let split = &lane_splits(&program)[0];
            assert_eq!(split.full_ramp.as_deref(), full.name.as_deref());
            assert_eq!(parse(&program.to_road_source()).unwrap(), program);
            assert!(set_name(&mut program, exit.full, "renamed_exit".into()));
            assert_eq!(
                lane_splits(&program)[0].full_ramp.as_deref(),
                Some("renamed_exit")
            );
            let entry_name = lane_splits(&program)[0].ramp.clone();
            assert!(set_split_mode(
                &mut program,
                &entry_name,
                SplitMode::KeepLane
            ));
            assert_eq!(lane_splits(&program)[0].mode, SplitMode::KeepLane);
            assert_eq!(parse(&program.to_road_source()).unwrap(), program);
            assert!(set_road_type(
                &mut program,
                exit.through.unwrap(),
                &ramp.name
            ));
            assert_eq!(
                project(&program)
                    .segments
                    .iter()
                    .find(|segment| Some(segment.source) == exit.through)
                    .unwrap()
                    .road_name,
                ramp.name
            );
        }
    }

    #[test]
    fn an_exit_carrying_the_whole_section_ends_the_parent_at_the_split() {
        let mut program = parse("road path: sidewalk; end;").unwrap();
        let road = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let parent = append(
            &mut program,
            &road,
            WorldPoint { x: 0.0, y: 0.0 },
            WorldPoint { x: 4.0, y: 0.0 },
        );
        let (attachment, start) =
            create_lane_point(&mut program, SegmentSource::TopLevel(parent), 0, 0.5).unwrap();
        let exit = append_exit(
            &mut program,
            &road,
            SplitMode::Dedicated,
            start,
            &attachment,
            WorldPoint { x: 4.0, y: -2.0 },
            None,
        )
        .unwrap();
        assert!(exit.through.is_none());
        let projection = project(&program);
        assert_eq!(
            projection
                .segments
                .iter()
                .find(|segment| segment.source == SegmentSource::TopLevel(parent))
                .unwrap()
                .end,
            WorldPoint { x: 2.0, y: 0.0 }
        );
        assert_eq!(projection.unresolved, 0);
    }

    #[test]
    fn a_start_end_exit_transfers_elements_without_a_zero_length_segment() {
        let mut program = parse("road highway: carlane-carlane-sidewalk; end;").unwrap();
        let road = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let parent = append(
            &mut program,
            &road,
            WorldPoint { x: 0.0, y: 0.0 },
            WorldPoint { x: 4.0, y: 0.0 },
        );
        let (attachment, start) =
            create_lane_point(&mut program, SegmentSource::TopLevel(parent), 1, 0.0).unwrap();
        let exit = append_exit(
            &mut program,
            &road,
            SplitMode::Dedicated,
            start,
            &attachment,
            WorldPoint { x: 3.0, y: -2.0 },
            None,
        )
        .unwrap();
        assert!(exit.through.is_some());
        let projection = project(&program);
        assert_eq!(projection.unresolved, 0);
        assert!(
            projection
                .segments
                .iter()
                .find(|segment| segment.source == SegmentSource::TopLevel(parent))
                .unwrap()
                .end
                .x
                > 0.0
        );
        assert_eq!(lane_attachment(&program, &attachment).unwrap().2, 1.0);
    }

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
        let mut program = parse("road lane: verge-carlane-verge; end;").unwrap();
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
        assert!(set_connected_endpoint(
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

    #[test]
    fn named_curves_reuse_cross_sections_and_move_connected_junctions() {
        let mut program = parse("road avenue: sidewalk-carlane-sidewalk; end;").unwrap();
        let road = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let first = append_curve(
            &mut program,
            &road,
            WorldPoint { x: 0.0, y: 0.0 },
            WorldPoint { x: 0.0, y: 1.0 },
            WorldPoint { x: 1.0, y: 1.0 },
            WorldPoint { x: 1.0, y: 0.0 },
        );
        let second = append_curve(
            &mut program,
            &road,
            WorldPoint { x: 1.0, y: 0.0 },
            WorldPoint { x: 1.0, y: -1.0 },
            WorldPoint { x: 2.0, y: -1.0 },
            WorldPoint { x: 2.0, y: 0.0 },
        );
        assert_eq!(
            project(&program).segments[0].point_at(0.5),
            WorldPoint { x: 0.5, y: 0.75 }
        );
        assert!(set_name(
            &mut program,
            SegmentSource::TopLevel(first),
            "curved_main".into()
        ));
        assert!(set_name(
            &mut program,
            SegmentSource::TopLevel(second),
            "second_main".into()
        ));
        assert!(set_connected_endpoint(
            &mut program,
            SegmentSource::TopLevel(first),
            true,
            WorldPoint { x: 1.5, y: 0.5 }
        ));
        let segments = project(&program).segments;
        assert_eq!(segments[0].end, segments[1].start);
        assert_eq!(segments[0].controls[1], WorldPoint { x: 1.5, y: 1.5 });
        assert_eq!(segments[0].road_name, segments[1].road_name);
        assert_ne!(segments[0].name, segments[1].name);
        assert!(set_control(
            &mut program,
            SegmentSource::TopLevel(first),
            true,
            WorldPoint { x: 2.0, y: 1.5 }
        ));
        assert_eq!(
            project(&program).segments[1].controls[0],
            WorldPoint { x: 1.0, y: -0.5 }
        );
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    }

    #[test]
    fn clicked_junction_is_shared_and_moves_both_roads() {
        let mut program = parse("road street: sidewalk-carlane-sidewalk; end;").unwrap();
        let road = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let (first, junction) = append_connected(
            &mut program,
            &road,
            WorldPoint { x: 0.0, y: 0.0 },
            None,
            WorldPoint { x: 1.0, y: 0.0 },
            None,
        );
        let (second, _) = append_connected(
            &mut program,
            &road,
            WorldPoint { x: 1.0, y: 0.0 },
            Some(&junction),
            WorldPoint { x: 2.0, y: 0.0 },
            None,
        );
        let segments = project(&program).segments;
        assert_eq!(segments[0].end_ref.as_deref(), Some(junction.as_str()));
        assert_eq!(segments[1].start_ref.as_deref(), Some(junction.as_str()));
        assert!(set_connected_endpoint(
            &mut program,
            SegmentSource::TopLevel(first),
            true,
            WorldPoint { x: 1.0, y: 0.5 }
        ));
        let segments = project(&program).segments;
        assert_eq!(segments[0].end, segments[1].start);
        assert_eq!(segments[1].start, WorldPoint { x: 1.0, y: 0.5 });
        assert!(remove(&mut program, SegmentSource::TopLevel(second)));
        assert_eq!(project(&program).segments.len(), 1);
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    }

    #[test]
    fn continuation_inherits_curve_direction_at_shared_end() {
        let mut program = parse("road street: sidewalk-carlane-sidewalk; end;").unwrap();
        let road = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let (first, junction) = append_connected(
            &mut program,
            &road,
            WorldPoint { x: 0.0, y: 0.0 },
            None,
            WorldPoint { x: 2.0, y: 0.0 },
            None,
        );
        assert!(set_control_direct(
            &mut program,
            SegmentSource::TopLevel(first),
            true,
            WorldPoint { x: 1.5, y: -0.5 }
        ));
        let (second, _) = append_connected(
            &mut program,
            &road,
            WorldPoint { x: 2.0, y: 0.0 },
            Some(&junction),
            WorldPoint { x: 3.0, y: 1.0 },
            None,
        );
        let segments = project(&program).segments;
        let incoming = WorldPoint {
            x: segments[0].end.x - segments[0].controls[1].x,
            y: segments[0].end.y - segments[0].controls[1].y,
        };
        let outgoing = WorldPoint {
            x: segments[1].controls[0].x - segments[1].start.x,
            y: segments[1].controls[0].y - segments[1].start.y,
        };
        assert!((incoming.x * outgoing.y - incoming.y * outgoing.x).abs() < 0.0001);
        assert!(incoming.x * outgoing.x + incoming.y * outgoing.y > 0.0);
        assert_eq!(segments[1].source, SegmentSource::TopLevel(second));
    }

    #[test]
    fn lane_branch_follows_parent_shape_and_name() {
        let mut program = parse("road highway: verge-carlane-carlane-verge; end; road ramp: verge-carlane-verge; end;").unwrap();
        let parent_road = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let ramp = match &program.statements[1] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let (parent, _) = append_connected(
            &mut program,
            &parent_road,
            WorldPoint { x: 0.0, y: 0.0 },
            None,
            WorldPoint { x: 4.0, y: 0.0 },
            None,
        );
        let (attachment, position) =
            create_lane_point(&mut program, SegmentSource::TopLevel(parent), 1, 0.5).unwrap();
        let (branch, _) = append_connected(
            &mut program,
            &ramp,
            position,
            Some(&attachment),
            WorldPoint { x: 3.0, y: -2.0 },
            None,
        );
        let before = project(&program)
            .segments
            .into_iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(branch))
            .unwrap();
        assert!(set_control(
            &mut program,
            SegmentSource::TopLevel(parent),
            false,
            WorldPoint { x: 1.0, y: 1.0 }
        ));
        let after = project(&program)
            .segments
            .into_iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(branch))
            .unwrap();
        assert_ne!(before.start, after.start);
        let parent_after = project(&program)
            .segments
            .into_iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(parent))
            .unwrap();
        let direction = element_direction(&parent_after, &parent_road, 1, 0.5).unwrap();
        let handle = WorldPoint {
            x: after.controls[0].x - after.start.x,
            y: after.controls[0].y - after.start.y,
        };
        assert!((handle.x * direction.y - handle.y * direction.x).abs() < 0.001);
        assert!(handle.x * direction.x + handle.y * direction.y > 0.0);
        assert!(set_name(
            &mut program,
            SegmentSource::TopLevel(parent),
            "interstate_west".into()
        ));
        assert_eq!(
            project(&program)
                .segments
                .iter()
                .find(|segment| segment.source == SegmentSource::TopLevel(branch))
                .unwrap()
                .start,
            after.start
        );
        assert!(set_lane_attachment(&mut program, &attachment, 2, 0.75));
        let relocated = project(&program)
            .segments
            .into_iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(branch))
            .unwrap();
        assert_ne!(relocated.start, after.start);
        assert!(remove(&mut program, SegmentSource::TopLevel(parent)));
        let remaining = project(&program).segments;
        assert_eq!(remaining.len(), 1);
        assert!((remaining[0].start.x - relocated.start.x).abs() < 0.01);
        assert!((remaining[0].start.y - relocated.start.y).abs() < 0.01);
        assert!((remaining[0].controls[0].x - relocated.controls[0].x).abs() < 0.01);
        assert!((remaining[0].controls[0].y - relocated.controls[0].y).abs() < 0.01);
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    }

    #[test]
    fn branch_uses_visible_element_side_and_parent_tangent() {
        let mut program = parse("road highway: verge-carlane-middle-carlane-verge; end; road ramp: verge-carlane-verge; end;").unwrap();
        let parent_road = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let ramp = match &program.statements[1] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let (parent, _) = append_connected(
            &mut program,
            &parent_road,
            WorldPoint { x: 0.0, y: 0.0 },
            None,
            WorldPoint { x: 4.0, y: 0.0 },
            None,
        );
        let segment = project(&program)
            .segments
            .into_iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(parent))
            .unwrap();
        let upper = element_position(&segment, &parent_road, 1, 0.5).unwrap();
        let lower = element_position(&segment, &parent_road, 3, 0.5).unwrap();
        assert!(upper.y > 0.0);
        assert!(lower.y < 0.0);
        assert!((upper.x - 2.0).abs() < 0.0001);

        let (attachment, start) =
            create_lane_point(&mut program, SegmentSource::TopLevel(parent), 1, 0.5).unwrap();
        let (branch, _) = append_connected(
            &mut program,
            &ramp,
            start,
            Some(&attachment),
            WorldPoint { x: 3.0, y: 1.0 },
            None,
        );
        let projected = project(&program)
            .segments
            .into_iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(branch))
            .unwrap();
        assert!((projected.start.y - upper.y).abs() < 0.0001);
        assert!((projected.controls[0].y - projected.start.y).abs() < 0.0001);
        assert!(projected.controls[0].x > projected.start.x);
    }

    #[test]
    fn lane_branch_tracks_cross_section_edits_and_detaches_when_lane_is_removed() {
        use opencity::{RoadElement, RoadElementKind};

        let mut program = parse("road highway: verge-carlane-carlane-verge; end; road ramp: verge-carlane-verge; end;").unwrap();
        let parent_road = match &program.statements[0] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let ramp = match &program.statements[1] {
            Statement::Road(road) => road.clone(),
            _ => unreachable!(),
        };
        let (parent, _) = append_connected(
            &mut program,
            &parent_road,
            WorldPoint { x: 0.0, y: 0.0 },
            None,
            WorldPoint { x: 4.0, y: 0.0 },
            None,
        );
        let (attachment, position) =
            create_lane_point(&mut program, SegmentSource::TopLevel(parent), 1, 0.5).unwrap();
        let (branch, _) = append_connected(
            &mut program,
            &ramp,
            position,
            Some(&attachment),
            WorldPoint { x: 3.0, y: -2.0 },
            None,
        );

        let before = project(&program);
        if let Statement::Road(road) = &mut program.statements[0] {
            road.elements.insert(
                1,
                RoadElement {
                    kind: RoadElementKind::Sidewalk,
                    catch: None,
                },
            );
        }
        update_lane_attachments(&mut program, "highway", ElementEdit::Inserted(1), &before);
        assert_eq!(lane_attachment(&program, &attachment).unwrap().1, 2);

        let before = project(&program);
        if let Statement::Road(road) = &mut program.statements[0] {
            road.elements.swap(2, 3);
        }
        update_lane_attachments(&mut program, "highway", ElementEdit::Swapped(2, 3), &before);
        assert_eq!(lane_attachment(&program, &attachment).unwrap().1, 3);

        let before = project(&program);
        let branch_before = before
            .segments
            .iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(branch))
            .unwrap()
            .clone();
        if let Statement::Road(road) = &mut program.statements[0] {
            road.elements.remove(3);
        }
        update_lane_attachments(&mut program, "highway", ElementEdit::Removed(3), &before);
        let branch_after = project(&program)
            .segments
            .into_iter()
            .find(|segment| segment.source == SegmentSource::TopLevel(branch))
            .unwrap();
        assert!(lane_attachment(&program, &attachment).is_none());
        assert!((branch_after.start.x - branch_before.start.x).abs() < 0.01);
        assert!((branch_after.start.y - branch_before.start.y).abs() < 0.01);
        assert!((branch_after.controls[0].x - branch_before.controls[0].x).abs() < 0.01);
        assert!((branch_after.controls[0].y - branch_before.controls[0].y).abs() < 0.01);
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
    }
}
