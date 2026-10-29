//! Configurable low-rise building details. All dimensions are in map units.
//!
//! The consumer supplies the patterns and seed. This module makes no assumptions
//! about a city's identity, neighbourhoods, or map-unit scale.
use super::building_frontage::{front_for, Front};
use super::{geometry, City, Mesh, Region, SurfaceKind, Vertex};
use crate::{LandscapeFeature, LandscapeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacadeKind {
    Compact,
    Cottage,
    Rowhouse,
    Stacked,
    SimpleStacked,
    WideStacked,
    Detached,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoofKind {
    Coping,
    Cornice,
    Parapet,
    Pitched,
    Mansard,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StairKind {
    None,
    Straight,
    Curved,
    QuarterTurn,
}

/// Optional visual boundaries and objects chosen by the consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GardenEdge {
    Open,
    StoneWall,
    BrickWall,
    IronFence,
    ConcreteEdging,
    WireFence,
    Hedge,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GardenProp {
    None,
    Planter,
    Bench,
    Bin,
    Bicycle,
}

/// Shader patterns for two-triangle fence panels; values map to surface metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FencePattern {
    IronPickets = 0,
    ChainLink = 1,
}

/// One weighted consumer-supplied pattern, selected only for matching floor counts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildingPattern {
    pub floors: u8,
    pub weight: u16,
    pub facade: FacadeKind,
    pub roof: RoofKind,
    pub stair: StairKind,
    pub trim: [f32; 3],
    pub metal: [f32; 3],
    pub door: [f32; 3],
}

/// Dimensions and choices are supplied by the host; a zero weight excludes a pattern.
#[derive(Clone, Debug)]
pub struct BuildingDetailConfig {
    pub seed: u64,
    pub patterns: Vec<BuildingPattern>,
    /// Maximum height and minimum facade length eligible for these details.
    pub maximum_height: f32,
    pub minimum_frontage: f32,
    /// Distance used when orienting a facade toward the nearest road.
    pub road_reach: f32,
    /// The size of the units used by the pattern, in map coordinates.
    pub unit: f32,
    /// Maximum width in front of a facade used for visual garden features.
    pub garden_depth: f32,
    /// Probability that a clear frontage receives garden details, between zero and one.
    pub garden_chance: f32,
    /// Independent weighted choices for frontage boundaries and visual objects.
    pub garden_edges: Vec<(GardenEdge, u16)>,
    pub garden_props: Vec<(GardenProp, u16)>,
}

fn seed_for(feature: &LandscapeFeature, seed: u64) -> u64 {
    let mut hash = seed ^ 0xcbf29ce484222325;
    // Hash coordinates instead of collection indices, so region queries and map
    // insertions do not change the appearance of existing footprints.
    for p in &feature.points {
        for value in p {
            for byte in value.to_bits().to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
    }
    hash
}
fn random(seed: &mut u64) -> f32 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*seed >> 40) as u32) as f32 / 16_777_216.
}
fn pattern<'a>(
    config: &'a BuildingDetailConfig,
    floors: u8,
    seed: &mut u64,
) -> Option<&'a BuildingPattern> {
    let total: u32 = config
        .patterns
        .iter()
        .filter(|p| p.floors == floors)
        .map(|p| u32::from(p.weight))
        .sum();
    if total == 0 {
        return None;
    }
    let mut choice = (random(seed) * total as f32) as u32;
    for p in config
        .patterns
        .iter()
        .filter(|p| p.floors == floors && p.weight > 0)
    {
        if choice < u32::from(p.weight) {
            return Some(p);
        }
        choice -= u32::from(p.weight);
    }
    None
}
fn choose<T: Copy>(choices: &[(T, u16)], seed: &mut u64) -> Option<T> {
    let total: u64 = choices.iter().map(|(_, weight)| u64::from(*weight)).sum();
    if total == 0 {
        return None;
    }
    let mut value = (random(seed) * total as f32) as u64;
    for &(item, weight) in choices {
        if value < u64::from(weight) {
            return Some(item);
        }
        value -= u64::from(weight);
    }
    None
}

