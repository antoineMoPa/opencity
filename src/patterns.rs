//! Built-in `.road` patterns. `place` and `join` are compiled into ordinary
//! road calls while parsing, so projection, rendering, and GLB export share
//! the same geometry and named references as hand-written maps.

use std::collections::{HashMap, HashSet};

use crate::{Argument, Call, Program, RoadDirection, RoadElement, Span, Statement, SyntaxError};

const SIDES: [&str; 4] = ["north", "west", "south", "east"];

fn failure(message: impl Into<String>) -> SyntaxError {
    SyntaxError {
        span: Span {
            offset: 0,
            line: 1,
            column: 1,
        },
        message: message.into(),
    }
}

fn built_in(name: &str) -> Option<&'static str> {
    match name {
        "four_way" => Some(include_str!("../road_library/four_way.road")),
        "three_way" => Some(include_str!("../road_library/three_way.road")),
        "street_to_interstate_flat" => Some(include_str!(
            "../road_library/street_to_interstate_flat.road"
        )),
        "street_to_interstate_elevated" => Some(include_str!(
            "../road_library/street_to_interstate_elevated.road"
        )),
        "cloverleaf" => Some(include_str!("../road_library/cloverleaf.road")),
        _ => None,
    }
}

fn identifier(argument: &Argument) -> Option<&str> {
    match argument {
        Argument::Identifier(value) => Some(value),
        _ => None,
    }
}

fn number(argument: &Argument) -> Option<f32> {
    match argument {
        Argument::Number(value) => value.parse::<f32>().ok().filter(|value| value.is_finite()),
        _ => None,
    }
}

fn number_arg(value: f32) -> Argument {
    Argument::Number(format!("{value:.4}"))
}

fn rotated(x: f32, y: f32, turns: usize) -> (f32, f32) {
    match turns {
        0 => (x, y),
        1 => (-y, x),
        2 => (-x, -y),
        3 => (y, -x),
        _ => unreachable!(),
    }
}

#[derive(Clone)]
struct Placement {
    id: String,
    x: f32,
    y: f32,
    turns: usize,
    template: Program,
}

impl Placement {
    fn from_call(call: &Call) -> Result<Self, SyntaxError> {
        let [asset, id, x, y, turns] = call.args.as_slice() else {
            return Err(failure(
                "place(asset, id, x, y, quarter_turns) requires five arguments",
            ));
        };
        let asset = identifier(asset).ok_or_else(|| failure("place asset must be a name"))?;
        let id = identifier(id).ok_or_else(|| failure("place id must be a name"))?;
        let x = number(x).ok_or_else(|| failure("place x must be finite"))?;
        let y = number(y).ok_or_else(|| failure("place y must be finite"))?;
        let turns = number(turns).ok_or_else(|| failure("quarter_turns must be 0, 1, 2, or 3"))?;
        if !(0.0..=3.0).contains(&turns) || turns.fract() != 0.0 {
            return Err(failure("quarter_turns must be 0, 1, 2, or 3"));
        }
        let source =
            built_in(asset).ok_or_else(|| failure(format!("unknown road pattern '{asset}'")))?;
        Ok(Self {
            id: id.to_owned(),
            x,
            y,
            turns: turns as usize,
            template: crate::parser::parse_raw(source)?,
        })
    }

    fn point(&self, name: &str) -> Option<(f32, f32)> {
        self.template.statements.iter().find_map(|statement| {
            let Statement::Call(call) = statement else {
                return None;
            };
            if call.name != "map_point" || call.assigns.first().map(String::as_str) != Some(name) {
                return None;
            }
            let (x, y) = rotated(
                number(call.args.first()?)?,
                number(call.args.get(1)?)?,
                self.turns,
            );
            Some((x + self.x, y + self.y))
        })
    }

    fn port(&self, name: &str) -> Result<Port, SyntaxError> {
        let point = self
            .point(name)
            .ok_or_else(|| failure(format!("{}.{} is not a map_point", self.id, name)))?;
        let roads: HashMap<_, _> = self
            .template
            .statements
            .iter()
            .filter_map(|statement| match statement {
                Statement::Road(road) => Some((road.name.as_str(), road)),
                _ => None,
            })
            .collect();
        let mut found = None;
        for statement in &self.template.statements {
            let Statement::Call(call) = statement else {
                continue;
            };
            let Some(road) = roads.get(call.name.as_str()) else {
                continue;
            };
            if call.args.len() != 6 {
                continue;
            }
            let end = if call.args.first().and_then(identifier) == Some(name) {
                Some(false)
            } else if call.args.last().and_then(identifier) == Some(name) {
                Some(true)
            } else {
                None
            };
            let Some(end) = end else { continue };
            if found.is_some() {
                return Err(failure(format!(
                    "{}.{} is used by more than one road",
                    self.id, name
                )));
            }
            let index = if end { 3 } else { 1 };
            let (cx, cy) = rotated(
                number(&call.args[index]).ok_or_else(|| failure("invalid port control x"))?,
                number(&call.args[index + 1]).ok_or_else(|| failure("invalid port control y"))?,
                self.turns,
            );
            let inward = (cx + self.x - point.0, cy + self.y - point.1);
            let (start_z, end_z) =
                height_of(&self.template, call.assigns.first().map(String::as_str));
            let travel = match (road.direction, end) {
                (RoadDirection::Both, _) => Travel::Both,
                (RoadDirection::Forward, false) | (RoadDirection::Reverse, true) => Travel::Out,
                _ => Travel::In,
            };
            found = Some(Port {
                point,
                elements: road.elements.clone(),
                height: if end { end_z } else { start_z },
                inward,
                travel,
            });
        }
        found.ok_or_else(|| {
            failure(format!(
                "{}.{} is not one road's outer endpoint",
                self.id, name
            ))
        })
    }
}

