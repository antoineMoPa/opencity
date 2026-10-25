//! Bind the authored default's curves and paint to the connected road profiles.
use super::profile_binding::{pair_bands, ArmProfile, ArmSide};
use super::*;
use crate::{Program, Statement};
pub fn default_four_way() -> &'static Resolved {
    static DEFAULT: std::sync::OnceLock<Resolved> = std::sync::OnceLock::new();
    DEFAULT.get_or_init(|| {
        let program = crate::parse(include_str!("../../road_library/default_four_way.road"))
            .expect("default intersection parses");
        let Statement::Network(definition) = &program.statements[0] else {
            unreachable!()
        };
        resolve(definition).expect("default intersection resolves")
    })
}
fn add_band(
    geometry: &mut Resolved,
    name: String,
    path: &str,
    material: Material,
    kind: BandKind,
    quads: Vec<[Point; 4]>,
    width: f32,
) {
    geometry.band_meshes.push(BandMesh {
        name: name.clone(),
        quads,
    });
    geometry.bands.push(Band {
        name,
        path: path.into(),
        width,
        kind,
        material,
        side: Side::Center,
    });
}
fn template_face() -> f32 {
    default_four_way()
        .ports
        .iter()
        .map(|port| port.position.x.abs().max(port.position.y.abs()))
        .fold(0., f32::max)
}
pub fn default_four_way_footprint_factor() -> f32 {
    let geometry = default_four_way();
    let profile_half = geometry
        .bands
        .iter()
        .find(|b| b.name == "north_south")
        .unwrap()
        .width
        / 2.
        + geometry
            .bands
            .iter()
            .find(|b| b.name == "corner_road_NW")
            .unwrap()
            .width;
    template_face() / profile_half
}
fn profile_corners(geometry: &mut Resolved, arms: &[ArmProfile], half: f32) {
    let face = template_face();
    let mut pavement = Vec::new();
    for (name, vertical, horizontal, sx, sy) in [
        ("NW", ArmSide::North, ArmSide::West, -1., -1.),
        ("NE", ArmSide::North, ArmSide::East, 1., -1.),
        ("SE", ArmSide::South, ArmSide::East, 1., 1.),
        ("SW", ArmSide::South, ArmSide::West, -1., 1.),
    ] {
        let v = &arms[vertical.index()];
        let h = &arms[horizontal.index()];
        let path_name = format!("curb_{name}");
        let raw = default_four_way().path(&path_name).unwrap();
        let arc: Vec<_> = raw
            .points
            .iter()
            .filter(|p| p.x.abs() <= face + 0.00001 && p.y.abs() <= face + 0.00001)
            .copied()
            .collect();
        let drive_x = if sx > 0. { v.drive_high } else { v.drive_low };
        let drive_y = if sy > 0. { h.drive_high } else { h.drive_low };
        let rx = (half - drive_x.abs()).max(0.);
        let ry = (half - drive_y.abs()).max(0.);
        let radius = (arc[0].x - sx * face)
            .abs()
            .max((arc[0].y - sy * face).abs());
        let mapped = |p: Point, dx: f32, dy: f32| Point {
            x: sx * half + (p.x - sx * face) * (rx - dx).max(0.) / radius,
            y: sy * half + (p.y - sy * face) * (ry - dy).max(0.) / radius,
            z: 0.,
        };
        let inner: Vec<_> = arc.iter().map(|p| mapped(*p, 0., 0.)).collect();
        let horizontal_face = Point {
            x: sx * half,
            y: 0.,
            z: 0.,
        };
        let vertical_face = Point {
            x: 0.,
            y: sy * half,
            z: 0.,
        };
        let (first, last) = if matches!(name, "NW" | "SE") {
            (horizontal_face, vertical_face)
        } else {
            (vertical_face, horizontal_face)
        };
        let boundary: Vec<_> = std::iter::once(first)
            .chain(inner)
            .chain(std::iter::once(last))
            .collect();
        // Fill inward from the curved curb, rather than covering the whole
        // junction square and masking its outside corners with painted grass.
        for pair in boundary.windows(2) {
            pavement.push([
                Point {
                    x: 0.,
                    y: 0.,
                    z: 0.,
                },
                pair[0],
                pair[1],
                pair[1],
            ]);
        }
        let (mut x_offset, mut y_offset) = (0., 0.);
        for (index, (kind, x_width, y_width)) in pair_bands(&v.outer(sx > 0.), &h.outer(sy > 0.))
            .into_iter()
            .enumerate()
        {
            let next_x = x_offset + x_width;
            let next_y = y_offset + y_width;
            let quads = arc
                .windows(2)
                .map(|p| {
                    [
                        mapped(p[0], x_offset, y_offset),
                        mapped(p[1], x_offset, y_offset),
                        mapped(p[1], next_x, next_y),
                        mapped(p[0], next_x, next_y),
                    ]
                })
                .collect();
            add_band(
                geometry,
                format!("{name}_{index}_{}", kind.keyword()),
                &path_name,
                Material::RoadElement(kind),
                BandKind::Surface,
                quads,
                x_width.max(y_width),
            );
            x_offset = next_x;
            y_offset = next_y;
        }
        if let Some(path) = geometry
            .paths
            .iter_mut()
            .find(|path| path.name == path_name)
        {
            path.points = raw
                .points
                .iter()
                .map(|p| {
                    if p.x.abs() > face + 0.00001 {
                        Point {
                            x: p.x * half / face,
                            y: drive_y,
                            z: 0.,
                        }
                    } else if p.y.abs() > face + 0.00001 {
                        Point {
                            x: drive_x,
                            y: p.y * half / face,
                            z: 0.,
                        }
                    } else {
                        mapped(*p, 0., 0.)
                    }
                })
                .collect();
        }
    }
    add_band(
        geometry,
        "junction_pavement".into(),
        "junction_pavement",
        Material::Asphalt,
        BandKind::Surface,
        pavement,
        half * 2.,
    );
}
fn profile_approaches(geometry: &mut Resolved, arms: &[ArmProfile], half: f32) {
    for arm in arms {
        let name = arm.side.name();
        let path = if matches!(arm.side, ArmSide::North | ArmSide::South) {
            "pavement_ns"
        } else {
            "pavement_ew"
        };
        let stub = default_four_way()
            .path(path)
            .unwrap()
            .points
            .iter()
            .map(|p| p.x.abs().max(p.y.abs()))
            .fold(0., f32::max)
            * half
            / template_face();
        let stop_name = format!("stop_{name}");
        let stop = default_four_way().path(&stop_name).unwrap().points[0];
        let stop_distance = stop.x.abs().max(stop.y.abs()) * half / template_face();
        for (index, band) in arm.bands.iter().enumerate() {
            let quads = vec![[
                arm.side.position(band.low, half),
                arm.side.position(band.low, stub),
                arm.side.position(band.high, stub),
                arm.side.position(band.high, half),
            ]];
            add_band(
                geometry,
                format!("approach_{name}_{index}"),
                path,
                Material::RoadElement(band.kind),
                BandKind::Surface,
                quads,
                band.high - band.low,
            );
            if matches!(
                band.kind,
                crate::RoadElementKind::BusLane | crate::RoadElementKind::Middle
            ) {
                add_band(
                    geometry,
                    format!("entry_{name}_{index}"),
                    path,
                    Material::RoadElement(band.kind),
                    BandKind::Surface,
                    vec![[
                        arm.side.position(band.low, half),
                        arm.side.position(band.low, stop_distance),
                        arm.side.position(band.high, stop_distance),
                        arm.side.position(band.high, half),
                    ]],
                    band.high - band.low,
                );
            }
        }
        if let Some((low, high)) = arm.lane_bounds(true) {
            let path_name = format!("stop_{name}");
            let prototype = default_four_way().path(&path_name).unwrap();
            let first = prototype.points[0];
            let distance = first.x.abs().max(first.y.abs()) * half / template_face();
            let inset = ((high - low) * 0.04).min(0.002);
            let points = vec![
                arm.side.position(low + inset, distance),
                arm.side.position(high - inset, distance),
            ];
            let width = default_four_way()
                .bands
                .iter()
                .find(|band| band.name == path_name)
                .unwrap()
                .width
                * half;
            let quads = band_quads(&points, width, Side::Center);
            if let Some(path) = geometry.paths.iter_mut().find(|p| p.name == path_name) {
                path.points = points;
            }
            add_band(
                geometry,
                path_name.clone(),
                &path_name,
                Material::White,
                BandKind::Marking,
                quads,
                width,
            );
        }
    }
}
fn approach_underlays(geometry: &mut Resolved, arms: &[ArmProfile], half: f32) {
    for arm in arms {
        let path = if matches!(arm.side, ArmSide::North | ArmSide::South) {
            "pavement_ns"
        } else {
            "pavement_ew"
        };
        let stub = default_four_way()
            .path(path)
            .unwrap()
            .points
            .iter()
            .map(|p| p.x.abs().max(p.y.abs()))
            .fold(0., f32::max)
            * half
            / template_face();
        add_band(
            geometry,
            format!("underlay_{}", arm.side.name()),
            path,
            Material::Asphalt,
            BandKind::Surface,
            vec![[
                arm.side.position(-arm.half, half * 0.99),
                arm.side.position(-arm.half, stub),
                arm.side.position(arm.half, stub),
                arm.side.position(arm.half, half * 0.99),
            ]],
            arm.half * 2.,
        );
    }
}
fn profile_ports(geometry: &mut Resolved, arms: &[ArmProfile], half: f32) {
    let old_ports = geometry.ports.clone();
    geometry.ports.retain_mut(|port| {
        let Some(arm) = arms.iter().find(|arm| arm.side.name() == port.name) else {
            return false;
        };
        let Some((low, high)) = arm.lane_bounds(port.role == LaneRole::In) else {
            return false;
        };
        port.position = arm.side.position((low + high) / 2., half);
        port.width = Some(high - low);
        true
    });
    for movement in &mut geometry.movements {
        let m = &movement.movement;
        let find = |ports: &[Port], name: &str| {
            ports
                .iter()
                .find(|p| format!("{}.{}", p.name, p.role.keyword()) == name)
                .map(|p| p.position)
        };
        let Some(old_from) = find(&old_ports, &m.from) else {
            continue;
        };
        let Some(old_to) = find(&old_ports, &m.to) else {
            continue;
        };
        let Some(from) = find(&geometry.ports, &m.from) else {
            continue;
        };
        let Some(to) = find(&geometry.ports, &m.to) else {
            continue;
        };
        let count = movement.points.len().saturating_sub(1).max(1) as f32;
        for (index, point) in movement.points.iter_mut().enumerate() {
            let t = index as f32 / count;
            let blend = t * t * (3. - 2. * t);
            point.x += (from.x - old_from.x) * (1. - blend) + (to.x - old_to.x) * blend;
            point.y += (from.y - old_from.y) * (1. - blend) + (to.y - old_to.y) * blend;
        }
        if let Some(step) = m.via.first() {
            if let Some(path) = geometry.paths.iter_mut().find(|p| p.name == step.path) {
                path.points = movement.points.clone();
            }
        }
    }
    geometry.movements.retain(|movement| {
        [&movement.movement.from, &movement.movement.to]
            .iter()
            .all(|name| {
                geometry
                    .ports
                    .iter()
                    .any(|p| format!("{}.{}", p.name, p.role.keyword()) == name.as_str())
            })
    });
}
/// Bind every arm separately, including asymmetric and mixed-width road profiles.
pub fn default_four_way_instance(
    program: &Program,
    projection: &crate::map::Projection,
    intersection: &crate::map::Intersection,
) -> Option<NetworkInstance> {
    if intersection.kind != crate::map::IntersectionKind::FourWay {
        return None;
    }
    let arms: Vec<_> = ArmSide::ALL
        .into_iter()
        .map(|side| ArmProfile::bind(program, projection, intersection, side))
        .collect::<Option<_>>()?;
    let half = crate::map::intersection_footprint_half(
        intersection,
        arms.iter().map(|arm| arm.half).fold(0., f32::max),
    );
    if half <= 0. {
        return None;
    }
    let mut geometry = default_four_way().clone();
    let factor = half / template_face();
    let scale = |p: &mut Point| {
        p.x *= factor;
        p.y *= factor;
        p.z *= factor;
    };
    for path in &mut geometry.paths {
        for p in &mut path.points {
            scale(p);
        }
    }
    for port in &mut geometry.ports {
        scale(&mut port.position);
    }
    for movement in &mut geometry.movements {
        for p in &mut movement.points {
            scale(p);
        }
    }
    geometry.bands.clear();
    geometry.band_meshes.clear();
    approach_underlays(&mut geometry, &arms, half);
    profile_corners(&mut geometry, &arms, half);
    profile_approaches(&mut geometry, &arms, half);
    profile_ports(&mut geometry, &arms, half);
    Some(NetworkInstance {
        id: format!("profile:{}", intersection.point_ref),
        name: intersection.point_ref.clone(),
        definition: usize::MAX,
        origin: Point {
            x: intersection.position.x,
            y: intersection.position.y,
            z: 0.,
        },
        geometry,
    })
}