fn panel(
    mesh: &mut Mesh,
    x: f32,
    y: f32,
    z: f32,
    w: f32,
    h: f32,
    color: [f32; 3],
    kind: SurfaceKind,
) {
    mesh.quad(
        [[x, y, z], [x, y, z + h], [x + w, y, z + h], [x + w, y, z]],
        color,
        kind,
    );
}
fn box_at(mesh: &mut Mesh, x: f32, y: f32, z: f32, w: f32, d: f32, h: f32, color: [f32; 3]) {
    if w > 0. && d > 0. && h > 0. {
        mesh.cuboid([x, y, z], [x + w, y + d, z + h], color, SurfaceKind::Prop);
    }
}
fn window(mesh: &mut Mesh, x: f32, z: f32, w: f32, h: f32, u: f32, trim: [f32; 3], lit_seed: u64) {
    let glass = if lit_seed % 5 == 0 {
        [0.025, 0.05, 0.055]
    } else {
        [0.065, 0.105, 0.10]
    };
    panel(mesh, x, u * 0.10, z, w, h, glass, SurfaceKind::Window);
    // Keep a single pane (six vertices). Interpolated UVs span the whole room,
    // including both halves of the mullion; the packed value stays flat.
    let packed_room = 16. * (1 + lit_seed % 256) as f32 + (w / h).clamp(0.125, 15.);
    let pane_start = mesh.vertices.len() - 6;
    for vertex in &mut mesh.vertices[pane_start..] {
        vertex.surface = [
            SurfaceKind::Window as u32 as f32,
            ((vertex.position[0] - x) / w).clamp(0., 1.),
            ((vertex.position[2] - z) / h).clamp(0., 1.),
            packed_room,
        ];
    }
    let t = u * 0.075;
    for (xx, zz, ww, hh) in [
        (x - t, z - t, w + 2. * t, t),
        (x - t, z + h, w + 2. * t, t),
        (x - t, z, t, h),
        (x + w, z, t, h),
        (x + w * 0.5 - t * 0.5, z, t, h),
        (x, z + h * 0.5 - t * 0.5, w, t),
    ] {
        panel(mesh, xx, u * 0.14, zz, ww, hh, trim, SurfaceKind::Prop);
    }
    box_at(
        mesh,
        x - t,
        u * 0.05,
        z - t * 1.7,
        w + 2. * t,
        u * 0.20,
        t * 0.7,
        trim,
    );
}
fn door(mesh: &mut Mesh, x: f32, z: f32, w: f32, h: f32, u: f32, p: &BuildingPattern) {
    panel(mesh, x, u * 0.12, z, w, h, p.door, SurfaceKind::Prop);
    panel(
        mesh,
        x + w * 0.1,
        u * 0.16,
        z + h * 0.65,
        w * 0.8,
        h * 0.24,
        [0.12, 0.23, 0.25],
        SurfaceKind::Window,
    );
    let t = u * 0.07;
    for (xx, zz, ww, hh) in [
        (x - t, z, t, h),
        (x + w, z, t, h),
        (x - t, z + h, w + 2. * t, t),
    ] {
        panel(mesh, xx, u * 0.18, zz, ww, hh, p.trim, SurfaceKind::Prop);
    }
}
fn fence_plane(
    mesh: &mut Mesh,
    start: [f32; 2],
    end: [f32; 2],
    z: f32,
    height: f32,
    unit: f32,
    color: [f32; 3],
    pattern: FencePattern,
) {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let width = dx.hypot(dy);
    if width <= 0. || height <= 0. {
        return;
    }
    let normal = [-dy / width, dx / width, 0.];
    let positions = [
        [start[0], start[1], z],
        [end[0], end[1], z],
        [end[0], end[1], z + height],
        [start[0], start[1], z + height],
    ];
    let uv = [[0., 0.], [width / unit, 0.], [width / unit, 1.], [0., 1.]];
    for i in [0, 1, 2, 0, 2, 3] {
        mesh.vertices.push(Vertex {
            position: positions[i],
            normal,
            color,
            surface: [
                SurfaceKind::Fence as u32 as f32,
                uv[i][0],
                uv[i][1],
                pattern as u32 as f32,
            ],
        });
    }
}
fn railing(mesh: &mut Mesh, x: f32, z: f32, w: f32, depth: f32, u: f32, metal: [f32; 3]) {
    fence_plane(
        mesh,
        [x, depth],
        [x + w, depth],
        z,
        u * 0.85,
        u,
        metal,
        FencePattern::IronPickets,
    );
}
fn stair_path(style: StairKind, t: f32, x: f32, run: f32, width: f32) -> (f32, f32) {
    match style {
        StairKind::Curved => (x - (t * std::f32::consts::PI).sin() * width * 0.65, run * t),
        StairKind::QuarterTurn if t > 0.5 => (x - (t - 0.5) * width * 3.0, run * 0.5),
        _ => (x, run * t),
    }
}
fn wire_fence(mesh: &mut Mesh, x: f32, width: f32, depth: f32, u: f32, metal: [f32; 3]) {
    fence_plane(
        mesh,
        [x, depth],
        [x + width, depth],
        0.,
        u * 0.8,
        u,
        metal,
        FencePattern::ChainLink,
    );
}

