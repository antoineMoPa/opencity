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
// Land polygons sit 0.02 below authored pavement; leave that ground-road offset clear.
const SAFETY_CLEARANCE: f32 = 0.02001;

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
pub const BARRIER_HEIGHT: f32 = 0.011;
pub const BARRIER_HALF_WIDTH: f32 = 0.002;
pub fn paved_material(material: crate::network::Material) -> crate::network::Material {
    use crate::network::Material;
    match material {
        Material::Verge
        | Material::RoadElement(RoadElementKind::Verge | RoadElementKind::Middle) => {
            Material::Asphalt
        }
        other => other,
    }
}
impl Wall {
    /// Closed concrete barrier prism, in map coordinates (Z up).
    pub fn barrier_faces(&self) -> [[[f32; 3]; 4]; 6] {
        let dx = self.end.point.x - self.start.point.x;
        let dy = self.end.point.y - self.start.point.y;
        let length = dx.hypot(dy).max(1e-8);
        let nx = -dy / length * BARRIER_HALF_WIDTH;
        let ny = dx / length * BARRIER_HALF_WIDTH;
        let point = |v: Vertex, sign: f32, h: f32| {
            [v.point.x + nx * sign, v.point.y + ny * sign, v.height + h]
        };
        let [a, b, c, d] = [
            point(self.start, -1., 0.),
            point(self.end, -1., 0.),
            point(self.end, 1., 0.),
            point(self.start, 1., 0.),
        ];
        let [e, f, g, h] = [
            point(self.start, -1., BARRIER_HEIGHT),
            point(self.end, -1., BARRIER_HEIGHT),
            point(self.end, 1., BARRIER_HEIGHT),
            point(self.start, 1., BARRIER_HEIGHT),
        ];
        [
            [a, b, c, d],
            [e, h, g, f],
            [a, e, f, b],
            [d, c, g, h],
            [a, d, h, e],
            [b, f, g, c],
        ]
        .map(|[a, b, c, d]| [d, c, b, a])
    }
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
    pavement: Vec<Triangle>,
    edges: [Vec<Vertex>; 2],
}
pub struct RoadGeometry {
    pub walls: Vec<Wall>,
    pub safety_walls: Vec<Wall>,
    paved_edges: Vec<bool>,
    edge_walls: Vec<Wall>,
    protected_junctions: std::collections::HashSet<String>,
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
            if let Some(road) = definition(program, p).filter(|_| p.road_name == b.road_name) {
                let index = projection
                    .segments
                    .iter()
                    .position(|s| std::ptr::eq(s, p))
                    .unwrap();
                if crate::road_profiles::attachment_footprint(projection, index, road)
                    .segments
                    .iter()
                    .any(|s| s.source == b.source)
                {
                    return true;
                }
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
    /// Apply terrain-relative road protection after loading the elevation asset.
    pub fn configure_safety(
        &mut self,
        program: &Program,
        projection: &Projection,
        elevation: Option<&crate::heightmap::ElevationMap>,
    ) {
        let features: Vec<_> = crate::project_landscape_features(program)
            .features
            .into_iter()
            .filter(|feature| {
                matches!(
                    feature.kind,
                    crate::LandscapeKind::Land
                        | crate::LandscapeKind::Beach
                        | crate::LandscapeKind::Concrete
                        | crate::LandscapeKind::Terrain
                )
            })
            .collect();
        let ground = |v: Vertex| {
            let p = v.point;
            let mut height = elevation.and_then(|e| e.height_at([p.x, p.y]));
            for feature in &features {
                let mut inside = false;
                for (a, b) in feature
                    .points
                    .iter()
                    .zip(feature.points.iter().cycle().skip(1))
                    .take(feature.points.len())
                {
                    if (a[1] > p.y) != (b[1] > p.y)
                        && p.x < (b[0] - a[0]) * (p.y - a[1]) / (b[1] - a[1]) + a[0]
                    {
                        inside = !inside;
                    }
                }
                if inside {
                    let h = feature.elevation - 0.02 + feature.height.unwrap_or(0.);
                    height = Some(height.map_or(h, |prior| prior.max(h)));
                }
            }
            height.unwrap_or(0.)
        };
        let protected = |index: usize| {
            definition(program, &projection.segments[index])
                .is_some_and(|r| r.role != RoadRole::Ordinary)
        };
        self.safety_walls = self
            .edge_walls
            .iter()
            .copied()
            .filter(|w| {
                protected(w.segment)
                    || (w.start.height - ground(w.start) > SAFETY_CLEARANCE
                        && w.end.height - ground(w.end) > SAFETY_CLEARANCE)
            })
            .collect();
        self.paved_edges = self
            .surfaces
            .iter()
            .enumerate()
            .map(|(i, s)| {
                protected(i)
                    || s.rows.iter().any(|r| {
                        r.boundaries
                            .iter()
                            .any(|v| v.height - ground(*v) > SAFETY_CLEARANCE)
                    })
            })
            .collect();
        for (index, surface) in self.surfaces.iter().enumerate() {
            let Some(road) = definition(program, &projection.segments[index]) else {
                continue;
            };
            for (band, element) in road.elements.iter().enumerate() {
                if element.kind != RoadElementKind::Barrier
                    && !(road.role == RoadRole::Highway && element.kind == RoadElementKind::Middle)
                {
                    continue;
                }
                for pair in surface.rows.windows(2) {
                    self.safety_walls.push(Wall {
                        segment: index,
                        start: mix(pair[0].boundaries[band], pair[0].boundaries[band + 1], 0.5),
                        end: mix(pair[1].boundaries[band], pair[1].boundaries[band + 1], 0.5),
                    });
                }
            }
        }
        self.protected_junctions.clear();
        for junction in map::intersections(program, projection) {
            let mut seams = Vec::new();
            for (index, segment) in projection.segments.iter().enumerate() {
                for (reference, end) in [(&segment.start_ref, false), (&segment.end_ref, true)] {
                    if !reference.as_ref().is_some_and(|r| {
                        r == &junction.point_ref || junction.ports.iter().any(|p| &p.point_ref == r)
                    }) {
                        continue;
                    }
                    let rows = self.cross_sections(index);
                    if let Some(row) = if end { rows.last() } else { rows.first() } {
                        if let (Some(&a), Some(&b)) =
                            (row.boundaries.first(), row.boundaries.last())
                        {
                            seams.push((index, a, b));
                        }
                    }
                }
            }
            if !seams.iter().any(|&(index, a, b)| {
                protected(index)
                    || a.height - ground(a) > SAFETY_CLEARANCE
                    || b.height - ground(b) > SAFETY_CLEARANCE
            }) {
                continue;
            }
            let Some(instance) =
                crate::network::default_four_way_instance(program, projection, &junction)
                    .or_else(|| {
                        crate::network::default_three_way_instance(program, projection, &junction)
                    })
                    .or_else(|| {
                        crate::network::default_right_turn_instance(program, projection, &junction)
                    })
            else {
                continue;
            };
            self.protected_junctions.insert(junction.point_ref.clone());
            let half = map::intersection_footprint_half(
                &junction,
                seams
                    .iter()
                    .map(|(_, a, b)| distance(a.point, b.point) * 0.5)
                    .fold(0., f32::max),
            );
            let height = |point: WorldPoint| {
                let mut weight = 0.;
                let mut sum = 0.;
                for &(_, a, b) in &seams {
                    let dx = b.point.x - a.point.x;
                    let dy = b.point.y - a.point.y;
                    let t = (((point.x - a.point.x) * dx + (point.y - a.point.y) * dy)
                        / (dx * dx + dy * dy).max(1e-20))
                    .clamp(0., 1.);
                    let d = (point.x - a.point.x - dx * t).powi(2)
                        + (point.y - a.point.y - dy * t).powi(2);
                    let h = a.height + (b.height - a.height) * t;
                    if d < 1e-12 {
                        return h;
                    }
                    sum += h / d;
                    weight += 1. / d;
                }
                sum / weight
            };
            for edge in crate::junction_mesh::boundary(&instance.geometry, half) {
                let vertices = edge.map(|p| {
                    let p = instance.world_point(p);
                    let point = WorldPoint { x: p.x, y: p.y };
                    Vertex {
                        point,
                        height: height(point),
                    }
                });
                let middle = mix(vertices[0], vertices[1], 0.5).point;
                let mouth = seams.iter().any(|&(_, a, b)| {
                    let dx = b.point.x - a.point.x;
                    let dy = b.point.y - a.point.y;
                    let t = ((middle.x - a.point.x) * dx + (middle.y - a.point.y) * dy)
                        / (dx * dx + dy * dy).max(1e-20);
                    (-1e-4..=1.0001).contains(&t)
                        && (middle.x - a.point.x - dx * t).hypot(middle.y - a.point.y - dy * t)
                            < 1e-5
                });
                if !mouth {
                    self.safety_walls.push(Wall {
                        segment: seams[0].0,
                        start: vertices[0],
                        end: vertices[1],
                    });
                }
            }
        }
    }
    pub fn junction_is_protected(&self, reference: &str) -> bool {
        self.protected_junctions.contains(reference)
    }
    pub fn surface_element(&self, segment: usize, kind: RoadElementKind) -> RoadElementKind {
        if self.paved_edges[segment]
            && matches!(kind, RoadElementKind::Verge | RoadElementKind::Middle)
        {
            RoadElementKind::CarLane
        } else {
            kind
        }
    }
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
                        pavement: vec![],
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
                let mut pavement = Vec::new();
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
                            if element.kind == RoadElementKind::CarLane
                                || (road.role != RoadRole::Ordinary
                                    && element.kind == RoadElementKind::Verge)
                            {
                                pavement.push(triangle);
                            }
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
                    pavement,
                    edges,
                }
            })
            .collect();
        let mut connections = vec![Vec::new(); surfaces.len()];
        let joins = crate::road_profiles::road_joins(projection);
        for (mouth, segment) in projection.segments.iter().enumerate() {
            if !definition(program, segment).is_some_and(|r| {
                r.role == RoadRole::Merge && crate::road_profiles::merge_profile_is_paved(r)
            }) {
                continue;
            }
            for parent in 0..surfaces.len() {
                if parent != mouth && attached_parent(program, projection, mouth, parent) {
                    connections[parent].push(mouth);
                    connections[mouth].push(parent);
                    // A shoulder can still overlap the parent just past the
                    // shared mouth/ramp seam. Clip against that continuous
                    // pavement too, so no parent wall ends inside the shoulder.
                    for &(incoming, outgoing) in &joins {
                        let ramp = if incoming == mouth {
                            outgoing
                        } else if outgoing == mouth {
                            incoming
                        } else {
                            continue;
                        };
                        if definition(program, &projection.segments[ramp])
                            .is_some_and(|r| r.role == RoadRole::Ramp)
                            && crate::road_profiles::straight_join(
                                &projection.segments[incoming],
                                &projection.segments[outgoing],
                            )
                        {
                            connections[parent].push(ramp);
                            connections[ramp].push(parent);
                        }
                    }
                }
            }
        }
        for connected in &mut connections {
            connected.sort_unstable();
            connected.dedup();
        }
        let mut walls = Vec::new();
        for (index, surface) in surfaces.iter().enumerate() {
            for edge in &surface.edges {
                for pair in edge.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    let mut cuts = Vec::new();
                    for &other in &connections[index] {
                        for triangle in &surfaces[other].pavement {
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
            edge_walls: walls.clone(),
            protected_junctions: Default::default(),
            walls: walls
                .into_iter()
                .filter(|w| w.start.height.max(w.end.height) > 0.)
                .collect(),
            safety_walls: Vec::new(),
            paved_edges: vec![false; surfaces.len()],
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
                let is_merge_shoulder = definition(program, &projection.segments[other])
                    .is_some_and(|r| r.role == RoadRole::Merge)
                    && attached_parent(program, projection, other, lane);
                'pair: for &(band, kind) in &obstacle.forbidden {
                    if is_merge_shoulder && kind == RoadElementKind::Verge {
                        continue;
                    }
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