fn height_of(program: &Program, name: Option<&str>) -> (f32, f32) {
    let Some(name) = name else { return (0.0, 0.0) };
    let mut height = (0.0, 0.0);
    for statement in &program.statements {
        let Statement::Call(call) = statement else {
            continue;
        };
        if call.args.first().and_then(identifier) != Some(name) {
            continue;
        }
        match (call.name.as_str(), call.args.as_slice()) {
            ("elevation", [_, z]) => {
                if let Some(z) = number(z) {
                    height = (z, z);
                }
            }
            ("grade", [_, start, end]) => {
                if let (Some(start), Some(end)) = (number(start), number(end)) {
                    height = (start, end);
                }
            }
            _ => {}
        }
    }
    height
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Travel {
    Both,
    In,
    Out,
}

struct Port {
    point: (f32, f32),
    elements: Vec<RoadElement>,
    height: f32,
    inward: (f32, f32),
    travel: Travel,
}

fn check_join(a: &Placement, a_name: &str, b: &Placement, b_name: &str) -> Result<(), SyntaxError> {
    let a_port = a.port(a_name)?;
    let b_port = b.port(b_name)?;
    if a_port.elements != b_port.elements {
        return Err(failure(format!(
            "{}.{} and {}.{} have different cross sections",
            a.id, a_name, b.id, b_name
        )));
    }
    if a_port.travel == b_port.travel && a_port.travel != Travel::Both {
        return Err(failure(
            "joined one-way roads point traffic the same way at the seam",
        ));
    }
    if (a_port.point.0 - b_port.point.0).abs() > 0.00001
        || (a_port.point.1 - b_port.point.1).abs() > 0.00001
    {
        return Err(failure("joined road pattern ports do not meet"));
    }
    if (a_port.height - b_port.height).abs() > 0.00001 {
        return Err(failure("joined road pattern ports have different heights"));
    }
    let dot = a_port.inward.0 * b_port.inward.0 + a_port.inward.1 * b_port.inward.1;
    let al = a_port.inward.0.hypot(a_port.inward.1);
    let bl = b_port.inward.0.hypot(b_port.inward.1);
    if al * bl < 0.000001 || dot / (al * bl) > -0.95 {
        return Err(failure("joined road pattern ports have a sharp seam"));
    }
    Ok(())
}

fn placed_statements(
    placement: &Placement,
    aliases: &HashMap<String, String>,
) -> Result<Vec<Statement>, SyntaxError> {
    let prefix = format!("{}_", placement.id);
    let names: HashSet<_> = placement
        .template
        .statements
        .iter()
        .flat_map(|statement| match statement {
            Statement::Road(road) => vec![road.name.as_str()],
            Statement::Call(call) => call.assigns.iter().map(String::as_str).collect(),
            Statement::Block(_) => Vec::new(),
        })
        .collect();
    let roads: HashSet<_> = placement
        .template
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Road(road) => Some(road.name.as_str()),
            _ => None,
        })
        .collect();
    let mapped = |name: &str| {
        aliases
            .get(&format!("{prefix}{name}"))
            .cloned()
            .unwrap_or_else(|| format!("{prefix}{name}"))
    };
    let mut result = Vec::new();
    for statement in &placement.template.statements {
        match statement {
            Statement::Road(road) => {
                let mut road = road.clone();
                road.name = mapped(&road.name);
                result.push(Statement::Road(road));
            }
            Statement::Block(_) => return Err(failure("road patterns cannot contain blocks")),
            Statement::Call(call) => {
                if call.name == "map_point"
                    && call
                        .assigns
                        .first()
                        .is_some_and(|name| aliases.contains_key(&format!("{prefix}{name}")))
                {
                    continue;
                }
                let mut call = call.clone();
                if roads.contains(call.name.as_str()) {
                    if call.args.len() != 6 {
                        return Err(failure("pattern roads require six arguments"));
                    }
                    for index in [1, 3] {
                        let x = number(&call.args[index])
                            .ok_or_else(|| failure("invalid pattern control x"))?;
                        let y = number(&call.args[index + 1])
                            .ok_or_else(|| failure("invalid pattern control y"))?;
                        let (x, y) = rotated(x, y, placement.turns);
                        call.args[index] = number_arg(x + placement.x);
                        call.args[index + 1] = number_arg(y + placement.y);
                    }
                    call.name = mapped(&call.name);
                } else if call.name == "map_point" {
                    let x =
                        number(&call.args[0]).ok_or_else(|| failure("invalid pattern point x"))?;
                    let y =
                        number(&call.args[1]).ok_or_else(|| failure("invalid pattern point y"))?;
                    let (x, y) = rotated(x, y, placement.turns);
                    call.args[0] = number_arg(x + placement.x);
                    call.args[1] = number_arg(y + placement.y);
                } else if call.name.starts_with("intersection") {
                    for index in (2..call.args.len()).step_by(2) {
                        let side = identifier(&call.args[index])
                            .ok_or_else(|| failure("invalid intersection side"))?;
                        let side_index = SIDES
                            .iter()
                            .position(|candidate| *candidate == side)
                            .ok_or_else(|| failure("invalid intersection side"))?;
                        call.args[index] =
                            Argument::Identifier(SIDES[(side_index + placement.turns) % 4].into());
                    }
                }
                call.assigns = call.assigns.iter().map(|name| mapped(name)).collect();
                for arg in &mut call.args {
                    if let Argument::Identifier(name) = arg {
                        if names.contains(name.as_str()) {
                            *name = mapped(name);
                        }
                    }
                }
                result.push(Statement::Call(call));
            }
        }
    }
    Ok(result)
}