fn stairs(
    mesh: &mut Mesh,
    x: f32,
    z: f32,
    run: f32,
    width: f32,
    u: f32,
    style: StairKind,
    metal: [f32; 3],
) {
    let n = (z / (u * 0.21)).ceil().clamp(3., 32.) as usize;
    for i in 0..n {
        let t = i as f32 / n as f32;
        let (xx, yy) = stair_path(style, t, x, run, width);
        let zz = z * (1. - t);
        let turning = style == StairKind::QuarterTurn && t > 0.5;
        let (tx, ty, tw, td) = if turning {
            (
                xx + width * 0.5,
                yy - width * 0.5,
                width * 3. / n as f32 + u * 0.04,
                width,
            )
        } else {
            (xx, yy, width, run / n as f32 + u * 0.035)
        };
        mesh.quad(
            [
                [tx, ty, zz],
                [tx + tw, ty, zz],
                [tx + tw, ty + td, zz],
                [tx, ty + td, zz],
            ],
            [0.32, 0.34, 0.31],
            SurfaceKind::Prop,
        );
        if i % 2 == 0 {
            for side in [0., width] {
                let (px, py) = if turning {
                    (xx + width * 0.5, yy - width * 0.5 + side)
                } else {
                    (xx + side, yy)
                };
                panel(
                    mesh,
                    px - u * 0.018,
                    py,
                    zz,
                    u * 0.036,
                    u * 0.8,
                    metal,
                    SurfaceKind::Prop,
                );
            }
        }
        let (next_x, next_y) = stair_path(style, (i + 1) as f32 / n as f32, x, run, width);
        let next_z = z * (1. - (i + 1) as f32 / n as f32) + u * 0.8;
        for side in [0., width] {
            let (px, py, qx, qy) = if turning {
                (
                    xx + width * 0.5,
                    yy - width * 0.5 + side,
                    next_x + width * 0.5,
                    next_y - width * 0.5 + side,
                )
            } else {
                (xx + side, yy, next_x + side, next_y)
            };
            let top = zz + u * 0.8;
            mesh.quad(
                [
                    [px, py, top],
                    [qx, qy, next_z],
                    [qx, qy, next_z + u * 0.04],
                    [px, py, top + u * 0.04],
                ],
                metal,
                SurfaceKind::Prop,
            );
        }
    }
    if style == StairKind::QuarterTurn {
        box_at(
            mesh,
            x,
            run * 0.5 - width * 0.5,
            z * 0.5,
            width,
            width,
            u * 0.09,
            [0.38, 0.4, 0.36],
        );
    }
}
fn roof_quad(mesh: &mut Mesh, p: [[f32; 3]; 4], color: [f32; 3], unit: f32) {
    for ids in [[0, 1, 2], [0, 2, 3]] {
        mesh.triangle(ids.map(|i| p[i]), color, SurfaceKind::Roof, [unit, 0., 0.]);
    }
}
fn roof_triangle(mesh: &mut Mesh, p: [[f32; 3]; 3], color: [f32; 3], unit: f32) {
    mesh.triangle(p, color, SurfaceKind::Roof, [unit, 0., 0.]);
}
fn roof(mesh: &mut Mesh, f: Front, height: f32, kind: RoofKind, u: f32, p: &BuildingPattern) {
    match kind {
        RoofKind::Coping | RoofKind::Cornice | RoofKind::Parapet => {
            box_at(
                mesh,
                -u * 0.12,
                0.,
                height - u * 0.10,
                f.width + u * 0.24,
                u * 0.25,
                u * 0.18,
                p.trim,
            );
            if kind == RoofKind::Cornice {
                for i in 0..(f.width / (u * 0.65)).floor() as usize {
                    box_at(
                        mesh,
                        i as f32 * u * 0.65,
                        u * 0.08,
                        height - u * 0.35,
                        u * 0.12,
                        u * 0.12,
                        u * 0.25,
                        p.trim,
                    );
                }
            }
            if kind == RoofKind::Parapet {
                box_at(
                    mesh,
                    f.width * 0.27,
                    0.,
                    height,
                    f.width * 0.46,
                    u * 0.15,
                    u * 0.35,
                    p.trim,
                );
            }
        }
        RoofKind::Pitched => {
            let d = f.depth.max(u * 2.);
            let rise = u * 1.4;
            let ridge = -d * 0.5;
            let color = [0.15, 0.17, 0.15];
            roof_quad(
                mesh,
                [
                    [0., 0., height],
                    [f.width, 0., height],
                    [f.width, ridge, height + rise],
                    [0., ridge, height + rise],
                ],
                color,
                u,
            );
            roof_quad(
                mesh,
                [
                    [f.width, -d, height],
                    [0., -d, height],
                    [0., ridge, height + rise],
                    [f.width, ridge, height + rise],
                ],
                color,
                u,
            );
            roof_triangle(
                mesh,
                [
                    [0., 0., height],
                    [0., -d, height],
                    [0., ridge, height + rise],
                ],
                color,
                u,
            );
            roof_triangle(
                mesh,
                [
                    [f.width, -d, height],
                    [f.width, 0., height],
                    [f.width, ridge, height + rise],
                ],
                color,
                u,
            );
            box_at(
                mesh,
                -u * 0.1,
                0.,
                height - u * 0.08,
                f.width + u * 0.2,
                u * 0.15,
                u * 0.12,
                p.trim,
            );
        }
        RoofKind::Mansard => {
            let d = f.depth.max(u * 2.);
            let rise = u * 1.2;
            let inset = (u * 0.7).min(f.width * 0.2).min(d * 0.2);
            let color = [0.15, 0.17, 0.15];
            let base = [
                [0., 0., height],
                [f.width, 0., height],
                [f.width, -d, height],
                [0., -d, height],
            ];
            let upper = [
                [inset, -inset, height + rise],
                [f.width - inset, -inset, height + rise],
                [f.width - inset, -d + inset, height + rise],
                [inset, -d + inset, height + rise],
            ];
            for i in 0..4 {
                let next = (i + 1) % 4;
                roof_quad(mesh, [base[i], base[next], upper[next], upper[i]], color, u);
            }
            roof_quad(mesh, upper, color, u);
            box_at(
                mesh,
                -u * 0.1,
                0.,
                height - u * 0.08,
                f.width + u * 0.2,
                u * 0.15,
                u * 0.12,
                p.trim,
            );
            // Small dormers break up the sloped street-facing roof plane.
            for fraction in [0.22, 0.65] {
                let x = f.width * fraction;
                let w = (u * 0.9).min(f.width * 0.14);
                let y = -inset * 0.4;
                box_at(
                    mesh,
                    x,
                    y - u * 0.25,
                    height + u * 0.08,
                    w,
                    u * 0.3,
                    rise * 0.74,
                    p.trim,
                );
                let start = mesh.vertices.len();
                window(mesh, x, height + u * 0.12, w, rise * 0.55, u, p.trim, 17);
                for vertex in &mut mesh.vertices[start..] {
                    vertex.position[1] += y;
                }
                roof_triangle(
                    mesh,
                    [
                        [x - u * 0.05, y, height + rise * 0.82],
                        [x + w + u * 0.05, y, height + rise * 0.82],
                        [x + w * 0.5, y, height + rise * 1.1],
                    ],
                    color,
                    u,
                );
            }
        }
    }
}
// A single applied paving quad, aligned with the actual ground-floor door.
// Require a supported, unobstructed grass run ending at a sidewalk. Never bridge
// a road, water, another building, or a large change in ground elevation.
fn entrance_path(city: &City, mesh: &mut Mesh, f: Front, cfg: &BuildingDetailConfig) {
    let u = cfg.unit;
    if cfg.garden_depth <= 0. {
        return;
    }
    let door_x = f.width * 0.46;
    let width = (u * 1.2).min(f.width * 0.18);
    let center = door_x + (u * 0.95).min(f.width * 0.18) * 0.5;
    let start_point = f.point(center, u * 0.1, 0.);
    let Some(ground) = city.surface_at([start_point[0], start_point[1]], f.base + u * 0.3) else {
        return;
    };
    if ground.kind != SurfaceKind::Grass || (ground.height - f.base).abs() > u * 0.7 {
        return;
    }
    let max_depth = (cfg.garden_depth + u * 6.).min(cfg.road_reach);
    let nearby: Vec<_> = city
        .landscape
        .iter()
        .filter(|b| {
            b.kind == LandscapeKind::Building
                && b.elevation < f.base + u
                && b.elevation + b.height.unwrap_or(0.) > f.base
                && geometry::polygon_region(
                    &b.points,
                    Region::Radius {
                        center: [start_point[0], start_point[1]],
                        radius: max_depth + width,
                    },
                )
        })
        .collect();
    let mut end = None;
    let mut support = Vec::new();
    for step in 1..=64 {
        let y = max_depth * step as f32 / 64.;
        let mut sidewalk = true;
        for x in [center - width * 0.5, center, center + width * 0.5] {
            let p = f.point(x, y, 0.);
            let Some(s) = city.surface_at([p[0], p[1]], f.base + u) else {
                return;
            };
            if (s.height - ground.height).abs() > u * 0.8
                || !matches!(s.kind, SurfaceKind::Grass | SurfaceKind::Sidewalk)
                || nearby.iter().any(|b| {
                    geometry::polygon_region(
                        &b.points,
                        Region::Radius {
                            center: [p[0], p[1]],
                            radius: u * 0.02,
                        },
                    )
                })
            {
                return;
            }
            support.push((y, s.height));
            sidewalk &= s.kind == SurfaceKind::Sidewalk;
        }
        if sidewalk {
            if step > 2 {
                let p = f.point(center, y, 0.);
                end = city
                    .surface_at([p[0], p[1]], f.base + u)
                    .map(|s| (y, s.height));
            }
            break;
        }
    }
    let Some((depth, end_height)) = end else {
        return;
    };
    if (end_height - ground.height).abs() / depth > 0.15 {
        return;
    }
    // A single plane may cover a mild graded verge, but cannot bridge a dip or
    // cut through a mound. Keep it within 0.25 units of its ground support.
    if support.iter().any(|&(y, height)| {
        let plane = ground.height + (end_height - ground.height) * y / depth;
        plane - height > u * 0.25 || height - plane > u * 0.03
    }) {
        return;
    }
    let start = mesh.vertices.len();
    let left = center - width * 0.5;
    let z = ground.height - f.base + u * 0.015;
    let end_z = end_height - f.base + u * 0.015;
    mesh.quad(
        [
            [left, 0., z],
            [left + width, 0., z],
            [left + width, depth, end_z],
            [left, depth, end_z],
        ],
        [0.48, 0.46, 0.40],
        SurfaceKind::Sidewalk,
    );
    for v in &mut mesh.vertices[start..] {
        // Local paving coordinates in consumer units; w=1 selects small slabs.
        v.surface[1] = (v.position[0] - left) / u;
        v.surface[2] = v.position[1] / u;
        v.surface[3] = 1.;
    }
}
fn yard(
    city: &City,
    mesh: &mut Mesh,
    f: Front,
    p: &BuildingPattern,
    seed: &mut u64,
    cfg: &BuildingDetailConfig,
    ranges: &mut Vec<std::ops::Range<usize>>,
) {
    let u = cfg.unit;
    let depth = cfg.garden_depth;
    if depth <= 0. || random(seed) >= cfg.garden_chance {
        return;
    }
    let sample = f.point(f.width * 0.85, depth * 0.6, 0.);
    if city.building_collision([sample[0], sample[1]], u * 0.4, f.base, f.base + u * 1.2) {
        return;
    }
    if let Some(s) = city.surface_at([sample[0], sample[1]], f.base + u * 0.3) {
        if !matches!(s.kind, SurfaceKind::Grass) || (s.height - f.base).abs() > u * 0.7 {
            return;
        }
    }
    // Split the edge around the central entrance and the stair foot.
    let wall_kind = choose(&cfg.garden_edges, seed).unwrap_or(GardenEdge::Open);
    for (x, w) in [
        (u * 0.1, f.width * 0.29 - u * 0.1),
        (f.width * 0.61, f.width * 0.38),
    ] {
        if w <= u * 0.4 {
            continue;
        }
        let object_start = mesh.vertices.len();
        match wall_kind {
            GardenEdge::StoneWall => box_at(
                mesh,
                x,
                depth - u * 0.14,
                0.,
                w,
                u * 0.14,
                u * 0.35,
                [0.28, 0.29, 0.26],
            ),
            GardenEdge::BrickWall => box_at(
                mesh,
                x,
                depth - u * 0.14,
                0.,
                w,
                u * 0.14,
                u * 0.35,
                [0.28, 0.12, 0.08],
            ),
            GardenEdge::IronFence => railing(mesh, x, 0., w, depth, u, p.metal),
            GardenEdge::Hedge => box_at(
                mesh,
                x,
                depth - u * 0.3,
                0.,
                w,
                u * 0.3,
                u * 0.55,
                [0.13, 0.26, 0.10],
            ),
            GardenEdge::ConcreteEdging => box_at(
                mesh,
                x,
                depth - u * 0.1,
                0.,
                w,
                u * 0.1,
                u * 0.22,
                [0.48, 0.50, 0.45],
            ),
            GardenEdge::WireFence => wire_fence(mesh, x, w, depth, u, [0.30, 0.34, 0.31]),
            GardenEdge::Open => {}
        }
        if object_start < mesh.vertices.len() {
            ranges.push(object_start..mesh.vertices.len());
        }
    }
    if let Some(pattern) = match wall_kind {
        GardenEdge::IronFence => Some(FencePattern::IronPickets),
        GardenEdge::WireFence => Some(FencePattern::ChainLink),
        _ => None,
    } {
        // One divider belongs to each lot; add the far return at the end of a row.
        let neighbour = f.point(f.width + u * 0.2, -u * 0.5, 0.);
        let shared_far_side =
            city.building_collision([neighbour[0], neighbour[1]], u * 0.05, f.base, f.base + u);
        for x in [u * 0.025, f.width - u * 0.025] {
            if x > f.width * 0.5 && shared_far_side {
                continue;
            }
            let clear = [u * 0.15, depth * 0.5, depth].into_iter().all(|y| {
                let sample = f.point(x, y, 0.);
                !city.building_collision([sample[0], sample[1]], u * 0.04, f.base, f.base + u)
                    && city
                        .surface_at([sample[0], sample[1]], f.base + u * 0.3)
                        .is_none_or(|s| {
                            s.kind == SurfaceKind::Grass && (s.height - f.base).abs() < u * 0.7
                        })
            });
            if !clear {
                continue;
            }
            let object_start = mesh.vertices.len();
            fence_plane(
                mesh,
                [x, u * 0.1],
                [x, depth],
                0.,
                u * 0.85,
                u,
                p.metal,
                pattern,
            );
            ranges.push(object_start..mesh.vertices.len());
        }
    }
    let object_start = mesh.vertices.len();
    let x = f.width * 0.83;
    match choose(&cfg.garden_props, seed).unwrap_or(GardenProp::None) {
        GardenProp::Planter => {
            box_at(
                mesh,
                x,
                depth * 0.4,
                0.,
                u * 0.38,
                u * 0.35,
                u * 0.32,
                [0.35, 0.13, 0.07],
            );
            box_at(
                mesh,
                x + u * 0.05,
                depth * 0.4 + u * 0.05,
                u * 0.32,
                u * 0.28,
                u * 0.25,
                u * 0.28,
                [0.12, 0.28, 0.09],
            );
        }
        GardenProp::Bench => {
            box_at(
                mesh,
                x,
                depth * 0.35,
                u * 0.40,
                u * 0.9,
                u * 0.36,
                u * 0.08,
                [0.28, 0.18, 0.09],
            );
            for dx in [0., u * 0.75] {
                box_at(
                    mesh,
                    x + dx,
                    depth * 0.4,
                    0.,
                    u * 0.08,
                    u * 0.08,
                    u * 0.4,
                    p.metal,
                );
            }
        }
        GardenProp::Bin => box_at(
            mesh,
            x,
            depth * 0.4,
            0.,
            u * 0.4,
            u * 0.4,
            u * 0.7,
            [0.12, 0.20, 0.13],
        ),
        GardenProp::Bicycle => bicycle(mesh, x, depth * 0.4, u, p.metal),
        GardenProp::None => {}
    }
    if object_start < mesh.vertices.len() {
        ranges.push(object_start..mesh.vertices.len());
    }
}
fn bicycle(mesh: &mut Mesh, x: f32, y: f32, u: f32, metal: [f32; 3]) {
    // Low-poly wheels lie in the vertical facade plane; no tyre polygons follow surface textures.
    for center in [x, x + u * 1.0] {
        for i in 0..10 {
            let a = i as f32 * std::f32::consts::TAU / 10.;
            let b = (i + 1) as f32 * std::f32::consts::TAU / 10.;
            let r = u * 0.3;
            let first = [center + r * a.cos(), y, u * 0.31 + r * a.sin()];
            let second = [center + r * b.cos(), y, u * 0.31 + r * b.sin()];
            let c = [second[0], y + u * 0.025, second[2]];
            let d = [first[0], y + u * 0.025, first[2]];
            mesh.quad([first, second, c, d], metal, SurfaceKind::Prop);
        }
    }
    for (dx, dz, w, h) in [
        (0.0, 0.3, 1.0, 0.035),
        (0.25, 0.3, 0.035, 0.45),
        (0.65, 0.3, 0.035, 0.55),
        (0.2, 0.75, 0.2, 0.04),
        (0.65, 0.84, 0.2, 0.04),
    ] {
        box_at(mesh, x + dx * u, y, dz * u, w * u, u * 0.03, h * u, metal);
    }
}

