//! Shared authoring geometry for elevated walls and vehicle corridors.
//! Curves are subdivided with a control-polygon flatness bound. All consumers
//! use the resulting piecewise-linear surfaces, independent of camera zoom.
use crate::{
    map::{self, Projection, Segment, WorldPoint},
    Program, RoadDefinition, RoadElementKind, RoadRole, Statement,
};

pub const GEOMETRY_TOLERANCE: f32 = 0.0005;
const MAX_SPAN: f32 = 0.04;
const HEIGHT_TOLERANCE: f32 = 0.012;

#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub point: WorldPoint,
    pub height: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct Wall {
    pub segment: usize,
    pub start: Vertex,
    pub end: Vertex,
}
#[derive(Clone, Copy)]
struct Triangle {
    vertices: [Vertex; 3],
}
#[derive(Clone, Debug)]
pub struct CrossSection {
    pub t: f32,
    pub boundaries: Vec<Vertex>,
}

struct Surface {
    rows: Vec<CrossSection>,
    forbidden: Vec<(Triangle, RoadElementKind)>,
    lanes: Vec<Triangle>,
    edges: [Vec<Vertex>; 2],
}
pub struct RoadGeometry {
    pub walls: Vec<Wall>,
    /// Curves that could not be resolved within the subdivision budget.
    pub unresolved: Vec<usize>,
    surfaces: Vec<Surface>,
}
fn bounds(points: impl Iterator<Item = WorldPoint>) -> [f32; 4] {
    points.fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |b, p| [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)],
    )
}
fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn cross(a: WorldPoint, b: WorldPoint, c: WorldPoint) -> f64 {
    (b.x as f64 - a.x as f64) * (c.y as f64 - a.y as f64)
        - (b.y as f64 - a.y as f64) * (c.x as f64 - a.x as f64)
}
fn mix(a: Vertex, b: Vertex, t: f32) -> Vertex {
    Vertex {
        point: WorldPoint {
            x: a.point.x + (b.point.x - a.point.x) * t,
            y: a.point.y + (b.point.y - a.point.y) * t,
        },
        height: a.height + (b.height - a.height) * t,
    }
}
fn midpoint(a: WorldPoint, b: WorldPoint) -> WorldPoint {
    WorldPoint {
        x: (a.x + b.x) * 0.5,
        y: (a.y + b.y) * 0.5,
    }
}
fn distance(a: WorldPoint, b: WorldPoint) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}
fn parameters(segment: &Segment) -> (Vec<f32>, bool) {
    fn visit(p: [WorldPoint; 4], lo: f32, hi: f32, depth: u8, out: &mut Vec<f32>, ok: &mut bool) {
        let chord = distance(p[0], p[3]);
        let polygon = distance(p[0], p[1]) + distance(p[1], p[2]) + distance(p[2], p[3]);
        let flat = if chord > 1e-8 {
            (cross(p[0], p[3], p[1])
                .abs()
                .max(cross(p[0], p[3], p[2]).abs()) as f32)
                / chord
        } else {
            polygon
        };
        if polygon <= MAX_SPAN && flat <= GEOMETRY_TOLERANCE {
            out.push(hi);
            return;
        }
        if depth == 18 {
            *ok = false;
            out.push(hi);
            return;
        }
        let a = midpoint(p[0], p[1]);
        let b = midpoint(p[1], p[2]);
        let c = midpoint(p[2], p[3]);
        let d = midpoint(a, b);
        let e = midpoint(b, c);
        let f = midpoint(d, e);
        let mid = (lo + hi) * 0.5;
        visit([p[0], a, d, f], lo, mid, depth + 1, out, ok);
        visit([f, e, c, p[3]], mid, hi, depth + 1, out, ok);
    }
    let mut out = vec![0.0];
    let mut ok = true;
    visit(
        [
            segment.start,
            segment.controls[0],
            segment.controls[1],
            segment.end,
        ],
        0.0,
        1.0,
        0,
        &mut out,
        &mut ok,
    );
    (out, ok)
}
fn offset_vertex(segment: &Segment, t: f32, offset: f32) -> Vertex {
    let p = segment.point_at(t);
    let v = segment.tangent_at(t);
    let length = v.x.hypot(v.y).max(1e-8);
    Vertex {
        point: WorldPoint {
            x: p.x + v.y / length * offset,
            y: p.y - v.x / length * offset,
        },
        height: segment.height_at(t),
    }
}
fn definition<'a>(program: &'a Program, segment: &Segment) -> Option<&'a RoadDefinition> {
    program.statements.iter().find_map(|s| match s {
        Statement::Road(r) if r.name == segment.road_name => Some(r),
        _ => None,
    })
}
/// Clip a segment to a triangular surface extruded by a vertical interval.
/// f64 predicates reduce cancellation; this is a toleranced authoring kernel,
/// not an exact-arithmetic solid modeller.
fn clip(a: Vertex, b: Vertex, triangle: Triangle, below: f32, above: f32) -> Option<(f32, f32)> {
    let [p, q, r] = triangle.vertices;
    if a.point.x.max(b.point.x) < p.point.x.min(q.point.x).min(r.point.x)
        || a.point.x.min(b.point.x) > p.point.x.max(q.point.x).max(r.point.x)
        || a.point.y.max(b.point.y) < p.point.y.min(q.point.y).min(r.point.y)
        || a.point.y.min(b.point.y) > p.point.y.max(q.point.y).max(r.point.y)
        || a.height.max(b.height) < p.height.min(q.height).min(r.height) - below
        || a.height.min(b.height) > p.height.max(q.height).max(r.height) + above
    {
        return None;
    }

    let area = cross(p.point, q.point, r.point);
    if area.abs() < 1e-14 {
        return None;
    }
    let weights = |v: Vertex| {
        [
            cross(q.point, r.point, v.point) / area,
            cross(r.point, p.point, v.point) / area,
            cross(p.point, q.point, v.point) / area,
        ]
    };
    let wa = weights(a);
    let wb = weights(b);
    let za = a.height as f64
        - (wa[0] * p.height as f64 + wa[1] * q.height as f64 + wa[2] * r.height as f64);
    let zb = b.height as f64
        - (wb[0] * p.height as f64 + wb[1] * q.height as f64 + wb[2] * r.height as f64);
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    for (u, v) in [
        (wa[0], wb[0]),
        (wa[1], wb[1]),
        (wa[2], wb[2]),
        (za + below as f64, zb + below as f64),
        (above as f64 - za, above as f64 - zb),
    ] {
        if u < 0.0 && v < 0.0 {
            return None;
        }
        if u < 0.0 {
            lo = lo.max(u / (u - v));
        } else if v < 0.0 {
            hi = hi.min(u / (u - v));
        }
    }
    (hi - lo > 1e-7).then_some((lo as f32, hi as f32))
}
fn attached_parent(
    program: &Program,
    projection: &Projection,
    mouth: usize,
    parent: usize,
) -> bool {
    let a = &projection.segments[mouth];
    let b = &projection.segments[parent];
    [a.start_ref.as_deref(), a.end_ref.as_deref()]
        .into_iter()
        .flatten()
        .any(|reference| {
            let Some((name, lane, t)) = map::lane_attachment(program, reference) else {
                return false;
            };
            let Some(p) = projection
                .segments
                .iter()
                .find(|p| p.name.as_deref() == Some(&name))
            else {
                return false;
            };
            if !(0.0..=1.0).contains(&t)
                || !definition(program, p).is_some_and(|r| {
                    r.elements
                        .get(lane)
                        .is_some_and(|e| e.kind == RoadElementKind::CarLane)
                })
            {
                return false;
            }
            if p.name == b.name {
                return true;
            }
            let seam = if t < 0.001 {
                p.start_ref.as_deref()
            } else if t > 0.999 {
                p.end_ref.as_deref()
            } else {
                None
            };
            seam.is_some_and(|s| {
                b.start_ref.as_deref() == Some(s) || b.end_ref.as_deref() == Some(s)
            })
        })
}
fn surface_rows(
    segment: &Segment,
    road: &RoadDefinition,
    profile: &crate::road_profiles::RoadProfile,
) -> (Vec<CrossSection>, bool) {
    let (ts, ok) = parameters(segment);
    let start = profile.start_clip.unwrap_or(0.);
    let end = profile.end_clip.unwrap_or(1.);
    if start >= end {
        return (Vec::new(), ok);
    }
    let rows = std::iter::once(start)
        .chain(ts.into_iter().filter(|t| *t > start && *t < end))
        .chain(std::iter::once(end))
        .map(|t| {
            let (widths, mut offset) = crate::road_profiles::section_widths(road, profile, t);
            let mut boundaries = vec![offset_vertex(segment, t, offset)];
            for width in widths {
                offset += width;
                boundaries.push(offset_vertex(segment, t, offset));
            }
            CrossSection { t, boundaries }
        })
        .collect();
    (rows, ok)
}

