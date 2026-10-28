//! Bind typed block parameters independently for each placement.

use crate::{Argument, BlockDefinition, BlockParameterType, Call};

#[derive(Clone, Debug, PartialEq)]
pub struct BlockInstance {
    pub origin: [f32; 2],
    pub elevation: f32,
    pub rotation: f32,
    pub calls: Vec<Call>,
}

fn finite_number(value: &Argument) -> Option<f32> {
    match value {
        Argument::Number(text) => text.parse::<f32>().ok().filter(|value| value.is_finite()),
        _ => None,
    }
}

fn matches_type(value: &Argument, kind: BlockParameterType) -> bool {
    match kind {
        BlockParameterType::Number => finite_number(value).is_some(),
        BlockParameterType::String => matches!(value, Argument::String(_)),
        BlockParameterType::Identifier => matches!(value, Argument::Identifier(_)),
    }
}

fn bind_argument(value: &Argument, bindings: &[(String, Argument)]) -> Argument {
    match value {
        Argument::Identifier(name) => bindings
            .iter()
            .find(|(key, _)| key == name)
            .map_or_else(|| value.clone(), |(_, value)| value.clone()),
        Argument::Object(properties) => Argument::Object(
            properties
                .iter()
                .map(|(key, value)| (key.clone(), bind_argument(value, bindings)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

#[derive(Clone, Copy)]
pub(crate) enum PlacementKind {
    Block,
    Component,
}
impl PlacementKind {
    fn keyword(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Component => "component",
        }
    }
}
pub(crate) struct Placement<'a> {
    pub origin: [f32; 2],
    pub elevation: f32,
    pub rotation: f32,
    pub bindings: &'a [(String, Argument)],
}
pub(crate) fn bind_placement<'a>(
    parameters: Option<&[crate::BlockParameter]>,
    args: &'a [Argument],
    kind: PlacementKind,
    name: &str,
) -> Result<Placement<'a>, String> {
    let kind = kind.keyword();
    let (placement, bindings) = match parameters {
        None => (args, &[][..]),
        Some(parameters) => {
            let Some((Argument::Object(properties), placement)) = args.split_last() else {
                return Err(format!("{kind} '{}' needs a parameter object", name));
            };
            for (index, (name, _)) in properties.iter().enumerate() {
                if !parameters.iter().any(|parameter| &parameter.name == name) {
                    return Err(format!("unknown {kind} parameter '{name}'"));
                }
                if properties[..index].iter().any(|(key, _)| key == name) {
                    return Err(format!("duplicate {kind} parameter '{name}'"));
                }
            }
            for parameter in parameters {
                let Some((_, value)) = properties.iter().find(|(name, _)| name == &parameter.name)
                else {
                    return Err(format!("missing {kind} parameter '{}'", parameter.name));
                };
                if !matches_type(value, parameter.kind) {
                    return Err(format!(
                        "{kind} parameter '{}' must be {}",
                        parameter.name,
                        parameter.kind.keyword()
                    ));
                }
            }
            (placement, properties.as_slice())
        }
    };
    if !matches!(placement.len(), 2 | 3 | 4) {
        return Err(format!(
            "{kind} placement needs x, y, optional elevation and rotation, \
                then its parameter object"
        ));
    }
    let x =
        finite_number(&placement[0]).ok_or_else(|| format!("{kind} x must be a finite number"))?;
    let y =
        finite_number(&placement[1]).ok_or_else(|| format!("{kind} y must be a finite number"))?;
    let elevation = match placement.get(2) {
        Some(value) => finite_number(value)
            .ok_or_else(|| format!("{kind} elevation must be a finite number"))?,
        None => 0.0,
    };
    let rotation = match placement.get(3) {
        Some(value) => finite_number(value)
            .ok_or_else(|| format!("{kind} rotation must be a finite number"))?,
        None => 0.0,
    };
    Ok(Placement {
        origin: [x, y],
        elevation,
        rotation,
        bindings,
    })
}

impl BlockDefinition {
    /// Place a block at x/y, with optional elevation and rotation, followed by its typed
    /// parameter object when declared. The definition stays unchanged.
    pub fn instantiate(&self, args: &[Argument]) -> Result<BlockInstance, String> {
        let placement = bind_placement(
            self.parameters.as_deref(),
            args,
            PlacementKind::Block,
            &self.name,
        )?;
        let bindings = placement.bindings;
        let mut calls = self.calls.clone();
        for call in &mut calls {
            if let Some((_, value)) = bindings.iter().find(|(name, _)| name == &call.name) {
                let Argument::Identifier(name) = value else {
                    return Err(format!(
                        "callable block parameter '{}' must be identifier",
                        call.name
                    ));
                };
                call.name = name.clone();
            }
            call.args = call
                .args
                .iter()
                .map(|value| bind_argument(value, bindings))
                .collect();
        }
        Ok(BlockInstance {
            origin: placement.origin,
            elevation: placement.elevation,
            rotation: placement.rotation,
            calls,
        })
    }
}

/// Rotate a world-space vector counterclockwise, in degrees.
pub fn rotate_vector(point: [f32; 2], degrees: f32) -> [f32; 2] {
    let (sin, cos) = f64::from(degrees).to_radians().sin_cos();
    let (sin, cos) = (sin as f32, cos as f32);
    [
        point[0] * cos - point[1] * sin,
        point[0] * sin + point[1] * cos,
    ]
}