fn is_rectangular(feature: &LandscapeFeature) -> bool {
    if feature.points.len() != 4 {
        return false;
    }
    let points = &feature.points;
    for i in 0..4 {
        let a = points[i];
        let b = points[(i + 1) % 4];
        let c = points[(i + 2) % 4];
        let first = [b[0] - a[0], b[1] - a[1]];
        let second = [c[0] - b[0], c[1] - b[1]];
        let scale = first[0].hypot(first[1]) * second[0].hypot(second[1]);
        if scale < 1e-10 || (first[0] * second[0] + first[1] * second[1]).abs() > scale * 0.08 {
            return false;
        }
    }
    true
}

fn make_details(
    city: &City,
    f: Front,
    feature: &LandscapeFeature,
    cfg: &BuildingDetailConfig,
    p: &BuildingPattern,
    seed: &mut u64,
    mesh: &mut Mesh,
    props: &mut Vec<Mesh>,
) {
    let start = mesh.vertices.len();
    let mut ranges = Vec::new();
    let u = cfg.unit;
    let height = feature.height.unwrap_or(0.);
    let floors = p.floors as usize;
    let floor = height / floors as f32;
    let wide = matches!(p.facade, FacadeKind::WideStacked | FacadeKind::Detached);
    let door_x = f.width * 0.46;
    let door_w = (u * 0.95).min(f.width * 0.18);
    for n in 0..floors {
        let z = n as f32 * floor;
        let wh = (floor * 0.52).min(u * 2.1);
        let wz = z + floor * 0.26;
        let ww = if wide {
            f.width * 0.30
        } else {
            (u * 1.15).min(f.width * 0.20)
        };
        window(
            mesh,
            f.width * 0.12,
            wz,
            ww,
            wh,
            u,
            p.trim,
            seed.wrapping_add(n as u64),
        );
        window(
            mesh,
            f.width * 0.72 - w_adjust(wide, ww),
            wz,
            ww,
            wh,
            u,
            p.trim,
            seed.wrapping_add(n as u64).wrapping_add(19),
        );
        if n == 0
            || matches!(
                p.facade,
                FacadeKind::Stacked | FacadeKind::SimpleStacked | FacadeKind::WideStacked
            )
        {
            door(mesh, door_x, z, door_w, floor * 0.83, u, p);
        } else {
            window(
                mesh,
                door_x,
                wz,
                door_w,
                wh,
                u,
                p.trim,
                seed.wrapping_add(n as u64).wrapping_add(7),
            );
        }
        let balcony_start = mesh.vertices.len();
        if n > 0
            && matches!(
                p.facade,
                FacadeKind::Stacked | FacadeKind::SimpleStacked | FacadeKind::WideStacked
            )
        {
            let x = f.width * 0.34;
            let width = f.width * 0.36;
            let depth = u * 1.0;
            box_at(
                mesh,
                x,
                0.,
                z - u * 0.14,
                width,
                depth,
                u * 0.14,
                [0.42, 0.43, 0.39],
            );
            railing(mesh, x, z, width, depth, u, p.metal);
            for xx in [x, x + width] {
                fence_plane(
                    mesh,
                    [xx, 0.],
                    [xx, depth],
                    z,
                    u * 0.85,
                    u,
                    p.metal,
                    FencePattern::IronPickets,
                );
            }
        }
        if balcony_start < mesh.vertices.len() {
            ranges.push(balcony_start..mesh.vertices.len());
        }
    }
    if p.facade == FacadeKind::Cottage {
        // A shallow covered gallery is part of the facade, rather than a separate building.
        let canopy_height = floor * 0.88;
        let canopy_start = mesh.vertices.len();
        box_at(
            mesh,
            u * 0.15,
            0.,
            canopy_height,
            f.width - u * 0.3,
            u * 0.9,
            u * 0.12,
            p.metal,
        );
        ranges.push(canopy_start..mesh.vertices.len());
        for x in [u * 0.2, f.width - u * 0.3] {
            let post_start = mesh.vertices.len();
            box_at(
                mesh,
                x,
                u * 0.8,
                0.,
                u * 0.08,
                u * 0.08,
                canopy_height,
                p.trim,
            );
            ranges.push(post_start..mesh.vertices.len());
        }
    }
    let stair_start = mesh.vertices.len();
    if floors > 1 && p.stair != StairKind::None {
        stairs(
            mesh,
            f.width * 0.30,
            floor,
            u * 2.5,
            u * 0.85,
            u,
            p.stair,
            p.metal,
        );
    } else if !matches!(
        city.surface_at(
            { let p = f.point(door_x, u * 0.5, 0.); [p[0], p[1]] },
            f.base + u * 0.3,
        ),
        Some(s) if matches!(s.kind, SurfaceKind::Sidewalk | SurfaceKind::Asphalt)
    ) {
        stairs(
            mesh,
            door_x,
            u * 0.3,
            u * 0.5,
            door_w,
            u,
            StairKind::Straight,
            p.metal,
        );
    }
    if stair_start < mesh.vertices.len() {
        ranges.push(stair_start..mesh.vertices.len());
    }
    let roof_kind =
        if matches!(p.roof, RoofKind::Pitched | RoofKind::Mansard) && !is_rectangular(feature) {
            RoofKind::Coping
        } else {
            p.roof
        };
    roof(mesh, f, height, roof_kind, u, p);
    entrance_path(city, mesh, f, cfg);
    yard(city, mesh, f, p, seed, cfg, &mut ranges);
    f.rotate(mesh, start);
    for range in ranges.into_iter().rev() {
        props.push(Mesh {
            vertices: mesh.vertices.drain(range).collect(),
        });
    }
}
fn w_adjust(wide: bool, ww: f32) -> f32 {
    if wide {
        ww * 0.45
    } else {
        0.
    }
}
impl City {
    /// Additional configurable facade and frontage geometry; leaves the authored
    /// building footprints, surfaces, and collision queries unchanged.
    pub fn building_detail_mesh(&self, region: Region, cfg: &BuildingDetailConfig) -> Mesh {
        let (mut mesh, props) = self.building_detail_parts(region, cfg);
        for prop in props {
            mesh.vertices.extend(prop.vertices);
        }
        mesh
    }

