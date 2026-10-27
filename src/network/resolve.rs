use super::*;
use std::collections::{HashMap, HashSet};

struct Resolver<'a> {
    definition: &'a Definition,
    paths: HashMap<String, ResolvedPath>,
    active: HashSet<String>,
}
impl Resolver<'_> {
    fn reference(&mut self, name: &str) -> Result<Point, String> {
        if let Some(port) = self
            .definition
            .ports
            .iter()
            .find(|port| format!("{}.{}", port.name, port.role.keyword()) == name)
        {
            return Ok(port.position);
        }
        let anchor = self
            .definition
            .anchors
            .iter()
            .find(|anchor| anchor.name == name)
            .cloned()
            .ok_or_else(|| format!("unknown port or anchor '{name}'"))?;
        let path = self.path(&anchor.path)?;
        point_at_distance(&path.points, anchor.distance)
            .ok_or_else(|| format!("anchor '{}' is outside path '{}'", anchor.name, anchor.path))
    }
    fn endpoint(&mut self, endpoint: &Endpoint) -> Result<Point, String> {
        match endpoint {
            Endpoint::Point(point) => Ok(*point),
            Endpoint::Reference(name) => self.reference(name),
        }
    }
    fn path(&mut self, name: &str) -> Result<ResolvedPath, String> {
        if let Some(path) = self.paths.get(name) {
            return Ok(path.clone());
        }
        if !self.active.insert(name.into()) {
            return Err(format!("cyclic path/anchor dependency at '{name}'"));
        }
        let definition = self
            .definition
            .paths
            .iter()
            .find(|path| path.name == name)
            .cloned()
            .ok_or_else(|| format!("unknown path '{name}'"))?;
        let mut points = vec![self.endpoint(&definition.start)?];
        for segment in &definition.segments {
            let endpoint = match segment {
                PathSegment::Line(end)
                | PathSegment::Cubic { end, .. }
                | PathSegment::Arc { end, .. } => end,
            };
            let end = self.endpoint(endpoint)?;
            geometry::append_segment(&mut points, segment, end)?;
        }
        let path = ResolvedPath {
            name: name.into(),
            points,
        };
        if path.points.len() < 2 || !path.length().is_finite() || path.length() <= 0.000001 {
            return Err(format!("path '{name}' must have nonzero finite length"));
        }
        self.active.remove(name);
        self.paths.insert(name.into(), path.clone());
        Ok(path)
    }
    fn bound(&self, bound: &Bound, path: &ResolvedPath) -> Result<f32, String> {
        match bound {
            Bound::Start => Ok(0.),
            Bound::End => Ok(path.length()),
            Bound::Distance(value) => Ok(*value),
            Bound::Anchor(name) => self
                .definition
                .anchors
                .iter()
                .find(|anchor| anchor.name == *name && anchor.path == path.name)
                .map(|anchor| anchor.distance)
                .ok_or_else(|| format!("anchor '{name}' does not belong to path '{}'", path.name)),
        }
    }
}

/// Resolve forward references and explicit movement chains. Physical crossings
/// do not create connections. Heading mismatches are advisory for draft geometry.
pub fn resolve(definition: &Definition) -> Result<Resolved, String> {
    if definition.template.is_some() {
        return Err("parameterized component needs placement arguments".into());
    }
    let mut names = HashSet::new();
    for name in definition
        .paths
        .iter()
        .map(|p| p.name.clone())
        .chain(definition.anchors.iter().map(|a| a.name.clone()))
        .chain(
            definition
                .ports
                .iter()
                .map(|p| format!("{}.{}", p.name, p.role.keyword())),
        )
    {
        if !names.insert(name.clone()) {
            return Err(format!("duplicate component name '{name}'"));
        }
    }
    let mut band_names = HashSet::new();
    for band in &definition.bands {
        if !band_names.insert(&band.name) {
            return Err(format!("duplicate surface/marking '{}'", band.name));
        }
        if !band.width.is_finite() || band.width <= 0. {
            return Err(format!(
                "band '{}' needs a positive finite width",
                band.name
            ));
        }
    }
    for port in &definition.ports {
        if ![port.position.x, port.position.y, port.position.z]
            .iter()
            .all(|n| n.is_finite())
            || port.width.is_some_and(|w| !w.is_finite() || w <= 0.)
        {
            return Err(format!(
                "port '{}' has invalid position or width",
                port.name
            ));
        }
    }
    let mut resolver = Resolver {
        definition,
        paths: HashMap::new(),
        active: HashSet::new(),
    };
    let mut paths = Vec::new();
    for path in &definition.paths {
        paths.push(resolver.path(&path.name)?);
    }
    let mut anchors = Vec::new();
    for anchor in &definition.anchors {
        anchors.push((anchor.name.clone(), resolver.reference(&anchor.name)?));
    }
    for band in &definition.bands {
        resolver.path(&band.path)?;
    }
    let mut movements = Vec::new();
    let mut warnings = Vec::new();
    let mut movement_keys = HashSet::new();
    for movement in &definition.movements {
        let key = (&movement.class, &movement.from, &movement.to);
        if !movement_keys.insert(key) {
            return Err(format!(
                "duplicate movement {} -> {}",
                movement.from, movement.to
            ));
        }
        let port = |name: &str, role| {
            definition
                .ports
                .iter()
                .find(|port| {
                    port.role == role && format!("{}.{}", port.name, role.keyword()) == name
                })
                .ok_or_else(|| {
                    format!(
                        "movement endpoint '{name}' must be an {} port",
                        role.keyword()
                    )
                })
        };
        let from = port(&movement.from, LaneRole::In)?;
        let to = port(&movement.to, LaneRole::Out)?;
        let mut points: Vec<Point> = Vec::new();
        for traversal in &movement.via {
            let path = resolver.path(&traversal.path)?;
            let start = resolver.bound(&traversal.start, &path)?;
            let end = resolver.bound(&traversal.end, &path)?;
            let slice = slice_path(&path.points, start, end)
                .ok_or_else(|| format!("invalid range on path '{}'", path.name))?;
            if points
                .last()
                .is_some_and(|last| last.distance(slice[0]) > 0.001)
            {
                return Err(format!(
                    "disconnected movement chain {} -> {} at '{}'",
                    movement.from, movement.to, path.name
                ));
            }
            if points.is_empty() {
                points.extend(slice);
            } else {
                points.extend(slice.into_iter().skip(1));
            }
        }
        if points
            .first()
            .is_none_or(|p| p.distance(from.position) > 0.001)
            || points
                .last()
                .is_none_or(|p| p.distance(to.position) > 0.001)
        {
            return Err(format!(
                "movement {} -> {} does not meet its ports",
                movement.from, movement.to
            ));
        }
        for (port, a, b) in [
            (from, points[0], points[1]),
            (to, points[points.len() - 2], points[points.len() - 1]),
        ] {
            let heading = port.heading.vector();
            let angle = (b.y - a.y).atan2(b.x - a.x);
            if heading[0] * angle.cos() + heading[1] * angle.sin() < 0.95 {
                warnings.push(format!(
                    "movement {} -> {} has a tangent mismatch at {}.{}",
                    movement.from,
                    movement.to,
                    port.name,
                    port.role.keyword()
                ));
            }
        }
        movements.push(ResolvedMovement {
            movement: movement.clone(),
            points,
        });
    }
    Ok(Resolved {
        band_meshes: vec![],
        ports: definition.ports.clone(),
        paths,
        anchors,
        movements,
        bands: definition.bands.clone(),
        warnings,
    })
}
