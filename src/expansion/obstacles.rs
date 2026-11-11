use super::*;

struct Corridor {
    points: Vec<(WorldPoint, f32)>,
    half: f32,
    bounds: [f32; 4],
}
struct Footprint {
    points: Vec<WorldPoint>,
    low: f32,
    high: f32,
    closed: bool,
    bounds: [f32; 4],
    joins: Vec<String>,
}
pub(super) struct Obstacles {
    roads: Vec<Corridor>,
    polygons: Vec<Footprint>,
}
fn bounds(points: impl Iterator<Item = WorldPoint>) -> [f32; 4] {
    points.fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |mut b, p| {
            b[0] = b[0].min(p.x);
            b[1] = b[1].min(p.y);
            b[2] = b[2].max(p.x);
            b[3] = b[3].max(p.y);
            b
        },
    )
}
fn nearby(p: WorldPoint, b: [f32; 4], margin: f32) -> bool {
    p.x >= b[0] - margin && p.y >= b[1] - margin && p.x <= b[2] + margin && p.y <= b[3] + margin
}
impl Obstacles {
    pub fn new(program: &Program) -> Self {
        let roads = map::project(program)
            .segments
            .iter()
            .map(|s| {
                let points: Vec<_> = (0..=128)
                    .map(|i| {
                        let t = i as f32 / 128.;
                        (s.point_at(t), s.height_at(t))
                    })
                    .collect();
                Corridor {
                    bounds: bounds(points.iter().map(|(p, _)| *p)),
                    points,
                    half: definition(program, s).map(width).unwrap_or(0.) / 2.,
                }
            })
            .collect();
        let landscape = project_landscape_features(program);
        let mut polygons: Vec<_> = landscape
            .features
            .iter()
            .filter(|f| {
                matches!(
                    f.kind,
                    LandscapeKind::Building | LandscapeKind::Lake | LandscapeKind::River
                )
            })
            .map(|f| {
                let points: Vec<_> = f
                    .points
                    .iter()
                    .map(|p| WorldPoint { x: p[0], y: p[1] })
                    .collect();
                Footprint {
                    bounds: bounds(points.iter().copied()),
                    points,
                    low: if f.kind == LandscapeKind::Building {
                        f.elevation - 0.02
                    } else {
                        f32::NEG_INFINITY
                    },
                    high: f.elevation + f.height.unwrap_or(0.) + 0.06,
                    closed: f.kind != LandscapeKind::River,
                    joins: vec![],
                }
            })
            .collect();
        for feature in &landscape.features {
            for wall in terrain_walls(feature, &landscape.features) {
                let points = vec![
                    WorldPoint {
                        x: wall.start[0],
                        y: wall.start[1],
                    },
                    WorldPoint {
                        x: wall.end[0],
                        y: wall.end[1],
                    },
                ];
                polygons.push(Footprint {
                    bounds: bounds(points.iter().copied()),
                    points,
                    low: wall.bottom,
                    high: wall.top,
                    closed: false,
                    joins: vec![],
                });
            }
        }
        let projection = map::project(program);
        for junction in map::intersections(program, &projection) {
            let Some(road) = program.statements.iter().find_map(|s| match s {
                Statement::Road(r) if r.name == junction.road_name => Some(r),
                _ => None,
            }) else {
                continue;
            };
            let half = map::intersection_footprint_half(&junction, width(road) / 2.);
            let p = junction.position;
            let points = vec![
                WorldPoint {
                    x: p.x - half,
                    y: p.y - half,
                },
                WorldPoint {
                    x: p.x + half,
                    y: p.y - half,
                },
                WorldPoint {
                    x: p.x + half,
                    y: p.y + half,
                },
                WorldPoint {
                    x: p.x - half,
                    y: p.y + half,
                },
            ];
            let z = elevation_of(program, &junction.point_ref).unwrap_or(0.);
            polygons.push(Footprint {
                bounds: bounds(points.iter().copied()),
                points,
                low: z - 0.06,
                high: z + 0.06,
                closed: true,
                joins: junction.ports.iter().map(|p| p.point_ref.clone()).collect(),
            });
        }
        for instance in crate::network::project(program).instances {
            for band in instance
                .geometry
                .bands
                .iter()
                .filter(|b| b.kind == crate::network::BandKind::Surface)
            {
                for quad in instance.geometry.band_quads(band) {
                    let world = quad.map(|p| instance.world_point(p));
                    let points: Vec<_> = world
                        .iter()
                        .map(|p| WorldPoint { x: p.x, y: p.y })
                        .collect();
                    polygons.push(Footprint {
                        bounds: bounds(points.iter().copied()),
                        points,
                        low: world.iter().map(|p| p.z).fold(f32::INFINITY, f32::min) - 0.06,
                        high: world.iter().map(|p| p.z).fold(f32::NEG_INFINITY, f32::max) + 0.06,
                        closed: true,
                        joins: vec![],
                    });
                }
            }
        }
        Self { roads, polygons }
    }
    pub fn clashes(&self, after: &Program, ghosts: &[Segment], end: &End) -> bool {
        for ghost in ghosts {
            let half = definition(after, ghost).map(width).unwrap_or(0.) / 2.;
            let length = (0..32)
                .map(|i| {
                    distance(
                        ghost.point_at(i as f32 / 32.),
                        ghost.point_at((i + 1) as f32 / 32.),
                    )
                })
                .sum::<f32>();
            let steps = ((length / (half.max(0.005) * 0.5)).ceil() as usize).clamp(32, 4096);
            for i in 0..=steps {
                let t = i as f32 / steps as f32;
                let p = ghost.point_at(t);
                let z = ghost.height_at(t);
                for road in &self.roads {
                    let clearance = half + road.half;
                    if !nearby(p, road.bounds, clearance) {
                        continue;
                    }
                    let joins = road
                        .points
                        .first()
                        .into_iter()
                        .chain(road.points.last())
                        .any(|(p, z)| {
                            distance(*p, end.point) < 0.001 && (*z - end.height).abs() < 0.001
                        });
                    if joins && distance(p, end.point) < clearance * 1.5 {
                        continue;
                    }
                    for pair in road.points.windows(2) {
                        if (z - (pair[0].1 + pair[1].1) / 2.).abs() < 0.06
                            && point_line(p, pair[0].0, pair[1].0) < clearance
                        {
                            return true;
                        }
                    }
                }
                for polygon in &self.polygons {
                    if matches!(&end.anchor,Anchor::IntersectionPort(reference)
                        if polygon.joins.contains(reference))
                        && distance(p, end.point) < half * 3.
                    {
                        continue;
                    }
                    if z < polygon.low || z > polygon.high || !nearby(p, polygon.bounds, half) {
                        continue;
                    }
                    let mut inside = false;
                    let count = if polygon.closed {
                        polygon.points.len()
                    } else {
                        polygon.points.len().saturating_sub(1)
                    };
                    for j in 0..count {
                        let a = polygon.points[j];
                        let b = polygon.points[(j + 1) % polygon.points.len()];
                        if point_line(p, a, b) < half {
                            return true;
                        }
                        if (a.y > p.y) != (b.y > p.y)
                            && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x
                        {
                            inside = !inside;
                        }
                    }
                    if polygon.closed && inside {
                        return true;
                    }
                }
            }
        }
        false
    }
}