    /// Facade geometry and independently removable balconies, stairs, and frontage objects.
    pub fn building_detail_parts(
        &self,
        region: Region,
        cfg: &BuildingDetailConfig,
    ) -> (Mesh, Vec<Mesh>) {
        let mut mesh = Mesh::default();
        let mut props = Vec::new();
        if !region.valid()
            || !cfg.unit.is_finite()
            || cfg.unit <= 0.
            || !cfg.maximum_height.is_finite()
            || !cfg.minimum_frontage.is_finite()
            || !cfg.road_reach.is_finite()
            || cfg.road_reach < 0.
            || !cfg.garden_depth.is_finite()
            || !cfg.garden_chance.is_finite()
            || !(0. ..=1.).contains(&cfg.garden_chance)
            || cfg.patterns.iter().any(|p| {
                p.trim
                    .iter()
                    .chain(&p.metal)
                    .chain(&p.door)
                    .any(|v| !v.is_finite())
            })
        {
            return (mesh, props);
        }
        for feature in self.landscape.iter().filter(|f| {
            f.kind == LandscapeKind::Building
                && f.model.is_none()
                && f.building_use.allows_residential_details()
                && geometry::polygon_region(&f.points, region)
        }) {
            let height = feature.height.unwrap_or(0.);
            if height <= 0. || height > cfg.maximum_height {
                continue;
            }
            let style = feature.building_style.unwrap_or_default();
            let floor_count = (height / style.floor_height).round();
            if !(1. ..=4.).contains(&floor_count) {
                continue;
            }
            let floors = floor_count as u8;
            let Some(front) = front_for(self, feature, cfg.road_reach) else {
                continue;
            };
            if front.width < cfg.minimum_frontage {
                continue;
            }
            let mut seed = seed_for(feature, cfg.seed);
            let Some(&mut_chosen) = pattern(cfg, floors, &mut seed) else {
                continue;
            };
            let mut chosen = mut_chosen;
            let stair_end = front.point(front.width * 0.30, cfg.unit * 2.6, 0.);
            if matches!(
                self.surface_at([stair_end[0], stair_end[1]], front.base + cfg.unit * 0.3),
                Some(s) if matches!(s.kind, SurfaceKind::Sidewalk | SurfaceKind::Asphalt)
            ) {
                chosen.stair = StairKind::None;
                if matches!(
                    chosen.facade,
                    FacadeKind::Stacked | FacadeKind::SimpleStacked | FacadeKind::WideStacked
                ) {
                    chosen.facade = FacadeKind::Rowhouse;
                }
            }
            make_details(
                self, front, feature, cfg, &chosen, &mut seed, &mut mesh, &mut props,
            );
        }
        (mesh, props)
    }
}
