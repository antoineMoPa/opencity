//! Typed landscape geometry projected from `.road` calls.
//!
//! `land`, `beach`, `concrete`, and `lake` take at least three coordinate pairs; `river` takes at
//! least two. Coordinates inside a block are local to its placement:
//! `block_name(x, y)` translates them by `(x, y)`.

use std::fmt;

use crate::elevation::{elevation_in_calls, elevation_of};
use crate::{Argument, BlockDefinition, Call, Program, Statement};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandscapeKind {
    Land,
    Beach,
    Concrete,
    Lake,
    River,
    Vegetation,
    Terrain,
    Building,
}

impl LandscapeKind {
    /// Map the exact source call name to a landscape kind.
    pub fn from_call_name(name: &str) -> Option<Self> {
        match name {
            "land" => Some(Self::Land),
            "beach" => Some(Self::Beach),
            "concrete" => Some(Self::Concrete),
            "lake" => Some(Self::Lake),
            "river" => Some(Self::River),
            "vegetation" => Some(Self::Vegetation),
            "terrain" => Some(Self::Terrain),
            "building" => Some(Self::Building),
            _ => None,
        }
    }

    pub fn call_name(self) -> &'static str {
        match self {
            Self::Land => "land",
            Self::Beach => "beach",
            Self::Concrete => "concrete",
            Self::Lake => "lake",
            Self::River => "river",
            Self::Vegetation => "vegetation",
            Self::Terrain => "terrain",
            Self::Building => "building",
        }
    }

    fn minimum_points(self) -> usize {
        match self {
            Self::Land
            | Self::Beach
            | Self::Concrete
            | Self::Lake
            | Self::Vegetation
            | Self::Terrain
            | Self::Building => 3,
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
    /// Building extrusion height above its footprint.
    pub height: Option<f32>,
    /// Terrain patch height relative to its containing block.
    pub terrain_height: Option<f32>,
    /// Optional quoted GLB path attached to a building.
    pub model: Option<String>,
    /// Vegetation classification, such as `forest`.
    pub vegetation_type: Option<String>,
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainWall {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub bottom: f32,
    pub top: f32,
}

fn polygon_contains(points: &[[f32; 2]], point: [f32; 2]) -> bool {
    let mut inside = false;
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        if (a[1] > point[1]) != (b[1] > point[1])
            && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}

fn edge_crossing(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]) -> Option<f32> {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let cd = [d[0] - c[0], d[1] - c[1]];
    let denominator = ab[0] * cd[1] - ab[1] * cd[0];
    if denominator.abs() < 0.000001 {
        return None;
    }
    let ac = [c[0] - a[0], c[1] - a[1]];
    let t = (ac[0] * cd[1] - ac[1] * cd[0]) / denominator;
    let u = (ac[0] * ab[1] - ac[1] * ab[0]) / denominator;
    (t > 0.00001 && t < 0.99999 && u >= 0.0 && u <= 1.0).then_some(t)
}

/// Exposed vertical sides of a terrain contour after considering other
/// overlapping contours. Edges are split where another outline crosses them.
pub fn terrain_walls(
    feature: &LandscapeFeature,
    features: &[LandscapeFeature],
) -> Vec<TerrainWall> {
    if feature.kind != LandscapeKind::Terrain {
        return Vec::new();
    }
    let top = feature.elevation;
    let mut walls = Vec::new();
    for index in 0..feature.points.len() {
        let a = feature.points[index];
        let b = feature.points[(index + 1) % feature.points.len()];
        let mut cuts = vec![0.0, 1.0];
        for other in features
            .iter()
            .filter(|other| other.kind == LandscapeKind::Terrain && !std::ptr::eq(*other, feature))
        {
            for edge in 0..other.points.len() {
                if let Some(t) = edge_crossing(
                    a,
                    b,
                    other.points[edge],
                    other.points[(edge + 1) % other.points.len()],
                ) {
                    cuts.push(t);
                }
            }
        }
        cuts.sort_by(f32::total_cmp);
        cuts.dedup_by(|left, right| (*left - *right).abs() < 0.00001);
        for pair in cuts.windows(2) {
            let point = |t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            let middle = point((pair[0] + pair[1]) * 0.5);
            let mut bottom = 0.0_f32;
            let mut covered = false;
            for other in features.iter().filter(|other| {
                other.kind == LandscapeKind::Terrain && !std::ptr::eq(*other, feature)
            }) {
                if polygon_contains(&other.points, middle) {
                    if other.elevation >= top - 0.0001 {
                        covered = true;
                        break;
                    }
                    if other.elevation < top {
                        bottom = bottom.max(other.elevation);
                    }
                }
            }
            if !covered && top > bottom + 0.0001 {
                walls.push(TerrainWall {
                    start: point(pair[0]),
                    end: point(pair[1]),
                    bottom,
                    top,
                });
            }
        }
    }
    walls
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
    let mut offset = 0;
    let mut height = None;
    let mut model = None;
    let mut vegetation_type = None;
    match kind {
        LandscapeKind::Building => {
            let value = call
                .args
                .first()
                .and_then(number)
                .ok_or_else(|| error(call, "building needs a positive height"))?;
            if value <= 0.0 {
                return Err(error(call, "building height must be positive"));
            }
            height = Some(value);
            offset = 1;
            if let Some(Argument::String(path)) = call.args.get(offset) {
                let path = path
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .ok_or_else(|| error(call, "invalid GLB path"))?;
                model = Some(path.to_owned());
                offset += 1;
            }
        }
        LandscapeKind::Terrain => {
            let value = call
                .args
                .first()
                .and_then(number)
                .ok_or_else(|| error(call, "terrain needs an elevation"))?;
            height = Some(value);
            offset = 1;
        }
        LandscapeKind::Vegetation => {
            let Some(Argument::Identifier(value)) = call.args.first() else {
                return Err(error(call, "vegetation needs a type, such as forest"));
            };
            vegetation_type = Some(value.clone());
            offset = 1;
        }
        _ => {}
    }
    let coordinates = &call.args[offset..];
    if coordinates.len() < kind.minimum_points() * 2 || coordinates.len() % 2 != 0 {
        return Err(error(
            call,
            format!(
                "expected at least {} coordinate pairs",
                kind.minimum_points()
            ),
        ));
    }
    let mut points = Vec::with_capacity(coordinates.len() / 2);
    for pair in coordinates.chunks_exact(2) {
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
        elevation: match kind {
            LandscapeKind::Terrain => elevation + height.unwrap_or(0.0),
            // A beach lies just below adjacent land, so overlapping polygons
            // leave a narrow shore without coplanar surfaces in 3D exports.
            LandscapeKind::Beach => elevation - 0.005,
            _ => elevation,
        },
        height: if kind == LandscapeKind::Building {
            height
        } else {
            None
        },
        terrain_height: if kind == LandscapeKind::Terrain {
            height
        } else {
            None
        },
        model,
        vegetation_type,
    })
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
    let bound = match block.instantiate(&call.args) {
        Ok(bound) => bound,
        Err(message) => {
            result.errors.push(error(call, &message));
            return;
        }
    };
    let origin = [origin[0] + bound.origin[0], origin[1] + bound.origin[1]];
    if !origin.iter().all(|value| value.is_finite()) {
        result.errors.push(error(
            call,
            "translated block coordinates must be finite numbers",
        ));
        return;
    }
    let named_elevation = call
        .assigns
        .first()
        .and_then(|name| {
            elevation_in_calls(local_calls, name).or_else(|| elevation_of(program, name))
        })
        .unwrap_or(0.0);
    stack.push(block.name.clone());
    let block_calls: Vec<_> = bound.calls.iter().collect();
    for inner in &bound.calls {
        visit_call(
            inner,
            program,
            blocks,
            &block_calls,
            origin,
            elevation + bound.elevation + named_elevation,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn outlines_keep_heights_types_and_model_paths() {
        let source = "b = building(0.4, \"assets/hall.glb\", 0, 0, 2, 0, 2, 1, 0, 1);\
            woods = vegetation(forest, 0, 0, 3, 0, 3, 3);\
            ridge = terrain(0.7, 0, 0, 2, 0, 1, 2);";
        let program = parse(source).unwrap();
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
        let projection = project_landscape_features(&program);
        assert!(projection.errors.is_empty());
        assert_eq!(projection.features[0].height, Some(0.4));
        assert_eq!(
            projection.features[0].model.as_deref(),
            Some("assets/hall.glb")
        );
        assert_eq!(
            projection.features[1].vegetation_type.as_deref(),
            Some("forest")
        );
        assert_eq!(projection.features[2].elevation, 0.7);
        assert_eq!(projection.features[2].terrain_height, Some(0.7));
    }

    #[test]
    fn building_height_is_validated() {
        let program = parse("bad = building(0, 0, 0, 1, 0, 0, 1);").unwrap();
        assert_eq!(project_landscape_features(&program).errors.len(), 1);
    }

    #[test]
    fn beach_is_a_closed_area_below_adjacent_land() {
        let program = parse(
            "shore = beach(-1, -1, 2, -1, 2, 2, -1, 2);\
             island = land(0, 0, 1, 0, 1, 1, 0, 1);",
        )
        .unwrap();
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
        let projection = project_landscape_features(&program);
        assert!(projection.errors.is_empty());
        assert_eq!(projection.features[0].kind, LandscapeKind::Beach);
        assert_eq!(projection.features[0].points.len(), 4);
        assert!(projection.features[0].elevation < projection.features[1].elevation);
    }

    #[test]
    fn concrete_area_projects_as_closed_pavement() {
        let program = crate::parse("quay = concrete(0, 0, 2, 0, 2, 1, 0, 1);").unwrap();
        let projection = project_landscape_features(&program);
        assert!(projection.errors.is_empty());
        assert_eq!(projection.features.len(), 1);
        let feature = &projection.features[0];
        assert_eq!(feature.kind, LandscapeKind::Concrete);
        assert_eq!(feature.name.as_deref(), Some("quay"));
        assert_eq!(feature.points.len(), 4);
        assert_eq!(feature.elevation, 0.0);
        assert_eq!(LandscapeKind::Concrete.call_name(), "concrete");
    }
}
