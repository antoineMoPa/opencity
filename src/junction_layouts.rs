//! Reusable road-profile junction layouts, projected without rewriting source.
use crate::{
    blocks::rotate_vector,
    network::{ComponentTemplateBody, Definition},
    Argument, BlockDefinition, Call, Program, Statement,
};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

pub(crate) fn validate(calls: &[Call]) -> Result<(), String> {
    let points: HashSet<_> = calls
        .iter()
        .filter(|c| c.name == "map_point")
        .flat_map(|c| c.assigns.iter().map(String::as_str))
        .collect();
    let mut junctions = 0;
    let mut assigned = HashSet::new();
    for call in calls {
        for name in &call.assigns {
            if !assigned.insert(name) {
                return Err(format!("duplicate junction layout point '{name}'"));
            }
        }
        match call.name.as_str() {
            "map_point" if call.assigns.len() == 1 && call.args.len() == 2 => {}
            "intersection2" | "intersection3" | "intersection4" => {
                junctions += 1;
                let count = match call.name.as_str() {
                    "intersection2" => 2,
                    "intersection3" => 3,
                    _ => 4,
                };
                if !call.assigns.is_empty() || call.args.len() != count * 2 + 2 {
                    return Err(
                        "junction layout needs a center, road profile, and side/point pairs".into(),
                    );
                }
                let point = |value: &Argument| matches!(value, Argument::Identifier(name) if points.contains(name.as_str()));
                let mut sides = HashSet::new();
                if !point(&call.args[0]) || !matches!(call.args[1], Argument::Identifier(_)) || !call.args[2..].chunks_exact(2).all(|pair| {
                    matches!(&pair[0], Argument::Identifier(side) if matches!(side.as_str(), "north" | "east" | "south" | "west") && sides.insert(side)) && point(&pair[1])
                }) { return Err("junction layout needs local center/port points and distinct cardinal sides".into()); }
            }
            _ => {
                return Err(
                    "junction layouts contain only map_point and one intersection2/3/4 call".into(),
                )
            }
        }
    }
    if junctions != 1 {
        return Err("junction layout must contain exactly one intersection2/3/4 call".into());
    }
    Ok(())
}
fn number(value: &Argument) -> Result<f32, String> {
    match value {
        Argument::Number(text) => text.parse::<f32>().ok().filter(|v| v.is_finite()),
        _ => None,
    }
    .ok_or_else(|| "junction coordinates must be finite numbers".into())
}
fn side(name: &str, rotation: f32) -> String {
    let vector = match name {
        "north" => [0., 1.],
        "east" => [1., 0.],
        "south" => [0., -1.],
        "west" => [-1., 0.],
        _ => unreachable!(),
    };
    let [x, y] = rotate_vector(vector, rotation);
    if x > 0.5 {
        "east"
    } else if x < -0.5 {
        "west"
    } else if y > 0.5 {
        "north"
    } else {
        "south"
    }
    .into()
}
#[derive(Clone, Copy, Default)]
struct Transform {
    origin: [f32; 2],
    elevation: f32,
    rotation: f32,
}
fn layout(definition: &Definition) -> Option<BlockDefinition> {
    let template = definition.template.as_ref()?;
    let ComponentTemplateBody::Junction(calls) = &template.body else {
        return None;
    };
    Some(BlockDefinition {
        name: definition.name.clone()?,
        parameters: Some(template.parameters.clone()),
        calls: calls.clone(),
    })
}
struct Definitions<'a> {
    junctions: HashMap<&'a str, BlockDefinition>,
    blocks: HashMap<&'a str, &'a BlockDefinition>,
}
impl<'a> Definitions<'a> {
    fn new(program: &'a Program) -> Self {
        let mut definitions = Self {
            junctions: HashMap::new(),
            blocks: HashMap::new(),
        };
        for statement in &program.statements {
            match statement {
                Statement::Network(d) => {
                    if let (Some(name), Some(block)) = (d.name.as_deref(), layout(d)) {
                        definitions.junctions.insert(name, block);
                    }
                }
                Statement::Block(b) => {
                    definitions.blocks.insert(&b.name, b);
                }
                _ => {}
            }
        }
        definitions
    }
}

