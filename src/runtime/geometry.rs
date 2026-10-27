use super::*;
use crate::{
    network::{BandKind, Material},
    road_geometry::RoadGeometry,
    RoadElementKind as K,
};
const CELL: f32 = 0.5;
pub(super) fn cell(p: [f32; 2]) -> (i32, i32) {
    ((p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32)
}
pub(super) fn bounds(points: impl IntoIterator<Item = [f32; 2]>) -> [f32; 4] {
    points.into_iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |b, p| {
            [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ]
        },
    )
}
pub(super) fn contains(poly: &[[f32; 2]], p: [f32; 2]) -> bool {
    let mut inside = false;
    for (a, b) in poly
        .iter()
        .zip(poly.iter().cycle().skip(1))
        .take(poly.len())
    {
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}
fn distance_squared(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1])
        / (d[0] * d[0] + d[1] * d[1]).max(1e-20))
    .clamp(0., 1.);
    (p[0] - a[0] - d[0] * t).powi(2) + (p[1] - a[1] - d[1] * t).powi(2)
}
pub(super) fn polygon_region(poly: &[[f32; 2]], r: Region) -> bool {
    if poly.is_empty() || !r.intersects_bounds(bounds(poly.iter().copied())) {
        return false;
    }
    match r {
        Region::WholeMap => true,
        Region::Radius { center, radius } => {
            contains(poly, center)
                || poly
                    .iter()
                    .zip(poly.iter().cycle().skip(1))
                    .take(poly.len())
                    .any(|(&a, &b)| distance_squared(center, a, b) <= radius * radius)
        }
    }
}
pub(super) fn path_region(path: &[[f32; 3]], r: Region) -> bool {
    if !r.valid() {
        return false;
    }
    match r {
        Region::WholeMap => !path.is_empty(),
        Region::Radius { center, radius } => path.windows(2).any(|p| {
            distance_squared(center, [p[0][0], p[0][1]], [p[1][0], p[1][1]]) <= radius * radius
        }),
    }
}
pub(super) fn triangle_height(v: [[f32; 3]; 3], p: [f32; 2]) -> Option<(f32, [f32; 2])> {
    let [a, b, c] = v;
    let d = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    if d.abs() < 1e-16 {
        return None;
    }
    let u = ((p[0] - a[0]) * (c[1] - a[1]) - (p[1] - a[1]) * (c[0] - a[0])) / d;
    let w = ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])) / d;
    if u < -1e-4 || w < -1e-4 || u + w > 1.0001 {
        return None;
    }
    let g = [
        ((b[2] - a[2]) * (c[1] - a[1]) - (c[2] - a[2]) * (b[1] - a[1])) / d,
        ((b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2])) / d,
    ];
    Some((a[2] + u * (b[2] - a[2]) + w * (c[2] - a[2]), g))
}
pub(super) fn kind(k: K) -> SurfaceKind {
    match k {
        K::Sidewalk | K::BusStop | K::BuildingEdge | K::Barrier => SurfaceKind::Sidewalk,
        K::Verge | K::Middle => SurfaceKind::Grass,
        _ => SurfaceKind::Asphalt,
    }
}
fn material(m: Material) -> SurfaceKind {
    match m {
        Material::RoadElement(k) => kind(k),
        Material::Sidewalk | Material::Concrete => SurfaceKind::Sidewalk,
        Material::Verge => SurfaceKind::Grass,
        Material::White | Material::Yellow | Material::Red | Material::Blue => SurfaceKind::Marking,
        _ => SurfaceKind::Asphalt,
    }
}
fn color(c: [u8; 3]) -> [f32; 3] {
    c.map(|v| (v as f32 / 255.).powf(2.2))
}
pub(super) fn push(
    city: &mut City,
    p: [[f32; 3]; 3],
    kind: SurfaceKind,
    road: Option<usize>,
    rgb: [u8; 3],
    data: [f32; 3],
) {
    city.triangles.push(Triangle {
        points: p,
        kind,
        road,
        color: color(rgb),
        data,
    });
}
fn quad(city: &mut City, p: [[f32; 3]; 4], kind: SurfaceKind, road: Option<usize>, rgb: [u8; 3]) {
    for i in [[0, 1, 2], [0, 2, 3]] {
        push(city, i.map(|i| p[i]), kind, road, rgb, [0.; 3]);
    }
}
fn polygon(
    city: &mut City,
    points: &[[f32; 2]],
    height: f32,
    kind: SurfaceKind,
    rgb: [u8; 3],
    data: [f32; 3],
) -> Result<(), String> {
    let coords: Vec<_> = points.iter().flat_map(|p| p.map(f64::from)).collect();
    for tri in earcutr::earcut(&coords, &[], 2)
        .map_err(|e| format!("polygon tessellation: {e:?}"))?
        .chunks_exact(3)
    {
        push(
            city,
            [tri[0], tri[1], tri[2]].map(|i| [points[i][0], points[i][1], height]),
            kind,
            None,
            rgb,
            data,
        );
    }
    Ok(())
}
pub(super) fn compile(
    city: &mut City,
    program: &Program,
    projection: &map::Projection,
) -> Result<(), String> {
    let geometry = RoadGeometry::compile(program, projection);
    if !geometry.unresolved.is_empty() {
        return Err("road subdivision budget exceeded".into());
    }
    for (id, seg) in projection.segments.iter().enumerate() {
        let road = crate::road_profiles::road_for_segment(program, seg)
            .ok_or_else(|| format!("missing road {}", seg.road_name))?;
        let rows = geometry.cross_sections(id);
        let start = city.triangles.len();
        let mut distance = 0.;
        for pair in rows.windows(2) {
            for (band, e) in road.elements.iter().enumerate() {
                let k = kind(e.kind);
                let raised = if k == SurfaceKind::Sidewalk {
                    0.0015
                } else {
                    0.
                };
                let p = [
                    pair[0].boundaries[band],
                    pair[1].boundaries[band],
                    pair[1].boundaries[band + 1],
                    pair[0].boundaries[band + 1],
                ]
                .map(|v| [v.point.x, v.point.y, v.height + raised]);
                quad(city, p, k, Some(id), network::road_color(e.kind, false));
                // Vertical curb faces share the pavement's subdivision and slopes.
                if k == SurfaceKind::Sidewalk {
                    for (a, b) in [(p[0], p[1]), (p[3], p[2])] {
                        quad(
                            city,
                            [
                                a,
                                b,
                                [b[0], b[1], b[2] - raised],
                                [a[0], a[1], a[2] - raised],
                            ],
                            k,
                            Some(id),
                            [118, 122, 124],
                        );
                    }
                }
            }
            let a = seg.point_at(pair[0].t);
            let b = seg.point_at(pair[1].t);
            distance += (b.x - a.x).hypot(b.y - a.y);
            // Stripe geometry follows exactly the same adaptive surface rows.
            for edge in 1..road.elements.len() {
                let left = road.elements[edge - 1].kind;
                let right = road.elements[edge].kind;
                let is_lane =
                    |k| matches!(k, K::CarLane | K::BusLane | K::ParkingLane | K::BikeLane);
                if !is_lane(left) && !is_lane(right) {
                    continue;
                }
                let both = is_lane(left) && is_lane(right);
                if both && distance % 0.09 > 0.035 {
                    continue;
                }
                let stripe = |row: &crate::road_geometry::CrossSection, sign: f32| {
                    let v = row.boundaries[edge];
                    let a = row.boundaries[0].point;
                    let b = row.boundaries.last().unwrap().point;
                    let d = (b.x - a.x).hypot(b.y - a.y).max(1e-8);
                    [
                        v.point.x + (b.x - a.x) / d * sign * 0.0005,
                        v.point.y + (b.y - a.y) / d * sign * 0.0005,
                        v.height + 0.00025,
                    ]
                };
                let rgb = if left == K::Middle || right == K::Middle {
                    [235, 184, 67]
                } else {
                    [222, 223, 210]
                };
                quad(
                    city,
                    [
                        stripe(&pair[0], -1.),
                        stripe(&pair[1], -1.),
                        stripe(&pair[1], 1.),
                        stripe(&pair[0], 1.),
                    ],
                    SurfaceKind::Marking,
                    Some(id),
                    rgb,
                );
            }
            // Short grass shoulders connect the authored raised pavement to terrain.
            // Large drops are bridge decks and retain the open space below them.
            if let Some(elevation) = city.elevation.as_ref() {
                let mut shoulders = Vec::new();
                for side in [0, road.elements.len()] {
                    let outward = |row: &crate::road_geometry::CrossSection| {
                        let a = row.boundaries[0];
                        let b = *row.boundaries.last().unwrap();
                        let v = row.boundaries[side];
                        let length = (b.point.x - a.point.x)
                            .hypot(b.point.y - a.point.y)
                            .max(1e-8);
                        let sign = if side == 0 { -1. } else { 1. };
                        [
                            v.point.x + sign * (b.point.x - a.point.x) / length * 0.035,
                            v.point.y + sign * (b.point.y - a.point.y) / length * 0.035,
                        ]
                    };
                    let a = pair[0].boundaries[side];
                    let b = pair[1].boundaries[side];
                    let pa = outward(&pair[0]);
                    let pb = outward(&pair[1]);
                    if let (Some(ha), Some(hb)) = (elevation.height_at(pa), elevation.height_at(pb))
                    {
                        let dry = elevation
                            .spec
                            .water_level
                            .is_none_or(|level| ha > level && hb > level);
                        if dry && (a.height - ha).abs() < 0.015 && (b.height - hb).abs() < 0.015 {
                            shoulders.push([
                                [a.point.x, a.point.y, a.height],
                                [b.point.x, b.point.y, b.height],
                                [pb[0], pb[1], hb],
                                [pa[0], pa[1], ha],
                            ]);
                        }
                    }
                }
                for p in shoulders {
                    quad(city, p, SurfaceKind::Grass, Some(id), [92, 127, 70]);
                }
            }
        }
        let b = bounds(
            city.triangles[start..]
                .iter()
                .flat_map(|t| t.points.map(|p| [p[0], p[1]])),
        );
        city.roads.push(Road {
            segment: seg.clone(),
            definition: road.clone(),
            bounds: b,
            triangles: start..city.triangles.len(),
        });
        city.paths.push(RoutePath {
            name: seg.name.clone().unwrap_or_else(|| format!("road:{id}")),
            points: rows
                .iter()
                .map(|r| {
                    let p = seg.point_at(r.t);
                    [p.x, p.y, seg.height_at(r.t)]
                })
                .collect(),
            component: seg.road_name.clone(),
        });
    }
    for j in &city.junctions.clone() {
        if let Some(mut instance) = network::default_four_way_instance(program, projection, j)
            .or_else(|| network::default_three_way_instance(program, projection, j))
            .or_else(|| network::default_right_turn_instance(program, projection, j))
        {
            // Native templates live at zero; use connected road heights at the seam.
            let heights: Vec<_> = projection
                .segments
                .iter()
                .flat_map(|s| [(&s.start_ref, s.start_z), (&s.end_ref, s.end_z)])
                .filter(|(r, _)| {
                    r.as_ref().is_some_and(|r| {
                        r == &j.point_ref || j.ports.iter().any(|p| &p.point_ref == r)
                    })
                })
                .map(|(_, h)| h)
                .collect();
            instance.origin.z = heights.iter().sum::<f32>() / heights.len().max(1) as f32;
            city.components.push(instance);
        } else {
            // Three-arm junction: continuous paved footprint, closed-side sidewalk.
            let road = program
                .statements
                .iter()
                .find_map(|s| match s {
                    crate::Statement::Road(r) if r.name == j.road_name => Some(r),
                    _ => None,
                })
                .ok_or("junction profile missing")?;
            let half = map::intersection_footprint_half(
                j,
                road.elements
                    .iter()
                    .map(|e| map::element_world_width(e.kind))
                    .sum::<f32>()
                    * 0.5,
            );
            let h = projection
                .segments
                .iter()
                .find_map(|s| {
                    if s.start_ref.as_ref().is_some_and(|r| {
                        r == &j.point_ref || j.ports.iter().any(|p| &p.point_ref == r)
                    }) {
                        Some(s.start_z)
                    } else if s.end_ref.as_ref().is_some_and(|r| {
                        r == &j.point_ref || j.ports.iter().any(|p| &p.point_ref == r)
                    }) {
                        Some(s.end_z)
                    } else {
                        None
                    }
                })
                .unwrap_or(0.);
            let x = j.position.x;
            let y = j.position.y;
            quad(
                city,
                [
                    [x - half, y - half, h],
                    [x + half, y - half, h],
                    [x + half, y + half, h],
                    [x - half, y + half, h],
                ],
                SurfaceKind::Asphalt,
                None,
                network::road_color(K::CarLane, false),
            );
            for d in [[1., 0.], [-1., 0.], [0., 1.], [0., -1.]] {
                if !j
                    .ports
                    .iter()
                    .any(|p| p.direction.x * d[0] + p.direction.y * d[1] > 0.9)
                {
                    let w = 0.032_f32.min(half);
                    let a = [
                        x + d[0] * half - d[1] * half,
                        y + d[1] * half + d[0] * half,
                        h + 0.0015,
                    ];
                    let b = [
                        x + d[0] * half + d[1] * half,
                        y + d[1] * half - d[0] * half,
                        h + 0.0015,
                    ];
                    quad(
                        city,
                        [
                            a,
                            b,
                            [b[0] - d[0] * w, b[1] - d[1] * w, b[2]],
                            [a[0] - d[0] * w, a[1] - d[1] * w, a[2]],
                        ],
                        SurfaceKind::Sidewalk,
                        None,
                        network::road_color(K::Sidewalk, false),
                    );
                }
            }
        }
    }
    for instance in &city.components.clone() {
        let native = city
            .junctions
            .iter()
            .find(|j| instance.id == format!("profile:{}", j.point_ref));
        let mut seams = Vec::new();
        if let Some(j) = native {
            for (i, s) in projection.segments.iter().enumerate() {
                for (reference, end) in [(&s.start_ref, false), (&s.end_ref, true)] {
                    if reference.as_ref().is_some_and(|r| {
                        r == &j.point_ref || j.ports.iter().any(|p| &p.point_ref == r)
                    }) {
                        let row = if end {
                            geometry.cross_sections(i).last()
                        } else {
                            geometry.cross_sections(i).first()
                        };
                        if let Some(row) = row {
                            if let (Some(a), Some(b)) =
                                (row.boundaries.first(), row.boundaries.last())
                            {
                                seams.push((
                                    [a.point.x, a.point.y],
                                    [b.point.x, b.point.y],
                                    a.height,
                                ));
                            }
                        }
                    }
                }
            }
        }
        let world = |p: network::Point| {
            let p = instance.world_point(p);
            let mut weighted = 0.;
            let mut weight = 0.;
            for &(a, b, height) in &seams {
                let d = distance_squared([p.x, p.y], a, b);
                if d < 1e-12 {
                    return [p.x, p.y, height];
                }
                weighted += height / d;
                weight += 1. / d;
            }
            [p.x, p.y, if weight > 0. { weighted / weight } else { p.z }]
        };
        for path in &instance.geometry.paths {
            city.paths.push(RoutePath {
                name: path.name.clone(),
                component: instance.name.clone(),
                points: path.points.iter().map(|&p| world(p)).collect(),
            });
        }
        let half = native.map(|j| {
            let width = std::iter::once(j.road_name.as_str())
                .chain(j.arm_roads.iter().map(|(_, name)| name.as_str()))
                .filter_map(|name| {
                    program.statements.iter().find_map(|s| match s {
                        crate::Statement::Road(r) if r.name == name => Some(
                            r.elements
                                .iter()
                                .map(|e| map::element_world_width(e.kind))
                                .sum::<f32>()
                                * 0.5,
                        ),
                        _ => None,
                    })
                })
                .fold(0., f32::max);
            map::intersection_footprint_half(j, width)
        });
        for patch in super::junction_mesh::patches(&instance.geometry, half) {
            let k = material(patch.material);
            let offset = if patch.kind == BandKind::Marking {
                0.0002
            } else if k == SurfaceKind::Sidewalk {
                0.0015
            } else {
                0.
            };
            let points: Vec<_> = patch
                .points
                .into_iter()
                .map(|p| {
                    let p = world(p);
                    [p[0], p[1], p[2] + offset]
                })
                .collect();
            for i in 1..points.len().saturating_sub(1) {
                push(
                    city,
                    [points[0], points[i], points[i + 1]],
                    k,
                    None,
                    patch.material.rgb(),
                    [0.; 3],
                );
            }
        }
    }
    for f in city.landscape.clone() {
        let k = match f.kind {
            LandscapeKind::Building => SurfaceKind::Building,
            LandscapeKind::Concrete => SurfaceKind::Sidewalk,
            LandscapeKind::Beach => SurfaceKind::Sand,
            LandscapeKind::Lake | LandscapeKind::River => SurfaceKind::Water,
            _ => SurfaceKind::Grass,
        };
        if f.kind == LandscapeKind::Vegetation {
            continue;
        }
        let style = f.building_style.unwrap_or_default();
        let rgb = match k {
            SurfaceKind::Building => style.material.roof_rgb(),
            SurfaceKind::Sidewalk => [148, 153, 154],
            SurfaceKind::Water => [48, 111, 133],
            SurfaceKind::Sand => [185, 172, 130],
            _ => [92, 127, 70],
        };
        let base = if k == SurfaceKind::Building {
            f.elevation
        } else {
            f.elevation - 0.02
        };
        polygon(
            city,
            &f.points,
            base + f.height.unwrap_or(0.),
            k,
            rgb,
            [base, style.floor_height, style.material.shader_id() as f32],
        )?;
        if k == SurfaceKind::Building {
            for (a, b) in f
                .points
                .iter()
                .zip(f.points.iter().cycle().skip(1))
                .take(f.points.len())
            {
                let top = base + f.height.unwrap_or(0.);
                let p = [
                    [a[0], a[1], base],
                    [b[0], b[1], base],
                    [b[0], b[1], top],
                    [a[0], a[1], top],
                ];
                for i in [[0, 1, 2], [0, 2, 3]] {
                    push(
                        city,
                        i.map(|i| p[i]),
                        k,
                        None,
                        style.material.wall_rgb(),
                        [base, style.floor_height, style.material.shader_id() as f32],
                    );
                }
            }
        }
    }
    city.bounds = city
        .elevation
        .as_ref()
        .map(|e| {
            [
                e.spec.origin[0],
                e.spec.origin[1],
                e.spec.origin[0] + e.spec.size[0],
                e.spec.origin[1] + e.spec.size[1],
            ]
        })
        .unwrap_or_else(|| {
            bounds(
                city.triangles
                    .iter()
                    .flat_map(|t| t.points.map(|p| [p[0], p[1]])),
            )
        });
    if !city.bounds.iter().all(|v| v.is_finite()) {
        city.bounds = [-1., -1., 1., 1.];
    }
    for (i, t) in city.triangles.iter().enumerate() {
        let b = bounds(t.points.map(|p| [p[0], p[1]]));
        let a = cell([b[0], b[1]]);
        let z = cell([b[2], b[3]]);
        for x in a.0..=z.0 {
            for y in a.1..=z.1 {
                city.cells.entry((x, y)).or_default().push(i);
            }
        }
    }
    Ok(())
}