impl RoadGeometry {
    pub fn cross_sections(&self, segment: usize) -> &[CrossSection] {
        &self.surfaces[segment].rows
    }

    /// Compile only one road's surface rows while resolving its profile against
    /// the complete projection (including attached parent roads).
    pub fn placement_cross_sections(
        program: &Program,
        projection: &Projection,
        index: usize,
    ) -> Option<(Vec<CrossSection>, bool)> {
        let segment = projection.segments.get(index)?;
        let road = definition(program, segment)?;
        let profiles = crate::road_profiles::road_profiles(program, projection);
        Some(surface_rows(segment, road, &profiles[index]))
    }

    pub fn compile(program: &Program, projection: &Projection) -> Self {
        let profiles = crate::road_profiles::road_profiles(program, projection);
        let mut unresolved = Vec::new();
        let surfaces: Vec<_> = projection
            .segments
            .iter()
            .enumerate()
            .map(|(index, segment)| {
                let Some(road) = definition(program, segment) else {
                    return Surface {
                        rows: vec![],
                        forbidden: vec![],
                        lanes: vec![],
                        edges: [vec![], vec![]],
                    };
                };
                let (rows, ok) = surface_rows(segment, road, &profiles[index]);
                if !ok {
                    unresolved.push(index);
                }
                let edges = [
                    rows.iter().map(|r| r.boundaries[0]).collect(),
                    rows.iter().map(|r| *r.boundaries.last().unwrap()).collect(),
                ];
                let mut lanes = Vec::new();
                let mut forbidden = Vec::new();
                for (band, element) in road.elements.iter().enumerate() {
                    for pair in rows.windows(2) {
                        let [a, b, c, d] = [
                            pair[0].boundaries[band],
                            pair[0].boundaries[band + 1],
                            pair[1].boundaries[band + 1],
                            pair[1].boundaries[band],
                        ];
                        for triangle in [
                            Triangle {
                                vertices: [a, b, c],
                            },
                            Triangle {
                                vertices: [a, c, d],
                            },
                        ] {
                            if element.kind == RoadElementKind::CarLane {
                                lanes.push(triangle);
                            } else {
                                forbidden.push((triangle, element.kind));
                            }
                        }
                    }
                }
                Surface {
                    rows,
                    forbidden,
                    lanes,
                    edges,
                }
            })
            .collect();
        let mut connections = vec![Vec::new(); surfaces.len()];
        for (mouth, segment) in projection.segments.iter().enumerate() {
            if !definition(program, segment).is_some_and(|r| {
                r.role == RoadRole::Merge
                    && r.elements
                        .iter()
                        .all(|e| e.kind == RoadElementKind::CarLane)
            }) {
                continue;
            }
            for parent in 0..surfaces.len() {
                if parent != mouth && attached_parent(program, projection, mouth, parent) {
                    connections[parent].push(mouth);
                    connections[mouth].push(parent);
                }
            }
        }
        let mut walls = Vec::new();
        for (index, surface) in surfaces.iter().enumerate() {
            for edge in &surface.edges {
                for pair in edge.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    if a.height.max(b.height) <= 0.0 {
                        continue;
                    }
                    let mut cuts = Vec::new();
                    for &other in &connections[index] {
                        for triangle in &surfaces[other].lanes {
                            if let Some(interval) =
                                clip(a, b, *triangle, HEIGHT_TOLERANCE, HEIGHT_TOLERANCE)
                            {
                                cuts.push(interval);
                            }
                        }
                    }
                    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
                    let mut cursor = 0.0;
                    for (lo, hi) in cuts {
                        if lo > cursor + 1e-6 {
                            walls.push(Wall {
                                segment: index,
                                start: mix(a, b, cursor),
                                end: mix(a, b, lo),
                            });
                        }
                        cursor = cursor.max(hi);
                    }
                    if cursor < 1.0 - 1e-6 {
                        walls.push(Wall {
                            segment: index,
                            start: mix(a, b, cursor),
                            end: b,
                        });
                    }
                }
            }
        }
        Self {
            walls,
            unresolved,
            surfaces,
        }
    }
    /// Full-width lane overlap against non-car bands, including narrow barriers
    /// that can lie entirely between centerline samples.
    pub fn band_intrusions(
        &self,
        program: &Program,
        projection: &Projection,
    ) -> Vec<(usize, usize, WorldPoint, RoadElementKind)> {
        let junctions = map::intersections(program, projection);
        let boxes: Vec<_> = self
            .surfaces
            .iter()
            .map(|s| bounds(s.edges.iter().flatten().map(|v| v.point)))
            .collect();
        let mut result = Vec::new();
        for (lane, surface) in self.surfaces.iter().enumerate() {
            for (other, obstacle) in self.surfaces.iter().enumerate() {
                if lane == other || !overlaps(boxes[lane], boxes[other]) {
                    continue;
                }
                let is_merge = definition(program, &projection.segments[lane])
                    .is_some_and(|r| r.role == RoadRole::Merge)
                    && attached_parent(program, projection, lane, other);
                'pair: for &(band, kind) in &obstacle.forbidden {
                    if is_merge && kind != RoadElementKind::Barrier {
                        continue;
                    }
                    let band_box = bounds(band.vertices.iter().map(|v| v.point));
                    for triangle in &surface.lanes {
                        if !overlaps(band_box, bounds(triangle.vertices.iter().map(|v| v.point))) {
                            continue;
                        }
                        // Inset both triangles very slightly to exclude boundary-only contact.
                        let inset = |triangle: Triangle| {
                            let [a, b, c] = triangle.vertices;
                            let center = Vertex {
                                point: WorldPoint {
                                    x: (a.point.x + b.point.x + c.point.x) / 3.0,
                                    y: (a.point.y + b.point.y + c.point.y) / 3.0,
                                },
                                height: (a.height + b.height + c.height) / 3.0,
                            };
                            Triangle {
                                vertices: triangle.vertices.map(|v| mix(v, center, 0.001)),
                            }
                        };
                        let a = inset(*triangle);
                        let b = inset(band);
                        for (edges, target) in [(a, b), (b, a)] {
                            for edge in 0..3 {
                                let start = edges.vertices[edge];
                                let end = edges.vertices[(edge + 1) % 3];
                                let clearance = if kind == RoadElementKind::Barrier {
                                    0.04
                                } else {
                                    0.01
                                };
                                if let Some((lo, hi)) =
                                    clip(start, end, target, clearance, clearance)
                                {
                                    let at = mix(start, end, (lo + hi) * 0.5).point;
                                    if kind != RoadElementKind::Barrier
                                        && junctions.iter().any(|j| distance(at, j.position) < 0.16)
                                    {
                                        continue;
                                    }
                                    if distance(
                                        mix(start, end, lo).point,
                                        mix(start, end, hi).point,
                                    ) > GEOMETRY_TOLERANCE
                                    {
                                        result.push((lane, other, at, kind));
                                        break 'pair;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        result
    }
    /// Remaining generated walls intersecting another full-width car surface.
    pub fn obstructions(&self) -> Vec<(usize, usize, WorldPoint)> {
        let boxes: Vec<_> = self
            .surfaces
            .iter()
            .map(|s| bounds(s.edges.iter().flatten().map(|v| v.point)))
            .collect();
        let mut found = std::collections::HashSet::new();
        let mut result = Vec::new();
        for wall in &self.walls {
            for (lane, surface) in self.surfaces.iter().enumerate() {
                if wall.segment == lane
                    || found.contains(&(wall.segment, lane))
                    || !overlaps(
                        bounds([wall.start.point, wall.end.point].into_iter()),
                        boxes[lane],
                    )
                {
                    continue;
                }
                for triangle in &surface.lanes {
                    // Wall extends from deck - .08 to deck + .005; reserve .04 above pavement.
                    if let Some((lo, hi)) = clip(wall.start, wall.end, *triangle, 0.005, 0.12) {
                        if distance(
                            mix(wall.start, wall.end, lo).point,
                            mix(wall.start, wall.end, hi).point,
                        ) > GEOMETRY_TOLERANCE
                        {
                            found.insert((wall.segment, lane));
                            result.push((
                                wall.segment,
                                lane,
                                mix(wall.start, wall.end, (lo + hi) * 0.5).point,
                            ));
                            break;
                        }
                    }
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn vertex(x: f32, y: f32, z: f32) -> Vertex {
        Vertex {
            point: WorldPoint { x, y },
            height: z,
        }
    }
    #[test]
    fn placement_rows_match_full_compiler_with_parent_profiles() {
        let mut program = crate::parse("road street: carlane-carlane; end; a=street(-1,0,1,0); grade(a,0.2,0.6); b=street(0,2,1,3); grade(b,0.1,0.3);").unwrap();
        let projection = map::project(&program);
        let parent = projection.segments[0].source;
        let (reference, start) = map::create_lane_point(&mut program, parent, 0, 0.6).unwrap();
        let Statement::Road(road) = program.statements[0].clone() else {
            unreachable!()
        };
        let (index, _) = map::append_connected(
            &mut program,
            &road,
            start,
            Some(&reference),
            WorldPoint { x: 2.0, y: 1.0 },
            None,
        );
        map::create_lane_split(
            &mut program,
            &reference,
            map::SegmentSource::TopLevel(index),
            map::SplitMode::KeepLane,
        );
        let projection = map::project(&program);
        let geometry = RoadGeometry::compile(&program, &projection);
        for index in 0..projection.segments.len() {
            let (selected, resolved) =
                RoadGeometry::placement_cross_sections(&program, &projection, index).unwrap();
            assert!(resolved);
            let full = geometry.cross_sections(index);
            assert_eq!(selected.len(), full.len());
            for (a, b) in selected.iter().zip(full) {
                assert_eq!(a.t, b.t);
                for (a, b) in a.boundaries.iter().zip(&b.boundaries) {
                    assert_eq!(a.point, b.point);
                    assert_eq!(a.height, b.height);
                }
            }
        }
    }
    #[test]
    fn placement_rows_do_not_subdivide_unrelated_large_curves() {
        let program = crate::parse("road street: carlane; end; small=street(0,0,1,0); unrelated=street(0,10,1000000,1000000,-1000000,1000000,1,10);").unwrap();
        let projection = map::project(&program);
        let (rows, resolved) =
            RoadGeometry::placement_cross_sections(&program, &projection, 0).unwrap();
        assert!(resolved);
        assert!(rows.len() < 100);
    }
    #[test]
    fn clips_a_short_opening_inside_a_long_wall_piece() {
        let triangle = Triangle {
            vertices: [
                vertex(0.2, -1.0, 0.3),
                vertex(0.22, -1.0, 0.3),
                vertex(0.2, 1.0, 0.3),
            ],
        };
        let (lo, hi) = clip(
            vertex(-1.0, 0.0, 0.3),
            vertex(1.0, 0.0, 0.3),
            triangle,
            0.012,
            0.012,
        )
        .unwrap();
        assert!((lo - 0.6).abs() < 1e-6);
        assert!((hi - 0.605).abs() < 1e-6);
        // A midpoint-only test would miss this entire opening.
        assert!(lo > 0.5);
    }
    #[test]
    fn clipping_respects_vertical_separation() {
        let triangle = Triangle {
            vertices: [
                vertex(-1.0, -1.0, 0.0),
                vertex(1.0, -1.0, 0.0),
                vertex(0.0, 1.0, 0.0),
            ],
        };
        assert!(clip(
            vertex(-1.0, 0.0, 0.3),
            vertex(1.0, 0.0, 0.3),
            triangle,
            0.012,
            0.012
        )
        .is_none());
    }
}