fn visit(
    definitions: &Definitions<'_>,
    call: &Call,
    siblings: &[&Call],
    parent: Transform,
    namespace: &str,
    stack: &mut Vec<String>,
    output: &mut Vec<Call>,
) -> Result<(), String> {
    let junction = definitions.junctions.get(call.name.as_str());
    let block = junction.or_else(|| definitions.blocks.get(call.name.as_str()).copied());
    let Some(block) = block else {
        return Ok(());
    };
    if stack.contains(&block.name) {
        return Err(format!("recursive junction block '{}'", block.name));
    }
    let bound = match block.instantiate(&call.args) {
        Ok(bound) => bound,
        Err(error) if junction.is_some() => return Err(error),
        Err(_) => return Ok(()),
    };
    let local = rotate_vector(bound.origin, parent.rotation);
    let transform = Transform {
        origin: [parent.origin[0] + local[0], parent.origin[1] + local[1]],
        elevation: parent.elevation
            + bound.elevation
            + call
                .assigns
                .first()
                .and_then(|name| crate::elevation_in_calls(siblings, name))
                .unwrap_or(0.),
        rotation: parent.rotation + bound.rotation,
    };
    if ![
        transform.origin[0],
        transform.origin[1],
        transform.elevation,
        transform.rotation,
    ]
    .iter()
    .all(|v| v.is_finite())
    {
        return Err("junction placement overflow".into());
    }
    let local_name = call.assigns.first().unwrap_or(&call.name);
    let namespace = if namespace.is_empty() {
        local_name.clone()
    } else {
        format!("{namespace}.{local_name}")
    };
    if junction.is_some() {
        if (transform.rotation / 90.).fract() != 0. {
            return Err("road-profile junction rotation must be a multiple of 90 degrees".into());
        }
        validate(&bound.calls)?;
        let mut generated = vec![];
        for mut inner in bound.calls {
            if inner.name == "map_point" {
                let local = rotate_vector(
                    [number(&inner.args[0])?, number(&inner.args[1])?],
                    transform.rotation,
                );
                let point = [
                    local[0] + transform.origin[0],
                    local[1] + transform.origin[1],
                ];
                if !point.iter().all(|v| v.is_finite()) {
                    return Err("junction coordinates overflow".into());
                }
                inner.args = point.map(|v| Argument::Number(v.to_string())).to_vec();
            } else {
                for pair in inner.args[2..].chunks_exact_mut(2) {
                    let Argument::Identifier(name) = &pair[0] else {
                        unreachable!()
                    };
                    pair[0] = Argument::Identifier(side(name, transform.rotation));
                }
            }
            if inner.name != "map_point" {
                for (index, arg) in inner.args.iter_mut().enumerate() {
                    if index == 0 || (index >= 3 && index % 2 == 1) {
                        let Argument::Identifier(name) = arg else {
                            unreachable!()
                        };
                        *name = format!("{namespace}.{name}");
                    }
                }
            }
            inner.assigns = inner
                .assigns
                .iter()
                .map(|name| format!("{namespace}.{name}"))
                .collect();
            if inner.name == "map_point" && transform.elevation != 0. {
                generated.push(Call {
                    assigns: vec![],
                    name: "elevation".into(),
                    args: vec![
                        Argument::Identifier(inner.assigns[0].clone()),
                        Argument::Number(transform.elevation.to_string()),
                    ],
                });
            }
            generated.push(inner);
        }
        output.extend(generated);
    } else {
        stack.push(block.name.clone());
        let siblings: Vec<_> = bound.calls.iter().collect();
        for inner in &bound.calls {
            visit(
                definitions,
                inner,
                &siblings,
                transform,
                &namespace,
                stack,
                output,
            )?;
        }
        stack.pop();
    }
    Ok(())
}
/// Append derived metadata while retaining source statement indices and definitions.
pub fn expand(program: &Program) -> (Cow<'_, Program>, Vec<String>) {
    let definitions = Definitions::new(program);
    if definitions.junctions.is_empty() {
        return (Cow::Borrowed(program), vec![]);
    }
    let calls: Vec<_> = program
        .statements
        .iter()
        .filter_map(|s| match s {
            Statement::Call(c) => Some(c),
            _ => None,
        })
        .collect();
    let mut output = vec![];
    let mut errors = vec![];
    for call in &calls {
        if let Err(error) = visit(
            &definitions,
            call,
            &calls,
            Transform::default(),
            "",
            &mut vec![],
            &mut output,
        ) {
            errors.push(format!("{}: {error}", call.name));
        }
    }
    let mut expanded = program.clone();
    expanded
        .statements
        .extend(output.into_iter().map(Statement::Call));
    (Cow::Owned(expanded), errors)
}

/// Resolve only the owning placement when a road requests a layout port's side.
pub fn port_direction(program: &Program, reference: &str) -> Option<[f32; 2]> {
    let definitions = Definitions::new(program);
    if definitions.junctions.is_empty() {
        return None;
    }
    let calls: Vec<_> = program
        .statements
        .iter()
        .filter_map(|s| match s {
            Statement::Call(c) => Some(c),
            _ => None,
        })
        .collect();
    let call = calls.iter().find(|c| {
        reference
            .strip_prefix(c.assigns.first().unwrap_or(&c.name))
            .is_some_and(|suffix| suffix.starts_with('.'))
    })?;
    let mut output = vec![];
    visit(
        &definitions,
        call,
        &calls,
        Transform::default(),
        "",
        &mut vec![],
        &mut output,
    )
    .ok()?;
    output
        .iter()
        .filter(|c| c.name != "map_point" && c.name != "elevation")
        .find_map(|c| {
            c.args[2..]
                .chunks_exact(2)
                .find_map(|pair| match (&pair[0], &pair[1]) {
                    (Argument::Identifier(side), Argument::Identifier(name))
                        if name == reference =>
                    {
                        match side.as_str() {
                            "north" => Some([0., 1.]),
                            "east" => Some([1., 0.]),
                            "south" => Some([0., -1.]),
                            "west" => Some([-1., 0.]),
                            _ => None,
                        }
                    }
                    _ => None,
                })
        })
}
