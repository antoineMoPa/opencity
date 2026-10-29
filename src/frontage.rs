//! Neutral road-side zoning and building-form controls, in map units.
use crate::{map, road_profiles, Argument, BuildingUse, Call, Program, Statement};

/// Building form is independent of land use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildingForm {
    Street,
    CityCentre,
}
impl BuildingForm {
    pub fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "street" => Some(Self::Street),
            "city_centre" => Some(Self::CityCentre),
            _ => None,
        }
    }
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Street => "street",
            Self::CityCentre => "city_centre",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneForm {
    pub form: BuildingForm,
    /// Fraction of deterministic slots retained; changing it does not reseed lots.
    pub density: f32,
    pub height_min: f32,
    pub height_max: f32,
    pub frontage_min: f32,
    pub frontage_max: f32,
    pub setback: f32,
}
impl Default for ZoneForm {
    fn default() -> Self {
        Self {
            form: BuildingForm::Street,
            density: 1.,
            height_min: 0.034,
            height_max: 0.21,
            frontage_min: 0.085,
            frontage_max: 0.255,
            setback: 0.004,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontageSide {
    Left,
    Right,
    Both,
}
impl FrontageSide {
    pub fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "both" => Some(Self::Both),
            _ => None,
        }
    }
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Both => "both",
        }
    }
}

/// Frame at a cubic parameter. Origin is the outside edge of the full road
/// cross-section (including sidewalk); outward points into the reservation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrontageFrame {
    pub origin: [f32; 3],
    pub along: [f32; 2],
    pub outward: [f32; 2],
    pub depth: f32,
}

#[derive(Clone, Debug)]
pub struct FrontageZone {
    pub name: String,
    pub road_placement: String,
    pub building_use: BuildingUse,
    pub side: FrontageSide,
    /// Cubic segment parameters, not arc-length fractions.
    pub start: f32,
    pub end: f32,
    pub depth: f32,
    pub seed: u64,
    pub form: ZoneForm,
    segment: map::Segment,
    definition: crate::RoadDefinition,
    profile: road_profiles::RoadProfile,
}
impl FrontageZone {
    pub fn segment(&self) -> &map::Segment {
        &self.segment
    }

    /// `Both` has no single frame. Invalid or singular samples return None.
    pub fn frame_at(&self, side: FrontageSide, t: f32) -> Option<FrontageFrame> {
        if !t.is_finite()
            || t < self.start
            || t > self.end
            || side == FrontageSide::Both
            || (self.side != FrontageSide::Both && self.side != side)
        {
            return None;
        }
        let tangent = self.segment.tangent_at(t);
        let length = tangent.x.hypot(tangent.y);
        if !length.is_finite() || length <= 1e-8 {
            return None;
        }
        let along = [tangent.x / length, tangent.y / length];
        let left = [-along[1], along[0]];
        let (widths, offset) = road_profiles::section_widths(&self.definition, &self.profile, t);
        let edge = match side {
            FrontageSide::Left => offset,
            FrontageSide::Right => offset + widths.iter().sum::<f32>(),
            FrontageSide::Both => return None,
        };
        let sign = if side == FrontageSide::Left {
            1.0
        } else {
            -1.0
        };
        let p = self.segment.point_at(t);
        let frame = FrontageFrame {
            origin: [
                p.x - left[0] * edge,
                p.y - left[1] * edge,
                self.segment.height_at(t),
            ],
            along,
            outward: [left[0] * sign, left[1] * sign],
            depth: self.depth,
        };
        if frame
            .origin
            .iter()
            .chain(frame.along.iter())
            .chain(frame.outward.iter())
            .any(|v| !v.is_finite())
            || (0..2).any(|i| !(frame.origin[i] + frame.outward[i] * self.depth).is_finite())
        {
            return None;
        }
        Some(frame)
    }

