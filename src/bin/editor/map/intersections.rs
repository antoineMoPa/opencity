//! Intersection ports, placement, and editing.

use super::*;

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
    pub footprint_half: f32,
    pub point_ref: String,
    pub position: WorldPoint,
    pub kind: IntersectionKind,
    pub road_name: String,
    pub arm_roads: Vec<(WorldPoint, String)>,
    pub ports: Vec<IntersectionPort>,
}

impl Intersection {
    pub fn road_name_for_direction(&self, direction: WorldPoint) -> &str {
        self.arm_roads
            .iter()
            .find(|(side, _)| side.x == direction.x && side.y == direction.y)
            .map(|(_, name)| name.as_str())
            .unwrap_or(&self.road_name)
    }

    pub fn road_name_for_segment(&self, segment: &Segment, is_end: bool) -> &str {
        let tangent = segment.tangent_at(if is_end { 1.0 } else { 0.0 });
        let outward = if is_end {
            WorldPoint {
                x: -tangent.x,
                y: -tangent.y,
            }
        } else {
            tangent
        };
        let direction = if outward.x.abs() > outward.y.abs() {
            WorldPoint {
                x: outward.x.signum(),
                y: 0.0,
            }
        } else {
            WorldPoint {
                x: 0.0,
                y: outward.y.signum(),
            }
        };
        self.road_name_for_direction(direction)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct IntersectionPort {
    pub point_ref: String,
    pub position: WorldPoint,
    pub direction: WorldPoint,
}

/// Existing placed ports define their footprint; center-connected curved junctions
/// reserve the turn radius authored by the shared component.
pub fn intersection_footprint_half(intersection: &Intersection, road_half: f32) -> f32 {
    if intersection.footprint_half > 0. {
        return intersection.footprint_half;
    }
    if !intersection.ports.is_empty() {
        return intersection
            .ports
            .iter()
            .map(|port| {
                (port.position.x - intersection.position.x)
                    .abs()
                    .max((port.position.y - intersection.position.y).abs())
            })
            .fold(road_half, f32::max);
    }
    if matches!(
        intersection.kind,
        IntersectionKind::FourWay | IntersectionKind::RightTurn
    ) {
        road_half * opencity::network::default_four_way_footprint_factor()
    } else {
        road_half
    }
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
    right_angle_arms(projection, &arms)
}

fn right_angle_arms(projection: &Projection, arms: &[(usize, bool)]) -> bool {
    let [(first, first_end), (second, second_end)] = arms else {
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

/// Return intersections from this document's projection, just like its segments
/// and points. Callers must project again after editing the source program.
pub fn intersections(_program: &Program, projection: &Projection) -> Vec<Intersection> {
    projection.intersections.clone()
}

pub(super) fn from_layout(program: &Program, projection: &Projection) -> Vec<Intersection> {
    let mut points = HashMap::with_capacity(projection.points.len());
    for point in &projection.points {
        points.entry(point.name.as_str()).or_insert(point);
    }
    let map_points: HashSet<&str> = program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Call(call) if call.name == "map_point" => {
                call.assigns.first().map(String::as_str)
            }
            _ => None,
        })
        .collect();
    let roads: HashSet<&str> = program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Road(road) => Some(road.name.as_str()),
            _ => None,
        })
        .collect();
    let mut arms_by_point: HashMap<&str, Vec<(usize, bool)>> = HashMap::new();
    for (index, segment) in projection.segments.iter().enumerate() {
        if let Some(name) = segment.start_ref.as_deref() {
            arms_by_point.entry(name).or_default().push((index, false));
        }
        if let Some(name) = segment.end_ref.as_deref() {
            arms_by_point.entry(name).or_default().push((index, true));
        }
    }
    let mut result: Vec<_> = program
        .statements
        .iter()
        .filter_map(|statement| {
            let Statement::Call(call) = statement else {
                return None;
            };
            let kind = IntersectionKind::from_call_name(&call.name)?;
            let point_ref = match call.args.first()? {
                Argument::Identifier(name) => name,
                _ => return None,
            };
            let placed = call.args.len() == kind.arm_count() * 2 + 2
                && call.args[2..].chunks_exact(2).all(|pair| {
                    matches!((&pair[0], &pair[1]),
                        (Argument::Identifier(side), Argument::Identifier(name))
                        if port_direction_from_name(side).is_some() &&
                             points.contains_key(name.as_str()))
                });
            if !placed && (call.args.len() < 2 || call.args.len() % 2 != 0) {
                return None;
            }
            let point = points.get(point_ref.as_str())?;
            let arms = arms_by_point
                .get(point_ref.as_str())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if !map_points.contains(point_ref.as_str())
                || (!placed && arms.len() != kind.arm_count())
                || (!placed
                    && kind == IntersectionKind::RightTurn
                    && !right_angle_arms(projection, arms))
            {
                return None;
            }
            let road_name = match call.args.get(1) {
                Some(Argument::Identifier(name)) => name.clone(),
                None => projection.segments.get(arms.first()?.0)?.road_name.clone(),
                _ => return None,
            };
            if !roads.contains(road_name.as_str()) {
                return None;
            }
            let arm_roads = if placed {
                Vec::new()
            } else {
                let mut mapped = Vec::new();
                for pair in call.args[2..].chunks_exact(2) {
                    let (Argument::Identifier(side), Argument::Identifier(name)) =
                        (&pair[0], &pair[1])
                    else {
                        return None;
                    };

                    let direction = port_direction_from_name(side)?;
                    if !roads.contains(name.as_str())
                        || mapped
                            .iter()
                            .any(|(existing, _): &(WorldPoint, String)| *existing == direction)
                    {
                        return None;
                    }
                    mapped.push((direction, name.clone()));
                }
                mapped
            };
            let ports = if placed {
                call.args[2..]
                    .chunks_exact(2)
                    .map(|pair| {
                        let (Argument::Identifier(side), Argument::Identifier(name)) =
                            (&pair[0], &pair[1])
                        else {
                            return None;
                        };
                        let direction = port_direction_from_name(side)?;
                        let port = points.get(name.as_str())?;
                        Some(IntersectionPort {
                            point_ref: name.clone(),
                            position: port.position,
                            direction,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?
            } else {
                Vec::new()
            };
            Some(Intersection {
                footprint_half: 0.,
                point_ref: point_ref.clone(),
                position: point.position,
                kind,
                road_name,
                arm_roads,
                ports,
            })
        })
        .collect();
    super::intersection_footprints::assign(program, projection, &mut result);
    result
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
    let road_half = road
        .elements
        .iter()
        .map(|element| element_world_width(element.kind))
        .sum::<f32>()
        * 0.5;
    let half = if matches!(
        kind,
        IntersectionKind::FourWay | IntersectionKind::RightTurn
    ) {
        road_half * opencity::network::default_four_way_footprint_factor()
    } else {
        road_half
    };
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
    program
        .statements
        .iter()
        .find_map(|statement| {
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
        .or_else(|| {
            opencity::junction_layouts::port_direction(program, point_ref)
                .map(|[x, y]| WorldPoint { x, y })
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

fn layout_placement(program: &Program, center_ref: &str) -> Option<usize> {
    program
        .statements
        .iter()
        .enumerate()
        .find_map(|(index, statement)| {
            let Statement::Call(call) = statement else {
                return None;
            };
            let instance = call.assigns.first().unwrap_or(&call.name);
            if !center_ref
                .strip_prefix(instance)
                .is_some_and(|suffix| suffix.starts_with('.'))
            {
                return None;
            }
            program.statements.iter().find_map(|s| {
                let Statement::Network(d) = s else {
                    return None;
                };
                if d.name.as_deref() != Some(&call.name) {
                    return None;
                }
                let template = d.template.as_ref()?;
                let opencity::network::ComponentTemplateBody::Junction(calls) = &template.body
                else {
                    return None;
                };
                let center = calls
                    .iter()
                    .find(|c| IntersectionKind::from_call_name(&c.name).is_some())?
                    .args
                    .first()?;
                let Argument::Identifier(center) = center else {
                    return None;
                };
                (format!("{}.{}", call.assigns.first().unwrap_or(&call.name), center) == center_ref)
                    .then_some(index)
            })
        })
}

pub fn translate_intersection(program: &mut Program, point_ref: &str, offset: WorldPoint) -> bool {
    let placement = layout_placement(program, point_ref);
    if placement.is_none()
        && !program.statements.iter().any(|s| {
            matches!(s,
         Statement::Call(c) if c.name == "map_point" &&
         c.assigns.first().is_some_and(|name| name == point_ref))
        })
    {
        return false;
    }

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
    if let Some(index) = placement {
        let Statement::Call(call) = &mut program.statements[index] else {
            unreachable!()
        };
        let point = point_args(call, 0).unwrap();
        call.args[0] = number_arg(point.x + offset.x);
        call.args[1] = number_arg(point.y + offset.y);
        return true;
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
    let placement = layout_placement(program, point_ref);
    if placement.is_none()
        && !program.statements.iter().any(|s| {
            matches!(s,
         Statement::Call(c) if c.name == "map_point" &&
         c.assigns.first().is_some_and(|name| name == point_ref))
        })
    {
        return false;
    }

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
                && matches!(call.args.first(),
                     Some(Argument::Identifier(name)) if name == point_ref)
            {
                for pair in call.args[2..].chunks_exact_mut(2) {
                    if matches!(&pair[1], Argument::Identifier(name) if name == &port.point_ref) {
                        pair[0] = Argument::Identifier(port_direction_name(direction).into());
                    }
                }
            }
        }
    }
    if let Some(index) = placement {
        let Statement::Call(call) = &mut program.statements[index] else {
            unreachable!()
        };
        let origin = point_args(call, 0).unwrap();
        let relative = [
            origin.x - intersection.position.x,
            origin.y - intersection.position.y,
        ];
        let [x, y] = opencity::blocks::rotate_vector(relative, -90.);
        call.args[0] = number_arg(intersection.position.x + x);
        call.args[1] = number_arg(intersection.position.y + y);
        let placement_len = call.args.len() - 1;
        if placement_len == 2 {
            call.args.insert(2, number_arg(0.));
        }
        if call.args.len() == 4 {
            call.args.insert(3, number_arg(0.));
        }
        let rotation = number(&call.args[3]).unwrap();
        call.args[3] = number_arg(rotation - 90.);
    }
    true
}

pub fn remove_placed_intersection(program: &mut Program, point_ref: &str) -> bool {
    let placement = layout_placement(program, point_ref);
    if placement.is_none()
        && !program.statements.iter().any(|s| {
            matches!(s,
         Statement::Call(c) if c.name == "map_point" &&
         c.assigns.first().is_some_and(|name| name == point_ref))
        })
    {
        return false;
    }

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
    if let Some(index) = placement {
        if unused.len() != intersection.ports.len() {
            return false;
        }
        program.statements.remove(index);
        return true;
    }
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
    if !program.statements.iter().any(|statement| {
        matches!(statement,
        Statement::Call(call) if call.name == "map_point" &&
             call.assigns.first().is_some_and(|name| name == point_ref))
    }) {
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
