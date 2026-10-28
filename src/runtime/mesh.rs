use super::*;
use std::collections::HashSet;
/// Packed f32-only layout: position(0), normal(1), linear color(2), surface(3).
/// `surface` = [SurfaceKind as f32, base elevation, floor spacing, facade material ID].
/// Fence planes use [Fence, horizontal pattern units, vertical fraction 0..1, FencePattern].
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub surface: [f32; 4],
}
#[derive(Default, Clone)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
}
impl Mesh {
    pub fn triangle(
        &mut self,
        p: [[f32; 3]; 3],
        color: [f32; 3],
        kind: SurfaceKind,
        data: [f32; 3],
    ) {
        let a = std::array::from_fn::<_, 3, _>(|i| p[1][i] - p[0][i]);
        let b = std::array::from_fn::<_, 3, _>(|i| p[2][i] - p[0][i]);
        let mut n = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let length = n.iter().map(|v| v * v).sum::<f32>().sqrt();
        if length < 1e-12 {
            return;
        }
        for v in &mut n {
            *v /= length;
        }
        if n[2] < 0. {
            n = n.map(|v| -v);
        }
        for position in p {
            self.vertices.push(Vertex {
                position,
                normal: n,
                color,
                surface: [kind as u32 as f32, data[0], data[1], data[2]],
            });
        }
    }
    pub fn quad(&mut self, p: [[f32; 3]; 4], color: [f32; 3], kind: SurfaceKind) {
        for ids in [[0, 1, 2], [0, 2, 3]] {
            self.triangle(ids.map(|i| p[i]), color, kind, [0.; 3]);
        }
    }
    pub fn cuboid(&mut self, min: [f32; 3], max: [f32; 3], color: [f32; 3], kind: SurfaceKind) {
        let p: Vec<_> = (0..8)
            .map(|i| {
                std::array::from_fn::<_, 3, _>(|a| if i & (1 << a) == 0 { min[a] } else { max[a] })
            })
            .collect();
        for ids in [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ] {
            self.quad(ids.map(|i| p[i]), color, kind);
        }
    }
    pub fn cylinder(
        &mut self,
        center: [f32; 3],
        radius: f32,
        height: f32,
        color: [f32; 3],
        kind: SurfaceKind,
    ) {
        let n = 12;
        for i in 0..n {
            let point = |i: usize, z: f32| {
                let a = i as f32 * std::f32::consts::TAU / n as f32;
                [
                    center[0] + radius * a.cos(),
                    center[1] + radius * a.sin(),
                    center[2] + z,
                ]
            };
            let a = point(i, 0.);
            let b = point(i + 1, 0.);
            let c = point(i + 1, height);
            let d = point(i, height);
            self.quad([a, b, c, d], color, kind);
            self.triangle([center, b, a], color, kind, [0.; 3]);
            self.triangle(
                [[center[0], center[1], center[2] + height], d, c],
                color,
                kind,
                [0.; 3],
            );
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreetFeatureKind {
    Lamp,
    Bench,
    Tree,
}
#[derive(Clone, Copy, Debug)]
pub struct StreetFeature {
    pub kind: StreetFeatureKind,
    pub position: [f32; 3],
    /// Rotation in radians from world +X. For sidewalk furniture, local +X
    /// faces the nearest travel lane; benches run along local Y with backs at -X.
    pub heading: f32,
}
impl City {
    /// A bounded mesh containing nearby compiled surfaces plus a heightmap grid.
    /// `terrain_step` chooses an integer PNG sample stride (map units).
    /// Steps below a pixel preserve exact terrain/support geometry; larger steps are LOD. Meshes are suitable for
    /// caching per fixed world tile, independent of camera orientation.
    pub fn mesh(&self, region: Region, terrain_step: f32) -> Result<Mesh, String> {
        if !region.valid() || !terrain_step.is_finite() || terrain_step < 0.005 {
            return Err("mesh needs a valid region and terrain step >= 0.005".into());
        }
        let mut mesh = Mesh::default();
        let mut ids = HashSet::new();
        match region {
            Region::WholeMap => ids.extend(0..self.triangles.len()),
            Region::Radius { center, radius } => {
                let b = self.bounds;
                let lo = geometry::cell([
                    (center[0] - radius).max(b[0]),
                    (center[1] - radius).max(b[1]),
                ]);
                let hi = geometry::cell([
                    (center[0] + radius).min(b[2]),
                    (center[1] + radius).min(b[3]),
                ]);
                for x in lo.0..=hi.0 {
                    for y in lo.1..=hi.1 {
                        if let Some(indices) = self.cells.get(&(x, y)) {
                            ids.extend(indices);
                        }
                    }
                }
            }
        }
        let mut ids: Vec<_> = ids.into_iter().collect();
        ids.sort_unstable();
        for i in ids {
            let t = &self.triangles[i];
            if geometry::polygon_region(&t.points.map(|p| [p[0], p[1]]), region) {
                mesh.triangle(t.points, t.color, t.kind, t.data);
            }
        }
        if let Some(e) = &self.elevation {
            let b = self.bounds;
            let view = match region {
                Region::WholeMap => b,
                Region::Radius {
                    center: c,
                    radius: r,
                } => [
                    (c[0] - r).max(b[0]),
                    (c[1] - r).max(b[1]),
                    (c[0] + r).min(b[2]),
                    (c[1] + r).min(b[3]),
                ],
            };
            // Keep the PNG's original cell boundaries and diagonal. At full
            // resolution, rendered terrain and wheel support are the same triangles.
            let stride = ((terrain_step / (e.spec.size[0] / 1023.).min(e.spec.size[1] / 1023.))
                .floor() as usize)
                .clamp(1, 1023);
            let lo = [
                (((view[0] - b[0]) / e.spec.size[0] * 1023.)
                    .floor()
                    .clamp(0., 1023.) as usize)
                    / stride
                    * stride,
                (((view[1] - b[1]) / e.spec.size[1] * 1023.)
                    .floor()
                    .clamp(0., 1023.) as usize)
                    / stride
                    * stride,
            ];
            let hi = [
                ((view[2] - b[0]) / e.spec.size[0] * 1023.)
                    .ceil()
                    .clamp(0., 1023.) as usize,
                ((view[3] - b[1]) / e.spec.size[1] * 1023.)
                    .ceil()
                    .clamp(0., 1023.) as usize,
            ];
            for x in (lo[0]..hi[0]).step_by(stride) {
                for y in (lo[1]..hi[1]).step_by(stride) {
                    let right = (x + stride).min(1023);
                    let top = (y + stride).min(1023);
                    let p = [
                        e.point(x, y),
                        e.point(right, y),
                        e.point(right, top),
                        e.point(x, top),
                    ];
                    mesh.quad(p, [0.12, 0.23, 0.075], SurfaceKind::Grass);
                    if let Some(level) = e.spec.water_level {
                        if p.iter().any(|p| p[2] < level) {
                            mesh.quad(
                                p.map(|p| [p[0], p[1], level]),
                                [0.025, 0.15, 0.20],
                                SurfaceKind::Water,
                            );
                        }
                    }
                }
            }
        }
        Ok(mesh)
    }
    /// Deterministic placements anchored to sidewalk bands, independent of camera motion.
    pub fn street_features(&self, region: Region) -> Vec<StreetFeature> {
        let mut out = Vec::new();
        if !region.valid() {
            return out;
        }
        for (id, r) in self
            .roads
            .iter()
            .enumerate()
            .filter(|(_, r)| region.intersects_bounds(r.bounds))
        {
            let s = &r.segment;
            let length = (0..32)
                .map(|i| {
                    let a = s.point_at(i as f32 / 32.);
                    let b = s.point_at((i + 1) as f32 / 32.);
                    (a.x - b.x).hypot(a.y - b.y)
                })
                .sum::<f32>();
            let count = (length / 0.28).floor() as usize;
            if count == 0 {
                continue;
            }
            for (band, _) in r
                .definition
                .elements
                .iter()
                .enumerate()
                .filter(|(_, e)| e.kind == crate::RoadElementKind::Sidewalk)
            {
                for i in 0..count {
                    let t = (i as f32 + 0.5) / count as f32;
                    if t * length < 0.08 || (1. - t) * length < 0.08 {
                        continue;
                    }
                    let Some(p) = map::element_position(s, &r.definition, band, t) else {
                        continue;
                    };
                    if !region.intersects_bounds([p.x, p.y, p.x, p.y]) {
                        continue;
                    }
                    // The models' local X axis crosses the sidewalk: lamp arms
                    // reach toward traffic and bench backs sit on the outer edge.
                    // Choose a travel band rather than assuming symmetric profiles.
                    let toward = r
                        .definition
                        .elements
                        .iter()
                        .enumerate()
                        .filter(|(_, element)| element.kind.is_travel_lane())
                        .filter_map(|(index, _)| map::element_position(s, &r.definition, index, t))
                        .min_by(|a, b| {
                            (a.x - p.x)
                                .hypot(a.y - p.y)
                                .total_cmp(&(b.x - p.x).hypot(b.y - p.y))
                        })
                        .unwrap_or_else(|| s.point_at(t));
                    let heading = (toward.y - p.y).atan2(toward.x - p.x);
                    let position = [p.x, p.y, s.height_at(t) + 0.0015];
                    let kind = if (i + id + band) % 4 == 0 {
                        StreetFeatureKind::Bench
                    } else {
                        StreetFeatureKind::Lamp
                    };
                    if !self.building_collision([p.x, p.y], 0.008, position[2], position[2] + 0.08)
                    {
                        out.push(StreetFeature {
                            kind,
                            position,
                            heading,
                        });
                    }
                }
            }
        }
        for (id, f) in self.landscape.iter().enumerate().filter(|(_, f)| {
            f.kind == LandscapeKind::Vegetation && geometry::polygon_region(&f.points, region)
        }) {
            let b = geometry::bounds(f.points.iter().copied());
            let nx = ((b[2] - b[0]) / 0.22).ceil() as usize;
            let ny = ((b[3] - b[1]) / 0.22).ceil() as usize;
            for x in 0..nx {
                for y in 0..ny {
                    let seed = (x as u32).wrapping_mul(73856093)
                        ^ (y as u32).wrapping_mul(19349663)
                        ^ (id as u32).wrapping_mul(83492791);
                    let p = [
                        b[0] + (x as f32 + 0.2 + (seed % 59) as f32 / 100.) * 0.22,
                        b[1] + (y as f32 + 0.2 + ((seed >> 8) % 59) as f32 / 100.) * 0.22,
                    ];
                    if geometry::contains(&f.points, p)
                        && region.intersects_bounds([p[0], p[1], p[0], p[1]])
                    {
                        if let Some(s) = self.surface_at(p, f32::INFINITY) {
                            if s.kind == SurfaceKind::Grass {
                                out.push(StreetFeature {
                                    kind: StreetFeatureKind::Tree,
                                    position: [p[0], p[1], s.height],
                                    heading: 0.,
                                });
                            }
                        }
                    }
                }
            }
        }
        out
    }
    pub fn detail_mesh(&self, region: Region) -> Mesh {
        let mut mesh = Mesh::default();
        for object in self.detail_objects(region) {
            mesh.vertices.extend(object.vertices);
        }
        mesh
    }

    /// Individual street furniture and trees, suitable for collision and removal.
    pub fn detail_objects(&self, region: Region) -> Vec<Mesh> {
        let mut objects = Vec::new();
        for f in self.street_features(region) {
            let mut mesh = Mesh::default();
            let p = f.position;
            let start = mesh.vertices.len();
            match f.kind {
                StreetFeatureKind::Lamp => {
                    mesh.cylinder(p, 0.0012, 0.075, [0.08, 0.10, 0.11], SurfaceKind::Prop);
                    mesh.cuboid(
                        [p[0] - 0.002, p[1] - 0.002, p[2] + 0.074],
                        [p[0] + 0.013, p[1] + 0.002, p[2] + 0.077],
                        [0.12, 0.14, 0.15],
                        SurfaceKind::Prop,
                    );
                    mesh.cuboid(
                        [p[0] + 0.005, p[1] - 0.002, p[2] + 0.072],
                        [p[0] + 0.013, p[1] + 0.002, p[2] + 0.074],
                        [0.8, 0.73, 0.45],
                        SurfaceKind::Light,
                    );
                }
                StreetFeatureKind::Bench => {
                    for y in [-0.007, 0., 0.007] {
                        mesh.cuboid(
                            [p[0] - 0.003, p[1] + y - 0.002, p[2] + 0.0045],
                            [p[0] + 0.003, p[1] + y + 0.002, p[2] + 0.006],
                            [0.28, 0.13, 0.045],
                            SurfaceKind::Prop,
                        );
                    }
                    for y in [-0.006, 0.006] {
                        mesh.cuboid(
                            [p[0] - 0.002, p[1] + y - 0.001, p[2]],
                            [p[0] + 0.002, p[1] + y + 0.001, p[2] + 0.005],
                            [0.08, 0.09, 0.09],
                            SurfaceKind::Prop,
                        );
                    }
                    mesh.cuboid(
                        [p[0] - 0.004, p[1] - 0.009, p[2] + 0.006],
                        [p[0] - 0.002, p[1] + 0.009, p[2] + 0.012],
                        [0.28, 0.13, 0.045],
                        SurfaceKind::Prop,
                    );
                }
                StreetFeatureKind::Tree => {
                    mesh.cylinder(p, 0.007, 0.075, [0.16, 0.08, 0.03], SurfaceKind::Prop);
                    mesh.cylinder(
                        [p[0], p[1], p[2] + 0.045],
                        0.045,
                        0.075,
                        [0.055, 0.17, 0.045],
                        SurfaceKind::Grass,
                    );
                }
            }
            let (sin, cos) = f.heading.sin_cos();
            for v in &mut mesh.vertices[start..] {
                let d = [v.position[0] - p[0], v.position[1] - p[1]];
                v.position[0] = p[0] + d[0] * cos - d[1] * sin;
                v.position[1] = p[1] + d[0] * sin + d[1] * cos;
                let n = v.normal;
                v.normal = [n[0] * cos - n[1] * sin, n[0] * sin + n[1] * cos, n[2]];
            }
            objects.push(mesh);
        }
        objects
    }
}