    /// Sampled strip cells, useful for overlays and precise footprint intersection
    /// tests. Curves are approximated with 1..=1024 subdivisions per side.
    /// Reservations may overlap; consumers resolve occupancy with stable zone IDs.
    pub fn strip_quads(&self, subdivisions: usize) -> Vec<[[f32; 3]; 4]> {
        let count = subdivisions.clamp(1, 1024);
        let mut result = Vec::new();
        for side in [FrontageSide::Left, FrontageSide::Right] {
            for step in 0..count {
                let t = |i| {
                    if i == count {
                        self.end
                    } else {
                        self.start + (self.end - self.start) * i as f32 / count as f32
                    }
                };
                let (Some(a), Some(b)) = (
                    self.frame_at(side, t(step)),
                    self.frame_at(side, t(step + 1)),
                ) else {
                    continue;
                };
                let outer = |f: FrontageFrame| {
                    [
                        f.origin[0] + f.outward[0] * f.depth,
                        f.origin[1] + f.outward[1] * f.depth,
                        f.origin[2],
                    ]
                };
                result.push([a.origin, b.origin, outer(b), outer(a)]);
            }
        }
        result
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontageError {
    pub zone: String,
    pub message: String,
}
impl std::fmt::Display for FrontageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.zone, self.message)
    }
}
impl std::error::Error for FrontageError {}
#[derive(Clone, Debug, Default)]
pub struct FrontageProjection {
    pub zones: Vec<FrontageZone>,
    pub errors: Vec<FrontageError>,
}

fn finite(value: &Argument) -> Option<f32> {
    let Argument::Number(text) = value else {
        return None;
    };
    text.parse::<f32>().ok().filter(|v| v.is_finite())
}
fn identifier(value: &Argument) -> Option<&str> {
    let Argument::Identifier(text) = value else {
        return None;
    };
    Some(text)
}
fn parse_zone(
    call: &Call,
    program: &Program,
    projection: &map::Projection,
    profiles: &[road_profiles::RoadProfile],
) -> Result<FrontageZone, String> {
    if call.assigns.len() != 1 {
        return Err("frontage_zone needs exactly one stable assigned name".into());
    }
    let name = &call.assigns[0];
    let declarations = |name: &str| {
        program
            .statements
            .iter()
            .map(|s| match s {
                Statement::Call(c) => c.assigns.iter().filter(|v| v.as_str() == name).count(),
                Statement::Road(r) => usize::from(r.name == name),
                Statement::Block(b) => usize::from(b.name == name),
                Statement::Network(n) => usize::from(n.name.as_deref() == Some(name)),
            })
            .sum::<usize>()
    };
    if declarations(name) != 1 {
        return Err("ambiguous zone name".into());
    }
    let [Argument::Identifier(reference), Argument::Object(options)] = call.args.as_slice() else {
        return Err(
            "expected frontage_zone(named_road_placement, {use, side, start, end, depth, seed})"
                .into(),
        );
    };
    if declarations(reference) > 1 {
        return Err(format!("ambiguous road placement '{reference}'"));
    }
    let matches: Vec<_> = projection
        .segments
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name.as_ref() == Some(reference))
        .collect();
    if matches.len() > 1 {
        return Err(format!("ambiguous road placement '{reference}'"));
    }
    let Some(&(index, segment)) = matches.first() else {
        return Err(format!(
            "missing or unsupported road placement '{reference}'; \
             reference a top-level named road placement, not a definition"
        ));
    };
    let map::SegmentSource::TopLevel(_) = segment.source else {
        return Err("block-instance road references are unsupported".into());
    };
    // Expanded junction/component roads must not accidentally become addressable placements.
    let is_placement = program.statements.iter().any(|s| match s {
        Statement::Call(c) => {
            c.assigns.len() == 1 && c.assigns[0] == *reference && c.name == segment.road_name
        }
        _ => false,
    });
    if !is_placement {
        return Err("reference must name a top-level road placement".into());
    }
    let mut use_kind = None;
    let mut side = None;
    let mut start = None;
    let mut end = None;
    let mut depth = None;
    let mut seed = None;
    let mut form = ZoneForm::default();
    if options
        .iter()
        .any(|(k, v)| k == "form" && identifier(v) == Some("city_centre"))
    {
        form = ZoneForm {
            form: BuildingForm::CityCentre,
            height_min: 0.24,
            height_max: 0.70,
            frontage_min: 0.17,
            frontage_max: 0.34,
            setback: 0.012,
            ..form
        };
    }
    for (i, (key, value)) in options.iter().enumerate() {
        if options[..i].iter().any(|(previous, _)| previous == key) {
            return Err(format!("duplicate frontage option '{key}'"));
        }
        match key.as_str() {
            "form" => {
                form.form = identifier(value)
                    .and_then(BuildingForm::from_keyword)
                    .ok_or("form must be street or city_centre")?
            }
            "density" => form.density = finite(value).ok_or("density must be finite")?,
            "height_min" => form.height_min = finite(value).ok_or("height_min must be finite")?,
            "height_max" => form.height_max = finite(value).ok_or("height_max must be finite")?,
            "frontage_min" => {
                form.frontage_min = finite(value).ok_or("frontage_min must be finite")?
            }
            "frontage_max" => {
                form.frontage_max = finite(value).ok_or("frontage_max must be finite")?
            }
            "setback" => form.setback = finite(value).ok_or("setback must be finite")?,
            "use" => {
                use_kind = Some(
                    identifier(value)
                        .and_then(BuildingUse::from_keyword)
                        .filter(|v| *v != BuildingUse::Unspecified)
                        .ok_or("use must be residential, commercial or mixed_use")?,
                )
            }
            "side" => {
                side = Some(
                    identifier(value)
                        .and_then(FrontageSide::from_keyword)
                        .ok_or("side must be left, right or both")?,
                )
            }
            "start" => start = Some(finite(value).ok_or("start must be finite")?),
            "end" => end = Some(finite(value).ok_or("end must be finite")?),
            "depth" => depth = Some(finite(value).ok_or("depth must be finite")?),
            "seed" => {
                let Argument::Number(text) = value else {
                    return Err("seed must be a u64 integer".into());
                };
                seed = Some(
                    text.parse::<u64>()
                        .map_err(|_| "seed must be a u64 integer")?,
                );
            }
            _ => return Err(format!("unknown frontage option '{key}'")),
        }
    }
    let zone = FrontageZone {
        name: name.clone(),
        road_placement: reference.clone(),
        building_use: use_kind.ok_or("missing use")?,
        side: side.ok_or("missing side")?,
        start: start.ok_or("missing start")?,
        end: end.ok_or("missing end")?,
        depth: depth.ok_or("missing depth")?,
        seed: seed.ok_or("missing seed")?,
        form,
        segment: segment.clone(),
        definition: road_profiles::road_for_segment(program, segment)
            .ok_or("missing road definition")?
            .clone(),
        profile: profiles[index].clone(),
    };
    if !(0.0 <= zone.start && zone.start < zone.end && zone.end <= 1.0) {
        return Err("expected 0 <= start < end <= 1".into());
    }
    if zone.depth <= 0.0 {
        return Err("depth must be positive".into());
    }
    if !(0. ..=1.).contains(&form.density)
        || form.height_min <= 0.
        || form.height_max < form.height_min
        || form.frontage_min <= 0.
        || form.frontage_max < form.frontage_min
        || form.setback < 0.
        || form.setback >= zone.depth
    {
        return Err(
            "expected density in 0..=1, ordered positive height/frontage ranges \
            and 0 <= setback < depth"
                .into(),
        );
    }
    for side in [FrontageSide::Left, FrontageSide::Right]
        .into_iter()
        .filter(|s| zone.side == FrontageSide::Both || zone.side == *s)
    {
        for i in 0..=32 {
            let t = if i == 32 {
                zone.end
            } else {
                zone.start + (zone.end - zone.start) * i as f32 / 32.0
            };
            if zone.frame_at(side, t).is_none() {
                return Err("frontage requires finite, nonsingular road geometry".into());
            }
        }
    }
    Ok(zone)
}

