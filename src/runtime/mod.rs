//! Renderer-independent game data. Map coordinates are X east, Y north, Z up.
//! A map unit is deliberately not assigned a physical length; consumers choose a scale.
//! Load once, query borrowed structured records, and request camera-local triangle meshes.
mod bridges;
mod geometry;
use crate::junction_mesh;
mod building_details;
mod mesh;
mod svg;
use crate::{
    heightmap::{ElevationMap, ElevationMapSpec},
    map, network, LandscapeFeature, LandscapeKind, Program, RoadDefinition,
};
pub use building_details::{
    BuildingDetailConfig, BuildingPattern, FacadeKind, FencePattern, GardenEdge, GardenProp,
    RoofKind, StairKind,
};
pub use mesh::{Mesh, StreetFeature, StreetFeatureKind, Vertex};
use std::{collections::HashMap, path::Path};

/// Optional standalone wgpu shader: Z-up positions, linear RGB, surface metadata.
/// Group 0 binding 0: view-projection, light view-projection, eye/time, sun/exposure, daylight/sky.
/// Group 1: 2048² shadow depth texture and comparison sampler (see shader).
pub const GAME_SHADER: &str = include_str!("game.wgsl");
/// Original editor shader, for consumers implementing its documented bindings.
pub const EDITOR_SHADER: &str = include_str!("../bin/editor/shaders/scene.wgsl");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SurfaceKind {
    Asphalt = 0,
    Grass = 1,
    Sidewalk = 2,
    Building = 3,
    Water = 4,
    Sand = 5,
    Marking = 6,
    Prop = 7,
    /// A window pane; optional local UV/room metadata enables shader interiors.
    Window = 8,
    /// An artificial light source, such as a street lamp.
    Light = 9,
    /// Roof covering with texture scale supplied in surface metadata.
    Roof = 10,
    /// A double-sided fence plane with local pattern coordinates and cutout gaps.
    Fence = 11,
}
impl SurfaceKind {
    /// Dry-surface tire friction coefficient, used by the example vehicle.
    pub fn friction(self) -> f32 {
        match self {
            Self::Asphalt => 0.95,
            Self::Grass => 0.48,
            Self::Sand => 0.38,
            Self::Water => 0.12,
            Self::Sidewalk => 0.8,
            _ => 0.7,
        }
    }
    pub fn rolling_resistance(self) -> f32 {
        match self {
            Self::Grass => 0.065,
            Self::Sand => 0.11,
            Self::Water => 0.2,
            _ => 0.015,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Road {
    pub segment: map::Segment,
    pub definition: RoadDefinition,
    pub bounds: [f32; 4],
    triangles: std::ops::Range<usize>,
}
#[derive(Clone, Debug)]
pub struct RoutePath {
    pub name: String,
    pub points: Vec<[f32; 3]>,
    pub component: String,
}
#[derive(Clone, Copy, Debug)]
pub enum Region {
    WholeMap,
    Radius { center: [f32; 2], radius: f32 },
}
impl Region {
    pub fn radius(center: [f32; 2], radius: f32) -> Result<Self, String> {
        if !center.iter().all(|v| v.is_finite()) || !radius.is_finite() || radius < 0. {
            return Err("query needs finite coordinates and a nonnegative radius".into());
        }
        Ok(Self::Radius { center, radius })
    }
    fn valid(self) -> bool {
        match self {
            Self::WholeMap => true,
            Self::Radius { center, radius } => Self::radius(center, radius).is_ok(),
        }
    }
    fn intersects_bounds(self, b: [f32; 4]) -> bool {
        if !self.valid() || !b.iter().all(|v| v.is_finite()) {
            return false;
        }
        match self {
            Self::WholeMap => true,
            Self::Radius {
                center: c,
                radius: r,
            } => {
                (c[0] - c[0].clamp(b[0], b[2])).powi(2) + (c[1] - c[1].clamp(b[1], b[3])).powi(2)
                    <= r * r
            }
        }
    }
}
pub struct Query<'a> {
    pub roads: Vec<(usize, &'a Road)>,
    pub paths: Vec<(usize, &'a RoutePath)>,
    pub buildings: Vec<(usize, &'a LandscapeFeature)>,
    pub landscape: Vec<(usize, &'a LandscapeFeature)>,
    pub junctions: Vec<&'a map::Intersection>,
}
#[derive(Clone, Copy, Debug)]
pub struct SurfaceSample {
    pub height: f32,
    pub gradient: [f32; 2],
    pub kind: SurfaceKind,
    pub road: Option<usize>,
}
#[derive(Clone)]
struct Triangle {
    points: [[f32; 3]; 3],
    kind: SurfaceKind,
    road: Option<usize>,
    color: [f32; 3],
    data: [f32; 3],
}
/// Compiled snapshot. IDs are indices stable for this snapshot, never source-file offsets.
pub struct City {
    pub roads: Vec<Road>,
    pub road_barriers: Vec<crate::road_geometry::Wall>,
    pub paths: Vec<RoutePath>,
    pub landscape: Vec<LandscapeFeature>,
    pub junctions: Vec<map::Intersection>,
    pub components: Vec<network::NetworkInstance>,
    pub elevation: Option<ElevationMap>,
    pub bounds: [f32; 4],
    triangles: Vec<Triangle>,
    cells: HashMap<(i32, i32), Vec<usize>>,
    barrier_cells: HashMap<(i32, i32), Vec<usize>>,
}
impl City {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let source =
            std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_program(
            &crate::parse(&source).map_err(|e| e.to_string())?,
            path.parent(),
        )
    }
    /// `base` resolves elevation PNGs relative to the authored map, not process CWD.
    pub fn from_program(program: &Program, base: Option<&Path>) -> Result<Self, String> {
        Self::from_program_with_elevation_loader(program, |spec| spec.load(base))
    }
    /// Compile with a caller-supplied elevation loader, including embedded/browser assets.
    pub fn from_program_with_elevation_loader(
        program: &Program,
        load: impl FnOnce(&ElevationMapSpec) -> Result<ElevationMap, String>,
    ) -> Result<Self, String> {
        let projection = map::project(program);
        if projection.unresolved > 0 {
            return Err(format!(
                "{} unresolved road placements",
                projection.unresolved
            ));
        }
        let elevation = ElevationMapSpec::from_program(program)?
            .map(|s| load(&s))
            .transpose()?;
        let landscape = crate::project_landscape_features(program);
        if !landscape.errors.is_empty() {
            return Err(format!("landscape: {:?}", landscape.errors));
        }
        let components = network::project(program);
        if !components.errors.is_empty() {
            return Err(components.errors.join("; "));
        }
        let mut city = Self {
            roads: Vec::new(),
            road_barriers: Vec::new(),
            paths: Vec::new(),
            landscape: landscape.features,
            junctions: map::intersections(program, &projection),
            components: components.instances,
            elevation,
            bounds: [-1., -1., 1., 1.],
            triangles: Vec::new(),
            cells: HashMap::new(),
            barrier_cells: HashMap::new(),
        };
        geometry::compile(&mut city, program, &projection)?;
        Ok(city)
    }
    /// Radius tests use road footprints, path segments and polygons (including containment).
    /// Invalid directly-constructed radius regions produce an empty result.
    pub fn query(&self, region: Region) -> Query<'_> {
        let roads = self
            .roads
            .iter()
            .enumerate()
            .filter(|(_, r)| region.intersects_bounds(r.bounds))
            .filter(|(id, _)| {
                self.triangles[self.roads[*id].triangles.clone()]
                    .iter()
                    .any(|t| geometry::polygon_region(&t.points.map(|p| [p[0], p[1]]), region))
            })
            .collect();
        let paths = self
            .paths
            .iter()
            .enumerate()
            .filter(|(_, p)| geometry::path_region(&p.points, region))
            .collect();
        let mut buildings = Vec::new();
        let mut landscape = Vec::new();
        for (i, f) in self
            .landscape
            .iter()
            .enumerate()
            .filter(|(_, f)| geometry::polygon_region(&f.points, region))
        {
            if f.kind == LandscapeKind::Building {
                buildings.push((i, f));
            } else {
                landscape.push((i, f));
            }
        }
        let junctions = self
            .junctions
            .iter()
            .filter(|j| {
                region.intersects_bounds([
                    j.position.x - j.footprint_half,
                    j.position.y - j.footprint_half,
                    j.position.x + j.footprint_half,
                    j.position.y + j.footprint_half,
                ])
            })
            .collect();
        Query {
            roads,
            paths,
            buildings,
            landscape,
            junctions,
        }
    }
    /// Highest supporting surface below `ceiling`. Use chassis height + step tolerance
    /// to stay underneath bridges. Buildings are colliders, never tire support surfaces.
    pub fn surface_at(&self, point: [f32; 2], ceiling: f32) -> Option<SurfaceSample> {
        if !point.iter().all(|v| v.is_finite()) || ceiling.is_nan() {
            return None;
        }
        let mut best = self
            .elevation
            .as_ref()
            .and_then(|e| e.height_at(point))
            .map(|height| {
                let d = 0.001;
                let e = self.elevation.as_ref().unwrap();
                let gradient = [
                    (e.height_at([point[0] + d, point[1]]).unwrap_or(height)
                        - e.height_at([point[0] - d, point[1]]).unwrap_or(height))
                        / (2. * d),
                    (e.height_at([point[0], point[1] + d]).unwrap_or(height)
                        - e.height_at([point[0], point[1] - d]).unwrap_or(height))
                        / (2. * d),
                ];
                SurfaceSample {
                    height,
                    gradient,
                    kind: SurfaceKind::Grass,
                    road: None,
                }
            })
            .filter(|s| s.height <= ceiling);
        for &i in self.cells.get(&geometry::cell(point)).into_iter().flatten() {
            let t = &self.triangles[i];
            if matches!(
                t.kind,
                SurfaceKind::Building | SurfaceKind::Prop | SurfaceKind::Marking
            ) {
                continue;
            }
            if let Some((height, gradient)) = geometry::triangle_height(t.points, point) {
                if height <= ceiling
                    && best.is_none_or(|b| {
                        height > b.height + 1e-7
                            || ((height - b.height).abs() <= 1e-7
                                && b.kind == SurfaceKind::Grass
                                && t.kind != SurfaceKind::Grass)
                    })
                {
                    best = Some(SurfaceSample {
                        height,
                        gradient,
                        kind: t.kind,
                        road: t.road,
                    });
                }
            }
        }
        if let Some(e) = &self.elevation {
            if let (Some(level), Some(b)) = (e.spec.water_level, best.as_mut()) {
                if b.height < level {
                    b.kind = SurfaceKind::Water;
                }
            }
        }
        best
    }
    /// Deepest circle contact with a road barrier; normal and depth are in map units.
    pub fn road_barrier_contact(
        &self,
        point: [f32; 2],
        radius: f32,
        bottom: f32,
        top: f32,
    ) -> Option<([f32; 2], f32)> {
        let reach = radius + crate::road_geometry::BARRIER_HALF_WIDTH;
        let a = geometry::cell([point[0] - reach, point[1] - reach]);
        let b = geometry::cell([point[0] + reach, point[1] + reach]);
        let mut best = None;
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                for &id in self.barrier_cells.get(&(x, y)).into_iter().flatten() {
                    let wall = &self.road_barriers[id];
                    let a = wall.start.point;
                    let b = wall.end.point;
                    let d = [b.x - a.x, b.y - a.y];
                    let length_squared = d[0] * d[0] + d[1] * d[1];
                    if length_squared < 1e-16 {
                        continue;
                    }
                    let t = (((point[0] - a.x) * d[0] + (point[1] - a.y) * d[1]) / length_squared)
                        .clamp(0., 1.);
                    let height = wall.start.height + (wall.end.height - wall.start.height) * t;
                    if height >= top || height + crate::road_geometry::BARRIER_HEIGHT <= bottom {
                        continue;
                    }
                    let delta = [point[0] - a.x - d[0] * t, point[1] - a.y - d[1] * t];
                    let distance = delta[0].hypot(delta[1]);
                    let depth = reach - distance;
                    if depth <= 0. || best.is_some_and(|(_, prior)| prior >= depth) {
                        continue;
                    }
                    let normal = if distance > 1e-8 {
                        delta.map(|v| v / distance)
                    } else {
                        let length = length_squared.sqrt();
                        [-d[1] / length, d[0] / length]
                    };
                    best = Some((normal, depth));
                }
            }
        }
        best
    }
    /// Circle/footprint overlap plus vertical interval for vehicle collision checks.
    pub fn building_collision(&self, point: [f32; 2], radius: f32, bottom: f32, top: f32) -> bool {
        self.landscape.iter().any(|f| {
            f.kind == LandscapeKind::Building
                && f.elevation < top
                && f.elevation + f.height.unwrap_or(0.) > bottom
                && geometry::polygon_region(
                    &f.points,
                    Region::Radius {
                        center: point,
                        radius,
                    },
                )
        })
    }
}