pub(crate) fn expand(program: Program) -> Result<Program, SyntaxError> {
    let mut placements = HashMap::new();
    for statement in &program.statements {
        if let Statement::Block(block) = statement {
            if block
                .calls
                .iter()
                .any(|call| matches!(call.name.as_str(), "place" | "join"))
            {
                return Err(failure("place and join must be top-level calls"));
            }
        }
        let Statement::Call(call) = statement else {
            continue;
        };
        if call.name != "place" {
            continue;
        }
        let placement = Placement::from_call(call)?;
        if placements.insert(placement.id.clone(), placement).is_some() {
            return Err(failure("road pattern placement ids must be unique"));
        }
    }
    if placements.is_empty() {
        if program
            .statements
            .iter()
            .any(|statement| matches!(statement, Statement::Call(call) if call.name == "join"))
        {
            return Err(failure("join requires placed road patterns"));
        }
        return Ok(program);
    }
    let mut aliases = HashMap::new();
    let mut joined = HashSet::new();
    for statement in &program.statements {
        let Statement::Call(call) = statement else {
            continue;
        };
        if call.name != "join" {
            continue;
        }
        let [a_id, a_port, b_id, b_port] = call.args.as_slice() else {
            return Err(failure(
                "join(a_id, a_port, b_id, b_port) requires four names",
            ));
        };
        let (a_id, a_port, b_id, b_port) = (
            identifier(a_id).ok_or_else(|| failure("join requires names"))?,
            identifier(a_port).ok_or_else(|| failure("join requires names"))?,
            identifier(b_id).ok_or_else(|| failure("join requires names"))?,
            identifier(b_port).ok_or_else(|| failure("join requires names"))?,
        );
        let a = placements
            .get(a_id)
            .ok_or_else(|| failure(format!("unknown pattern placement '{a_id}'")))?;
        let b = placements
            .get(b_id)
            .ok_or_else(|| failure(format!("unknown pattern placement '{b_id}'")))?;
        let target = format!("{a_id}_{a_port}");
        let source = format!("{b_id}_{b_port}");
        if target == source || !joined.insert(target.clone()) || !joined.insert(source.clone()) {
            return Err(failure("a road pattern port can be joined only once"));
        }
        check_join(a, a_port, b, b_port)?;
        aliases.insert(source, target);
    }
    let mut output = Vec::new();
    for statement in program.statements {
        match statement {
            Statement::Call(call) if call.name == "place" => {
                let id = call
                    .args
                    .get(1)
                    .and_then(identifier)
                    .ok_or_else(|| failure("place id must be a name"))?;
                output.extend(placed_statements(placements.get(id).unwrap(), &aliases)?);
            }
            Statement::Call(call) if call.name == "join" => {}
            other => output.push(other),
        }
    }
    let mut used = HashSet::new();
    for statement in &output {
        match statement {
            Statement::Road(road) if !used.insert(road.name.as_str()) => {
                return Err(failure(format!(
                    "duplicate name '{}' after placing patterns",
                    road.name
                )));
            }
            Statement::Call(call) => {
                for name in &call.assigns {
                    if !used.insert(name) {
                        return Err(failure(format!(
                            "duplicate name '{name}' after placing patterns"
                        )));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(Program { statements: output })
}
