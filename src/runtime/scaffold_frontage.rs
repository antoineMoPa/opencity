//! Deterministic generic frontage scaffolds; all dimensions are map units.
use crate::runtime::{City, Region, SurfaceKind};
use crate::{Argument, BuildingUse, Call, FrontageSide, LandscapeKind, Program, Statement};

pub type Footprint = Vec<[f32; 2]>;
// Preserve curved reservations while coalescing straight, adjacent cells.
// Whole-street zoning otherwise makes every housing test visit thousands of
// identical collinear pieces. Tolerance is 0.01 mm at the game's map scale.
pub fn strip_footprints(zone: &crate::FrontageZone) -> Vec<Footprint> {
    let mut result: Vec<Footprint> = Vec::new();
    for q in zone.strip_quads(128) {
        let q = q.map(|p| [p[0], p[1]]);
        if let Some(last) = result.last_mut() {
            let straight = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
                let d = [b[0] - a[0], b[1] - a[1]];
                let e = [c[0] - b[0], c[1] - b[1]];
                (d[0] * e[1] - d[1] * e[0]).abs() <= d[0].hypot(d[1]) * 1e-7
                    && d[0] * e[0] + d[1] * e[1] > 0.
            };
            if last[1] == q[0]
                && last[2] == q[3]
                && straight(last[0], last[1], q[1])
                && straight(last[3], last[2], q[2])
            {
                last[1] = q[1];
                last[2] = q[2];
                continue;
            }
        }
        result.push(q.to_vec());
    }
    result
}
pub fn overlap(a: &[[f32; 2]], b: &[[f32; 2]]) -> bool {
    for polygon in [a, b] {
        for (p, q) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
            let axis = [p[1] - q[1], q[0] - p[0]];
            let axis_length = axis[0].hypot(axis[1]);
            if axis_length < 1e-9 {
                continue;
            }
            let range = |points: &[[f32; 2]]| {
                points
                    .iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |r, v| {
                        let d = v[0] * axis[0] + v[1] * axis[1];
                        (r.0.min(d), r.1.max(d))
                    })
            };
            let (x, y) = range(a);
            let (u, v) = range(b);
            if y <= u + axis_length * 0.00003 || v <= x + axis_length * 0.00003 {
                return false;
            }
        }
    }
    true
}
pub fn contains(polygon: &[[f32; 2]], p: [f32; 2]) -> bool {
    let mut positive = false;
    let mut negative = false;
    for (a, b) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
        let cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        if cross > 1e-8 {
            positive = true;
        }
        if cross < -1e-8 {
            negative = true;
        }
    }
    !(positive && negative)
}
pub fn reservations(program: &Program) -> Result<Vec<Footprint>, String> {
    let projected = crate::project_frontage_zones(program);
    if !projected.errors.is_empty() {
        return Err(projected
            .errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; "));
    }
    Ok(projected.zones.iter().flat_map(strip_footprints).collect())
}
/// Only ordinary legacy shells intersecting the exact strips are replaceable.
pub fn prepare(program: &Program, strips: &[Footprint]) -> Program {
    let projection = crate::project_landscape_features(program);
    let removed: std::collections::HashSet<_> = projection
        .features
        .iter()
        .filter(|f| {
            f.kind == LandscapeKind::Building
                && f.model.is_none()
                && f.height.unwrap_or(0.) <= 0.16
                && f.building_use == BuildingUse::Unspecified
        })
        .filter(|f| strips.iter().any(|q| overlap(&f.points, q)))
        .filter_map(|f| f.name.clone())
        .collect();
    Program {
        statements: program
            .statements
            .iter()
            .filter(|s| match s {
                Statement::Call(c) if c.name == "building" => {
                    !c.assigns.first().is_some_and(|n| removed.contains(n))
                }
                Statement::Call(c) if c.name == "elevation" => {
                    !matches!(c.args.first(), Some(Argument::Identifier(n)) if removed.contains(n))
                }
                _ => true,
            })
            .cloned()
            .collect(),
    }
}
pub fn seed(name: &str) -> u64 {
    name.bytes().fold(0xcbf29ce484222325, |s, b| {
        (s ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}
#[derive(Clone, Copy, Debug)]
pub enum VolumeKind {
    Row,
    DeepRow,
    Low,
    Forecourt,
    Deep,
    Wide,
    Setback,
    Pitched,
}
impl VolumeKind {
    pub fn from_seed(s: u64) -> Self {
        match s % 8 {
            0 => Self::Row,
            1 => Self::DeepRow,
            2 => Self::Low,
            3 => Self::Forecourt,
            4 => Self::Deep,
            5 => Self::Wide,
            6 => Self::Setback,
            _ => Self::Pitched,
        }
    }
    pub fn floors(self, s: u64) -> u8 {
        match self {
            Self::Low => 1,
            Self::Setback | Self::Pitched => 1 + (s % 2) as u8,
            Self::Deep => 3 + (s % 3) as u8,
            Self::Wide => [3, 5, 4, 5][(s % 4) as usize],
            _ => 2 + (s % 2) as u8,
        }
    }
    pub fn floor_height(self) -> f32 {
        match self {
            Self::Wide => 0.042,
            Self::Deep => 0.038,
            _ => 0.034,
        }
    }
}
fn num(n: f32) -> Argument {
    Argument::Number(format!("{n:.7}"))
}
pub fn populate(program: &mut Program, city: &City) -> usize {
    let mut zones: Vec<_> = city.frontage_zones.iter().collect();
    let priority =
        |z: &&crate::FrontageZone| usize::from(z.building_use == BuildingUse::Residential);
    zones.sort_by(|a, b| priority(a).cmp(&priority(b)).then(a.name.cmp(&b.name)));
    let retail_strips: Vec<_> = zones
        .iter()
        .filter(|z| z.building_use != BuildingUse::Residential)
        .flat_map(|z| strip_footprints(z))
        .collect();
    let mut occupied: Vec<Footprint> = city
        .query(Region::WholeMap)
        .buildings
        .iter()
        .map(|(_, f)| f.points.clone())
        .collect();
    let mut count = 0;
    for zone in zones {
        // Fixed parameter slots depend only on this named placement and its seed.
        let length: f32 = (0..128)
            .map(|i| {
                let a = zone
                    .segment()
                    .point_at(zone.start + (zone.end - zone.start) * i as f32 / 128.);
                let b = zone
                    .segment()
                    .point_at(zone.start + (zone.end - zone.start) * (i + 1) as f32 / 128.);
                (a.x - b.x).hypot(a.y - b.y)
            })
            .sum();
        // Query exact road/junction/water triangles once per zone, then reject even a
        // small corner intersection that could fall between the support samples.
        let midpoint = zone.segment().point_at((zone.start + zone.end) * 0.5);
        let region = Region::Radius {
            center: [midpoint.x, midpoint.y],
            radius: length + zone.depth,
        };
        // Reuse compiled collision triangles. Rendering a local mesh here also
        // tessellates heightmap terrain that is discarded by the obstacle filter.
        let radius = length + zone.depth;
        let lo = super::geometry::cell([midpoint.x - radius, midpoint.y - radius]);
        let hi = super::geometry::cell([midpoint.x + radius, midpoint.y + radius]);
        let mut ids = std::collections::HashSet::new();
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                if let Some(indices) = city.cells.get(&(x, y)) {
                    ids.extend(indices.iter().copied());
                }
            }
        }
        let obstacles: Vec<_> = ids
            .into_iter()
            .filter_map(|i| {
                let t = &city.triangles[i];
                if !matches!(
                    t.kind,
                    SurfaceKind::Asphalt
                        | SurfaceKind::Sand
                        | SurfaceKind::Water
                        | SurfaceKind::Sidewalk
                ) || !super::geometry::polygon_region(&t.points.map(|p| [p[0], p[1]]), region)
                {
                    return None;
                }
                Some((
                    t.points.map(|p| [p[0], p[1]]),
                    t.points.iter().map(|p| p[2]).fold(f32::INFINITY, f32::min),
                    t.points
                        .iter()
                        .map(|p| p[2])
                        .fold(f32::NEG_INFINITY, f32::max),
                ))
            })
            .collect();

        let slots = (length / zone.form.frontage_min).floor().clamp(1., 2000.) as usize;
        let strips = strip_footprints(zone);
        for (side, label) in [(FrontageSide::Left, "left"), (FrontageSide::Right, "right")] {
            let mut next_slot = 0;
            while next_slot < slots {
                let slot = next_slot;
                let key = format!(
                    "{}:{}:{}:{label}:{slot}",
                    zone.name, zone.seed, zone.road_placement
                );
                let id = seed(&key);
                let family = VolumeKind::from_seed(id);
                let span = match family {
                    VolumeKind::Wide => 3,
                    VolumeKind::Deep | VolumeKind::Setback => 2,
                    _ => 1,
                };
                let span = span
                    .min(
                        (zone.form.frontage_max / zone.form.frontage_min)
                            .floor()
                            .max(1.) as usize,
                    )
                    .min(slots - slot);
                next_slot += span;
                if (id >> 32) as u32 as f64 / u32::MAX as f64 >= zone.form.density as f64 {
                    continue;
                }
                let t0 = zone.start + (zone.end - zone.start) * slot as f32 / slots as f32;
                let t1 = zone.start + (zone.end - zone.start) * (slot + span) as f32 / slots as f32;
                let t = (t0 + t1) * 0.5;

                let Some(frame) = zone.frame_at(side, t) else {
                    continue;
                };
                let (Some(a), Some(b)) = (zone.frame_at(side, t0), zone.frame_at(side, t1)) else {
                    continue;
                };
                let project = |p: [f32; 3]| {
                    (p[0] - frame.origin[0]) * frame.along[0]
                        + (p[1] - frame.origin[1]) * frame.along[1]
                };
                let width = 2. * project(a.origin).abs().min(project(b.origin).abs()) - 0.004;

                let depth = (zone.depth - 0.012).min(0.145);
                if width < 0.045 || depth < 0.055 {
                    continue;
                }
                let setback = zone.form.setback;
                let point = |x: f32, y: f32| {
                    [
                        frame.origin[0] + frame.along[0] * x + frame.outward[0] * y,
                        frame.origin[1] + frame.along[1] * x + frame.outward[1] * y,
                    ]
                };
                let corners = vec![
                    point(-width / 2., setback),
                    point(width / 2., setback),
                    point(width / 2., depth),
                    point(-width / 2., depth),
                ];
                if obstacles.iter().any(|(p, lo, hi)| {
                    *hi >= frame.origin[2] - 0.025
                        && *lo <= frame.origin[2] + 0.025
                        && overlap(&corners, p)
                }) {
                    continue;
                }
                if zone.building_use == BuildingUse::Residential
                    && retail_strips.iter().any(|p| overlap(&corners, p))
                {
                    continue;
                }
                if occupied.iter().any(|p| overlap(&corners, p)) {
                    continue;
                }
                let mut height = 0.;
                let mut valid = true;
                // Dense support checks reject steep or unsupported terrain.
                for i in 0..=6 {
                    for j in 0..=6 {
                        let p = point(
                            -width / 2. + width * i as f32 / 6.,
                            setback + (depth - setback) * j as f32 / 6.,
                        );
                        if !strips.iter().any(|q| contains(q, p)) {
                            valid = false;
                        }
                        match city.surface_at(p, frame.origin[2] + 0.025) {
                            Some(s)
                                if s.kind == SurfaceKind::Grass
                                    && (s.height - frame.origin[2]).abs() < 0.02 =>
                            {
                                height += s.height
                            }
                            _ => valid = false,
                        }
                    }
                }
                if !valid {
                    continue;
                }
                let name = format!(
                    "{}_{id:016x}",
                    if zone.building_use == BuildingUse::Residential {
                        "generated_home_zone"
                    } else {
                        "generated_shop"
                    }
                );
                let floors = family.floors(id >> 8);
                let floor_height = family.floor_height();

                let mut args = vec![
                    num(if zone.form.form == crate::BuildingForm::CityCentre {
                        zone.form.height_min
                            + ((id >> 24) % 1000) as f32 / 999.
                                * (zone.form.height_max - zone.form.height_min)
                    } else {
                        (0.042 + (floors - 1) as f32 * floor_height)
                            .clamp(zone.form.height_min, zone.form.height_max)
                    }),
                    Argument::Object(vec![
                        ("material".into(), Argument::Identifier("concrete".into())),
                        ("floor_height".into(), num(floor_height)),
                        (
                            "use".into(),
                            Argument::Identifier(zone.building_use.keyword().into()),
                        ),
                        ("front_edge".into(), Argument::Number("0".into())),
                    ]),
                ];
                args.extend(corners.iter().flatten().copied().map(num));
                program.statements.push(Statement::Call(Call {
                    assigns: vec![name.clone()],
                    name: "building".into(),
                    args,
                }));
                program.statements.push(Statement::Call(Call {
                    assigns: vec![],
                    name: "elevation".into(),
                    args: vec![Argument::Identifier(name), num(height / 49. + 0.001)],
                }));
                occupied.push(corners);
                count += 1;
            }
        }
    }
    count
}
