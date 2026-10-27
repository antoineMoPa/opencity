use super::*;
use crate::{Argument, Call, Program, Statement};

#[derive(Clone, Debug, PartialEq)]
pub struct NetworkInstance {
    pub id: String,
    pub name: String,
    pub definition: usize,
    pub origin: Point,
    pub rotation: f32,
    pub geometry: Resolved,
}
impl NetworkInstance {
    /// The proposal uses local +Y south; the existing city uses world +Y north.
    pub fn world_point(&self, point: Point) -> Point {
        let [x, y] = crate::blocks::rotate_vector([point.x, -point.y], self.rotation);
        Point {
            x: self.origin.x + x,
            y: self.origin.y + y,
            z: self.origin.z + point.z,
        }
    }
    pub fn world_direction(&self, heading: Heading) -> [f32; 2] {
        let [x, y] = heading.vector();
        crate::blocks::rotate_vector([x, -y], self.rotation)
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetworkProjection {
    pub instances: Vec<NetworkInstance>,
    pub errors: Vec<String>,
}

fn instantiate<'a>(
    definition: &Definition,
    args: &'a [Argument],
) -> Result<(Resolved, crate::blocks::Placement<'a>), String> {
    let bound = crate::blocks::bind_placement(
        definition
            .template
            .as_ref()
            .map(|t| t.parameters.as_slice()),
        args,
        crate::blocks::PlacementKind::Component,
        definition.name.as_deref().unwrap_or_default(),
    )?;
    let geometry = if let Some(ComponentTemplate {
        body: ComponentTemplateBody::Network(body),
        ..
    }) = &definition.template
    {
        let source = format!(
            "{} {} {{ {} }}",
            definition.kind.keyword(),
            definition.name.as_deref().unwrap_or_default(),
            body
        );
        let concrete = crate::parser::parse_bound_component(&source, bound.bindings.to_vec())
            .map_err(|e| e.to_string())?;
        resolve(&concrete)?
    } else {
        resolve(definition)?
    };
    Ok((geometry, bound))
}
fn visit(
    program: &Program,
    call: &Call,
    calls: &[&Call],
    origin: Point,
    rotation: f32,
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
        if matches!(
            &definition.template,
            Some(ComponentTemplate {
                body: ComponentTemplateBody::Junction(_),
                ..
            })
        ) {
            return;
        }
        let built: Result<NetworkInstance, String> = (|| {
            let (geometry, bound) = instantiate(definition, &call.args)?;
            let [x, y] = crate::blocks::rotate_vector(bound.origin, rotation);
            let named = call
                .assigns
                .first()
                .and_then(|name| crate::elevation_in_calls(calls, name))
                .unwrap_or(0.);
            let origin = Point {
                x: origin.x + x,
                y: origin.y + y,
                z: origin.z + bound.elevation + named,
            };
            if ![origin.x, origin.y, origin.z, rotation + bound.rotation]
                .iter()
                .all(|n| n.is_finite())
            {
                return Err("component placement overflow".into());
            }
            Ok(NetworkInstance {
                id,
                name: qualified.clone(),
                definition: index,
                origin,
                rotation: rotation + bound.rotation,
                geometry,
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
        let [x, y] = crate::blocks::rotate_vector(bound.origin, rotation);
        let next = Point {
            x: origin.x + x,
            y: origin.y + y,
            z: origin.z + bound.elevation + named,
        };
        if ![next.x, next.y, next.z, rotation + bound.rotation]
            .iter()
            .all(|n| n.is_finite())
        {
            result
                .errors
                .push(format!("{}: block placement overflow", block.name));
            return;
        }
        let calls: Vec<_> = bound.calls.iter().collect();
        stack.push(block.name.clone());
        for (i, inner) in bound.calls.iter().enumerate() {
            visit(
                program,
                inner,
                &calls,
                next,
                rotation + bound.rotation,
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
                        rotation: 0.,
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
                0.,
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