/// Project only uniquely named top-level road placements. Block-local zone calls
/// and unresolved/ambiguous references produce diagnostics, never guessed matches.
pub fn project_frontage_zones(program: &Program) -> FrontageProjection {
    let has_zones = program.statements.iter().any(|statement| match statement {
        Statement::Call(call) => call.name == "frontage_zone",
        Statement::Block(block) => block.calls.iter().any(|call| call.name == "frontage_zone"),
        _ => false,
    });
    if !has_zones {
        return FrontageProjection::default();
    }
    let projection = map::project(program);
    let profiles = road_profiles::road_profiles(program, &projection);
    let mut result = FrontageProjection::default();
    for statement in &program.statements {
        match statement {
            Statement::Call(call) if call.name == "frontage_zone" => {
                match parse_zone(call, program, &projection, &profiles) {
                    Ok(zone) => result.zones.push(zone),
                    Err(message) => result.errors.push(FrontageError {
                        zone: call
                            .assigns
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "frontage_zone".into()),
                        message,
                    }),
                }
            }
            Statement::Block(block) => {
                for call in &block.calls {
                    if call.name == "frontage_zone" {
                        result.errors.push(FrontageError {
                            zone: call
                                .assigns
                                .first()
                                .cloned()
                                .unwrap_or_else(|| "frontage_zone".into()),
                            message: "block-local frontage zones are unsupported; \
                                author a top-level zone"
                                .into(),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    result
}
