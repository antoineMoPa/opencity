//! Legacy residential-area migration into generic street-facing scaffolds.
use super::scaffold_frontage::{self as commercial, Footprint};
use crate::runtime::{City, SurfaceKind};
use crate::{map, Argument, Call, LandscapeKind, Program, RoadElementKind, RoadRole, Statement};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug)]
pub struct Area {
    seed: u64,
    min: [f32; 2],
    max: [f32; 2],
    centre: [f32; 2],
    core_radius: f32,
}
impl Area {
    fn contains(self, point: [f32; 2]) -> bool {
        point[0] >= self.min[0]
            && point[0] <= self.max[0]
            && point[1] >= self.min[1]
            && point[1] <= self.max[1]
            && (point[0] - self.centre[0]).hypot(point[1] - self.centre[1]) >= self.core_radius
    }
}
fn area_from_call(call: &Call) -> Result<Area, String> {
    if call.args.len() != 8 {
        return Err("residential_area expects seed, bounds, centre and core radius".into());
    }
    let number = |i| {
        match &call.args[i] {
            Argument::Number(s) => s.parse::<f32>().ok().filter(|v| v.is_finite()),
            _ => None,
        }
        .ok_or_else(|| {
            format!(
                "residential_area argument {} must be a finite number",
                i + 1
            )
        })
    };
    let seed = match &call.args[0] {
        Argument::Number(s) => s.parse::<u64>().ok(),
        _ => None,
    }
    .ok_or("residential_area seed must be an unsigned integer")?;
    let area = Area {
        seed,
        min: [number(1)?, number(2)?],
        max: [number(3)?, number(4)?],
        centre: [number(5)?, number(6)?],
        core_radius: number(7)?,
    };
    if area.min[0] >= area.max[0] || area.min[1] >= area.max[1] || area.core_radius < 0. {
        return Err(
            "residential_area requires ordered bounds and a nonnegative core radius".into(),
        );
    }
    Ok(area)
}
fn configured_areas(program: &Program) -> Result<Vec<Area>, String> {
    program
        .statements
        .iter()
        .filter_map(|s| match s {
            Statement::Call(c) if c.name == "residential_area" => Some(area_from_call(c)),
            _ => None,
        })
        .collect()
}
/// Remove individually authored buildings in area interiors, while preserving
/// named central cores and any non-building map statements.
fn without_replaced(program: &Program, areas: &[Area], strips: &[Footprint]) -> Program {
    let projected = crate::project_landscape_features(program);
    let names: HashSet<_> = projected
        .features
        .iter()
        .filter(|f| {
            f.kind == LandscapeKind::Building
                && f.model.is_none()
                && f.building_use == crate::BuildingUse::Unspecified
                && f.height.unwrap_or(0.) <= 0.16
        })
        .filter(|f| !strips.iter().any(|q| commercial::overlap(&f.points, q)))
        .filter_map(|f| {
            let n = f.points.len() as f32;
            let p = f
                .points
                .iter()
                .fold([0., 0.], |a, v| [a[0] + v[0] / n, a[1] + v[1] / n]);
            areas
                .iter()
                .any(|area| area.contains(p))
                .then(|| f.name.clone())
                .flatten()
        })
        .collect();
    let statements = program
        .statements
        .iter()
        .filter(|statement| match statement {
            Statement::Call(call) if call.name == "building" => {
                !call.assigns.first().is_some_and(|n| names.contains(n))
            }
            Statement::Call(call) if call.name == "elevation" => {
                !matches!(call.args.first(),Some(Argument::Identifier(n)) if names.contains(n))
            }
            _ => true,
        })
        .cloned()
        .collect();
    Program { statements }
}
fn mix(mut seed: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        seed = (seed ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    seed
}
fn random(seed: &mut u64) -> f32 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*seed >> 40) as u32) as f32 / 16_777_216.
}
fn row_layout(seed: &mut u64) -> (u8, f32) {
    let multiplex = random(seed) < 0.14;
    let floors = if multiplex {
        if random(seed) < 0.5 {
            3
        } else {
            4
        }
    } else {
        match (random(seed) * 10.) as u8 {
            0 => 1,
            1..=7 => 2,
            _ => 3,
        }
    };
    // Small homes usually have a garden; taller rows sometimes meet the sidewalk.
    let setback = if multiplex && random(seed) < 0.8 || !multiplex && random(seed) < 0.10 {
        0.001
    } else {
        0.040 + random(seed) * 0.010
    };
    (floors, setback)
}
fn row_seed(area: Area, road: &crate::runtime::Road, side: usize, group: usize) -> u64 {
    let mut s = area.seed ^ 0xcbf29ce484222325;
    for p in [
        road.segment.start,
        road.segment.end,
        road.segment.controls[0],
        road.segment.controls[1],
    ] {
        s = mix(s, u64::from(p.x.to_bits()));
        s = mix(s, u64::from(p.y.to_bits()));
    }
    mix(mix(s, side as u64), group as u64)
}
fn coord(point: [f32; 2], along: [f32; 2], out: [f32; 2], x: f32, y: f32) -> [f32; 2] {
    [
        point[0] + along[0] * x + out[0] * y,
        point[1] + along[1] * x + out[1] * y,
    ]
}
fn disjoint(a: &[[f32; 2]; 4], b: &[[f32; 2]; 4]) -> bool {
    for poly in [a, b] {
        for i in 0..2 {
            let edge = [poly[i + 1][0] - poly[i][0], poly[i + 1][1] - poly[i][1]];
            let axis = [-edge[1], edge[0]];
            let project = |p: &[f32; 2]| p[0] * axis[0] + p[1] * axis[1];
            let range = |p: &[[f32; 2]; 4]| {
                p.iter()
                    .map(project)
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
                        (lo.min(v), hi.max(v))
                    })
            };
            let (a0, a1) = range(a);
            let (b0, b1) = range(b);
            // Treat tiny rounding differences at a shared wall as contact, not overlap.
            let tolerance = edge[0].hypot(edge[1]) * 0.00003;
            if a1 <= b0 + tolerance || b1 <= a0 + tolerance {
                return true;
            }
        }
    }
    false
}
fn number(value: f32) -> Argument {
    Argument::Number(format!("{value:.7}"))
}
fn home_call(
    name: String,
    corners: [[f32; 2]; 4],
    floors: u8,
    material: &str,
    elevation: f32,
) -> [Statement; 2] {
    let mut args = vec![
        number(floors as f32 * 0.034),
        Argument::Object(vec![
            ("material".into(), Argument::Identifier(material.into())),
            ("floor_height".into(), number(0.034)),
        ]),
    ];
    args.extend(corners.into_iter().flat_map(|p| p.into_iter().map(number)));
    [
        Statement::Call(Call {
            assigns: vec![name.clone()],
            name: "building".into(),
            args,
        }),
        Statement::Call(Call {
            assigns: vec![],
            name: "elevation".into(),
            args: vec![Argument::Identifier(name), number(elevation)],
        }),
    ]
}
/// Prepare the source once before the first City compile. Custom maps can add
/// `residential_area` calls; maps without areas retain their original geometry.
pub fn prepare(program: &Program) -> Result<(Program, Vec<Area>), String> {
    let areas = configured_areas(program)?;
    let stripped = if areas.is_empty() {
        program.clone()
    } else {
        without_replaced(program, &areas, &commercial::reservations(program)?)
    };
    Ok((stripped, areas))
}
/// Add stable street-facing rows to a temporary compiled snapshot of the stripped map.
pub fn populate(program: &mut Program, city: &City, areas: &[Area]) -> usize {
    let existing: Vec<_> = city
        .query(crate::runtime::Region::WholeMap)
        .buildings
        .iter()
        .map(|(_, f)| f.points.clone())
        .collect();
    let reservations = commercial::reservations(program).expect("validated zones");
    let mut occupied: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    let mut homes: Vec<[[f32; 2]; 4]> = Vec::new();
    for (area_index, &area) in areas.iter().enumerate() {
        for road in &city.roads {
            if road.definition.role != RoadRole::Ordinary
                || !road
                    .definition
                    .elements
                    .iter()
                    .any(|e| e.kind == RoadElementKind::CarLane)
            {
                continue;
            }
            let mut length = 0.;
            for i in 0..24 {
                let a = road.segment.point_at(i as f32 / 24.);
                let b = road.segment.point_at((i + 1) as f32 / 24.);
                length += (a.x - b.x).hypot(a.y - b.y);
            }
            let count = ((length - 0.08) / 0.075).floor().clamp(0., 2000.) as usize;
            if count == 0 {
                continue;
            }
            for (side, _) in road
                .definition
                .elements
                .iter()
                .enumerate()
                .filter(|(_, e)| e.kind == RoadElementKind::Sidewalk)
            {
                for lot in 0..count {
                    let t = (lot as f32 + 0.5) / count as f32;
                    let segment = &road.segment;
                    let p = segment.point_at(t);
                    let tangent = segment.tangent_at(t);
                    let l = tangent.x.hypot(tangent.y);
                    if l < 1e-5 {
                        continue;
                    }
                    let Some(sidewalk) = map::element_position(segment, &road.definition, side, t)
                    else {
                        continue;
                    };
                    let side_x = sidewalk.x - p.x;
                    let side_y = sidewalk.y - p.y;
                    let side_len = side_x.hypot(side_y);
                    if side_len < 1e-5 {
                        continue;
                    }
                    let along = [tangent.x / l, tangent.y / l];
                    let out = [side_x / side_len, side_y / side_len];
                    let mut group = row_seed(area, road, side, lot / 6);
                    let mut individual = row_seed(area, road, side, lot + 10_000);
                    let start = segment.point_at(lot as f32 / count as f32);
                    let end = segment.point_at((lot + 1) as f32 / count as f32);
                    let lot_width = (end.x - start.x).hypot(end.y - start.y);
                    // Most lots fill their frontage; a few leave room for a passage.
                    let width = if random(&mut individual) < 0.015 {
                        lot_width * 0.80
                    } else {
                        lot_width
                    };
                    let depth = 0.085 + random(&mut group) * 0.022;
                    let (floors, setback) = row_layout(&mut group);
                    let front = [
                        sidewalk.x + out[0] * (0.016 + setback),
                        sidewalk.y + out[1] * (0.016 + setback),
                    ];
                    let corners = [
                        coord(front, along, out, -width / 2., 0.),
                        coord(front, along, out, width / 2., 0.),
                        coord(front, along, out, width / 2., depth),
                        coord(front, along, out, -width / 2., depth),
                    ];
                    if reservations
                        .iter()
                        .any(|q| commercial::overlap(&corners, q))
                        || !corners.iter().all(|&p| area.contains(p))
                    {
                        continue;
                    }
                    if existing.iter().any(|p| commercial::overlap(&corners, p)) {
                        continue;
                    }
                    let centre = coord(front, along, out, 0., depth * 0.5);
                    let key = (
                        (centre[0] / 0.25).floor() as i32,
                        (centre[1] / 0.25).floor() as i32,
                    );
                    if (-1..=1).any(|dx| {
                        (-1..=1).any(|dy| {
                            occupied
                                .get(&(key.0 + dx, key.1 + dy))
                                .is_some_and(|indices| {
                                    indices.iter().any(|&i| !disjoint(&corners, &homes[i]))
                                })
                        })
                    }) {
                        continue;
                    }
                    if city.building_collision(
                        centre,
                        width * 0.48,
                        segment.height_at(t) - 0.02,
                        segment.height_at(t) + 0.15,
                    ) {
                        continue;
                    }
                    let mut zsum = 0.;
                    let mut valid = true;
                    for point in corners.into_iter().chain([centre]) {
                        match city.surface_at(point, segment.height_at(t) + 0.02) {
                            Some(s)
                                if s.kind == SurfaceKind::Grass
                                    && (s.height - segment.height_at(t)).abs() < 0.025 =>
                            {
                                zsum += s.height
                            }
                            _ => {
                                valid = false;
                                break;
                            }
                        }
                    }
                    if !valid {
                        continue;
                    }

                    let id = row_seed(area, road, side, lot + 20_000);
                    let name = format!("generated_home_{area_index}_{id:016x}");
                    program.statements.extend(home_call(
                        name,
                        corners,
                        floors,
                        "concrete",
                        zsum / 5. + 0.002,
                    ));
                    occupied.entry(key).or_default().push(homes.len());
                    homes.push(corners);
                }
            }
        }
    }
    homes.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Region;

    fn sample(seed: u64) -> Program {
        crate::parse(&format!(
            "road street: sidewalk carlane sidewalk; end \
             street(-2,0,2,0); \
             ground = land(-3,-2,3,-2,3,2,-3,2); elevation(ground,0.02); \
             old = building(0.14, -1.4,0.1,-1.2,0.1,-1.2,0.25,-1.4,0.25); \
             core = building(0.30, -0.12,0.35,0.12,0.35,0.12,0.55,-0.12,0.55); \
             residential_area({seed},-2,-1,2,1,0,0.45,0.3);"
        ))
        .unwrap()
    }
    fn expanded(source: &Program) -> (Program, usize) {
        let (mut program, areas) = prepare(source).unwrap();
        let city = City::from_program(&program, None).unwrap();
        let count = populate(&mut program, &city, &areas);
        (program, count)
    }
    #[test]
    fn homes_are_seeded_repeatable_replace_outer_buildings_and_preserve_core() {
        let (first, count) = expanded(&sample(42));
        let (second, again) = expanded(&sample(42));
        assert!(count >= 12, "expected roadside homes, got {count}");
        assert_eq!(count, again);
        assert_eq!(first.to_road_source(), second.to_road_source());
        let source = first.to_road_source();
        assert!(!source.contains("old = building"));
        assert!(source.contains("core = building"));
        assert_ne!(source, expanded(&sample(43)).0.to_road_source());
        let city = City::from_program(&first, None).unwrap();
        let homes: Vec<_> = city
            .query(Region::WholeMap)
            .buildings
            .into_iter()
            .filter(|(_, f)| {
                f.name
                    .as_ref()
                    .is_some_and(|n| n.starts_with("generated_home_"))
            })
            .collect();
        assert_eq!(homes.len(), count);
        let attached = homes
            .iter()
            .filter(|(_, home)| {
                homes.iter().any(|(_, other)| {
                    if std::ptr::eq(*home, *other) {
                        return false;
                    }
                    let side_meets = (home.points[0][0] - other.points[1][0]).abs() < 0.0001
                        || (home.points[1][0] - other.points[0][0]).abs() < 0.0001;
                    let y_range = |points: &[[f32; 2]]| {
                        points
                            .iter()
                            .fold((f32::INFINITY, f32::NEG_INFINITY), |r, p| {
                                (r.0.min(p[1]), r.1.max(p[1]))
                            })
                    };
                    let a = y_range(&home.points);
                    let b = y_range(&other.points);
                    side_meets && a.0.max(b.0) < a.1.min(b.1)
                })
            })
            .count();
        assert!(
            attached * 100 >= count * 90,
            "{attached}/{count} homes attached"
        );
        for (_, home) in homes {
            assert!(home.height.unwrap() <= 0.137);
            let p = home
                .points
                .iter()
                .fold([0., 0.], |a, p| [a[0] + p[0] * 0.25, a[1] + p[1] * 0.25]);
            assert!(city.building_collision(p, 0., home.elevation, home.elevation + 0.02));
        }
    }
    #[test]
    fn maps_without_areas_are_unchanged_and_invalid_areas_fail() {
        let input = crate::parse("road local: carlane; end local(0,0,1,0);").unwrap();
        let (out, areas) = prepare(&input).unwrap();
        assert!(areas.is_empty());
        assert_eq!(out, input);
        for call in [
            "residential_area(1,0,0);",
            "residential_area(-1,0,0,1,1,0,0,0);",
            "residential_area(1,1,0,0,1,0,0,0);",
        ] {
            assert!(prepare(&crate::parse(call).unwrap()).is_err(), "{call}");
        }
    }

    #[test]
    fn low_rise_rows_usually_leave_garden_space_and_taller_rows_can_be_flush() {
        let mut low = 0;
        let mut gardens = 0;
        let mut flush_four = 0;
        for seed in 0..10_000 {
            let mut row = mix(0xcbf29ce484222325, seed);
            let (floors, setback) = row_layout(&mut row);
            if floors <= 2 {
                low += 1;
                if setback >= 0.04 {
                    gardens += 1;
                }
            }
            if floors == 4 && setback < 0.002 {
                flush_four += 1;
            }
        }
        assert!(
            gardens * 100 >= low * 80,
            "{gardens}/{low} low-rise rows have gardens"
        );
        assert!(flush_four > 0);
    }
    #[test]
    fn unrelated_commercial_zones_retain_legacy_residential_replacement() {
        let mut p = sample(42);
        let extra = crate::parse("tower = building(0.4,1.1,0.1,1.3,0.1,1.3,0.3,1.1,0.3);").unwrap();
        p.statements.extend(extra.statements);
        let (baseline, _) = prepare(&p).unwrap();
        let baseline = crate::project_landscape_features(&baseline).features;
        assert!(baseline.iter().any(|f| f.name.as_deref() == Some("tower")));
        let extra = crate::parse(
            "road far_street: sidewalk carlane sidewalk; end
            far = far_street(-2,4,2,4);
            far_zone = frontage_zone(far,{use:commercial,side:both,
                start:0.1,end:0.9,depth:0.16,seed:3});",
        )
        .unwrap();
        p.statements.extend(extra.statements);
        let (actual, _) = prepare(&p).unwrap();
        assert_eq!(
            baseline,
            crate::project_landscape_features(&actual).features
        );
    }
}
