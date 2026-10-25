//! Bind typed block parameters independently for each placement.

use crate::{Argument, BlockDefinition, BlockParameterType, Call};

#[derive(Clone, Debug, PartialEq)]
pub struct BlockInstance {
    pub origin: [f32; 2],
    pub elevation: f32,
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

impl BlockDefinition {
    /// Place a block at x/y, with optional elevation, followed by its typed
    /// parameter object when declared. The definition stays unchanged.
    pub fn instantiate(&self, args: &[Argument]) -> Result<BlockInstance, String> {
        let (placement, bindings) = match &self.parameters {
            None => (args, &[][..]),
            Some(parameters) => {
                let Some((Argument::Object(properties), placement)) = args.split_last() else {
                    return Err(format!("block '{}' needs a parameter object", self.name));
                };
                for (index, (name, _)) in properties.iter().enumerate() {
                    if !parameters.iter().any(|parameter| &parameter.name == name) {
                        return Err(format!("unknown block parameter '{name}'"));
                    }
                    if properties[..index].iter().any(|(key, _)| key == name) {
                        return Err(format!("duplicate block parameter '{name}'"));
                    }
                }
                for parameter in parameters {
                    let Some((_, value)) =
                        properties.iter().find(|(name, _)| name == &parameter.name)
                    else {
                        return Err(format!("missing block parameter '{}'", parameter.name));
                    };
                    if !matches_type(value, parameter.kind) {
                        return Err(format!(
                            "block parameter '{}' must be {}",
                            parameter.name,
                            parameter.kind.keyword()
                        ));
                    }
                }
                (placement, properties.as_slice())
            }
        };
        if !matches!(placement.len(), 2 | 3) {
            return Err(
                "block placement needs x, y, optional elevation, then its parameter object".into(),
            );
        }
        let x = finite_number(&placement[0]).ok_or("block x must be a finite number")?;
        let y = finite_number(&placement[1]).ok_or("block y must be a finite number")?;
        let elevation = match placement.get(2) {
            Some(value) => finite_number(value).ok_or("block elevation must be a finite number")?,
            None => 0.0,
        };
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
            origin: [x, y],
            elevation,
            calls,
        })
    }
}
