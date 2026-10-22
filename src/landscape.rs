//! Typed landscape geometry projected from `.road` calls.
//!
//! `land` and `lake` take at least three coordinate pairs; `river` takes at
//! least two. Coordinates inside a block are local to its placement:
//! `block_name(x, y)` translates them by `(x, y)`.

use std::fmt;

use crate::elevation::{block_height, elevation_in_calls, elevation_of};
use crate::{Argument, BlockDefinition, Call, Program, Statement};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandscapeKind {
    Land,
    Lake,
    River,
}

impl LandscapeKind {
    /// Map the exact source call name to a landscape kind.
    pub fn from_call_name(name: &str) -> Option<Self> {
        match name {
            "land" => Some(Self::Land),
            "lake" => Some(Self::Lake),
            "river" => Some(Self::River),
            _ => None,
        }
    }

    pub fn call_name(self) -> &'static str {
        match self {
            Self::Land => "land",
            Self::Lake => "lake",
            Self::River => "river",
        }
    }

    fn minimum_points(self) -> usize {
        match self {
            Self::Land | Self::Lake => 3,
            Self::River => 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LandscapeFeature {
    pub kind: LandscapeKind,
    /// The call's assigned name, if present.
    pub name: Option<String>,
    /// World coordinates. Polygons are implicitly closed; river points form an open path.
    pub points: Vec<[f32; 2]>,
    /// World elevation. A block placement elevation is added to a feature's elevation.
    pub elevation: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LandscapeError {
    pub call: String,
    pub message: String,
}

impl fmt::Display for LandscapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.call, self.message)
    }
}

impl std::error::Error for LandscapeError {}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LandscapeProjection {
    pub features: Vec<LandscapeFeature>,
    pub errors: Vec<LandscapeError>,
}

fn number(arg: &Argument) -> Option<f32> {
    let Argument::Number(value) = arg else {
        return None;
    };
    value.parse::<f32>().ok().filter(|value| value.is_finite())
}

fn error(call: &Call, message: impl Into<String>) -> LandscapeError {
    LandscapeError {
        call: call.name.clone(),
        message: message.into(),
    }
}

fn feature(
    call: &Call,
    kind: LandscapeKind,
    origin: [f32; 2],
    elevation: f32,
) -> Result<LandscapeFeature, LandscapeError> {
    if call.assigns.len() > 1 {
        return Err(error(
            call,
            "a landscape feature may have at most one assigned name",
        ));
    }
    if call.args.len() < kind.minimum_points() * 2 || call.args.len() % 2 != 0 {
        return Err(error(
            call,
            format!(
                "expected at least {} coordinate pairs",
                kind.minimum_points()
            ),
        ));
    }
    let mut points = Vec::with_capacity(call.args.len() / 2);
    for pair in call.args.chunks_exact(2) {
        let x =
            number(&pair[0]).ok_or_else(|| error(call, "coordinates must be finite numbers"))?;
        let y =
            number(&pair[1]).ok_or_else(|| error(call, "coordinates must be finite numbers"))?;
        let point = [x + origin[0], y + origin[1]];
        if !point[0].is_finite() || !point[1].is_finite() {
            return Err(error(call, "translated coordinates must be finite numbers"));
        }
        points.push(point);
    }
    Ok(LandscapeFeature {
        kind,
        name: call.assigns.first().cloned(),
        points,
        elevation,
    })
}

fn block_origin(call: &Call, origin: [f32; 2]) -> Result<([f32; 2], f32), LandscapeError> {
    if !(2..=3).contains(&call.args.len()) {
        return Err(error(
            call,
            "block placement needs x, y, and optional elevation",
        ));
    }
    let x = number(&call.args[0]).ok_or_else(|| error(call, "block x must be a finite number"))?;
    let y = number(&call.args[1]).ok_or_else(|| error(call, "block y must be a finite number"))?;
    let z = if call.args.len() == 3 {
        block_height(&call.args)
            .ok_or_else(|| error(call, "block elevation must be a finite number"))?
    } else {
        0.0
    };
    let translated = [origin[0] + x, origin[1] + y];
    if !translated[0].is_finite() || !translated[1].is_finite() {
        return Err(error(
            call,
            "translated block coordinates must be finite numbers",
        ));
    }
    Ok((translated, z))
}

fn visit_call(
    call: &Call,
    program: &Program,
    blocks: &[&BlockDefinition],
    local_calls: &[&Call],
    origin: [f32; 2],
    elevation: f32,
    stack: &mut Vec<String>,
    result: &mut LandscapeProjection,
) {
    if let Some(kind) = LandscapeKind::from_call_name(&call.name) {
        let feature_elevation = call
            .assigns
            .first()
            .and_then(|name| {
                elevation_in_calls(local_calls, name).or_else(|| elevation_of(program, name))
            })
            .unwrap_or(0.0);
        match feature(call, kind, origin, elevation + feature_elevation) {
            Ok(feature) => result.features.push(feature),
            Err(error) => result.errors.push(error),
        }
        return;
    }
    let Some(block) = blocks.iter().find(|block| block.name == call.name) else {
        return;
    };
    if stack.contains(&block.name) {
        result.errors.push(error(call, "recursive block placement"));
        return;
    }
    let (origin, local_elevation) = match block_origin(call, origin) {
        Ok(placement) => placement,
        Err(error) => {
            result.errors.push(error);
            return;
        }
    };
    let named_elevation = call
        .assigns
        .first()
        .and_then(|name| {
            elevation_in_calls(local_calls, name).or_else(|| elevation_of(program, name))
        })
        .unwrap_or(0.0);
    stack.push(block.name.clone());
    let block_calls: Vec<_> = block.calls.iter().collect();
    for inner in &block.calls {
        visit_call(
            inner,
            program,
            blocks,
            &block_calls,
            origin,
            elevation + local_elevation + named_elevation,
            stack,
            result,
        );
    }
    stack.pop();
}

/// Project landscape calls and instances of blocks containing them.
/// Invalid landscape or block calls are returned as errors alongside valid features.
pub fn project_landscape_features(program: &Program) -> LandscapeProjection {
    let blocks: Vec<_> = program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Block(block) => Some(block),
            _ => None,
        })
        .collect();
    let mut result = LandscapeProjection::default();
    for statement in &program.statements {
        if let Statement::Call(call) = statement {
            visit_call(
                call,
                program,
                &blocks,
                &[],
                [0.0, 0.0],
                0.0,
                &mut Vec::new(),
                &mut result,
            );
        }
    }
    result
}
