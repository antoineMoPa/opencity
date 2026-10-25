use super::*;
use crate::{Argument, Call, Program, Statement};

#[derive(Clone, Debug, PartialEq)]
pub struct NetworkInstance {
    pub id: String,
    pub name: String,
    pub definition: usize,
    pub origin: Point,
    pub geometry: Resolved,
}
impl NetworkInstance {
    /// The proposal uses local +Y south; the existing city uses world +Y north.
    pub fn world_point(&self, point: Point) -> Point {
        Point {
            x: self.origin.x + point.x,
            y: self.origin.y - point.y,
            z: self.origin.z + point.z,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetworkProjection {
    pub instances: Vec<NetworkInstance>,
    pub errors: Vec<String>,
}

fn number(argument: &Argument) -> Option<f32> {
    match argument {
        Argument::Number(text) => text.parse::<f32>().ok().filter(|n| n.is_finite()),
        _ => None,
    }
}
fn placement(args: &[Argument]) -> Result<Point, String> {
    if !matches!(args.len(), 2 | 3) {
        return Err("component placement needs x, y, and optional elevation".into());
    }
    Ok(Point {
        x: number(&args[0]).ok_or("component x must be finite")?,
        y: number(&args[1]).ok_or("component y must be finite")?,
        z: if args.len() == 3 {
            number(&args[2]).ok_or("component elevation must be finite")?
        } else {
            0.
        },
    })
}
fn visit(
    program: &Program,
    call: &Call,
    calls: &[&Call],
    origin: Point,
    id: String,
    namespace: &str,
    stack: &mut Vec<String>,
    result: &mut NetworkProjection,
) {
    let local_name = call.assigns.first().unwrap_or(&call.name);
    let qualified = if namespace.is_empty() {
        local_name.clone()
    } else {
        format!("{namespace}.{local_name}")
    };
    if let Some((index, definition)) =
        program
            .statements
            .iter()
            .enumerate()
            .find_map(|(i, s)| match s {
                Statement::Network(d)
                    if d.origin.is_none() && d.name.as_deref() == Some(&call.name) =>
                {
                    Some((i, d))
                }
                _ => None,
            })
    {
        let built: Result<NetworkInstance, String> = (|| {
            let local = placement(&call.args)?;
            let named = call
                .assigns
                .first()
                .and_then(|name| crate::elevation_in_calls(calls, name))
                .unwrap_or(0.);
            let origin = Point {
                x: origin.x + local.x,
                y: origin.y + local.y,
                z: origin.z + local.z + named,
            };
            if ![origin.x, origin.y, origin.z].iter().all(|n| n.is_finite()) {
                return Err("component placement overflow".into());
            }
            Ok(NetworkInstance {
                id,
                name: qualified.clone(),
                definition: index,
                origin,
                geometry: resolve(definition)?,
            })
        })();
        match built {
            Ok(instance) => result.instances.push(instance),
            Err(error) => result.errors.push(format!("{}: {error}", call.name)),
        }
    } else if let Some(block) = program.statements.iter().find_map(|s| match s {
        Statement::Block(b) if b.name == call.name => Some(b),
        _ => None,
    }) {
        if stack.contains(&block.name) {
            result
                .errors
                .push(format!("recursive block '{}'", block.name));
            return;
        }
        let bound = match block.instantiate(&call.args) {
            Ok(bound) => bound,
            Err(error) => {
                result.errors.push(error);
                return;
            }
        };
        let named = call
            .assigns
            .first()
            .and_then(|name| crate::elevation_in_calls(calls, name))
            .unwrap_or(0.);
        let next = Point {
            x: origin.x + bound.origin[0],
            y: origin.y + bound.origin[1],
            z: origin.z + bound.elevation + named,
        };
        let calls: Vec<_> = bound.calls.iter().collect();
        stack.push(block.name.clone());
        for (i, inner) in bound.calls.iter().enumerate() {
            visit(
                program,
                inner,
                &calls,
                next,
                format!("{id}/{i}"),
                &qualified,
                stack,
                result,
            );
        }
        stack.pop();
    }
}
pub fn project(program: &Program) -> NetworkProjection {
    let mut result = NetworkProjection::default();
    if !program
        .statements
        .iter()
        .any(|s| matches!(s, Statement::Network(_)))
    {
        return result;
    }
    let calls: Vec<_> = program
        .statements
        .iter()
        .filter_map(|s| match s {
            Statement::Call(c) => Some(c),
            _ => None,
        })
        .collect();
    for (index, statement) in program.statements.iter().enumerate() {
        match statement {
            Statement::Network(definition) if definition.origin.is_some() => {
                match resolve(definition) {
                    Ok(geometry) => result.instances.push(NetworkInstance {
                        id: format!("network:{index}"),
                        name: definition
                            .name
                            .clone()
                            .unwrap_or_else(|| format!("{}_{index}", definition.kind.keyword())),
                        definition: index,
                        origin: definition.origin.unwrap(),
                        geometry,
                    }),
                    Err(error) => result.errors.push(error),
                }
            }
            Statement::Call(call) => visit(
                program,
                call,
                &calls,
                Point {
                    x: 0.,
                    y: 0.,
                    z: 0.,
                },
                format!("network:{index}"),
                "",
                &mut vec![],
                &mut result,
            ),
            _ => {}
        }
    }
    result
}
