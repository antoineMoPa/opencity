//! Explicit ports, paths, and permitted traversals for reusable road components.
mod defaults;
mod materials;
mod profile_binding;
mod turns;
pub use materials::road_color;
pub use turns::default_right_turn_instance;
mod geometry;
pub use defaults::{
    default_four_way, default_four_way_footprint_factor, default_four_way_instance,
    default_three_way_instance,
};
mod instances;
mod resolve;
mod svg;
mod writer;

pub use geometry::{band_quads, point_at_distance, slice_path};
pub use instances::{project, NetworkInstance, NetworkProjection};
pub use resolve::resolve;
pub use svg::{to_svg, to_svg_with_options};
pub(crate) use writer::write_definition;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
impl Point {
    pub fn distance(self, other: Self) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2) + (self.z - other.z).powi(2))
            .sqrt()
    }
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
            z: self.z + (other.z - self.z) * t,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentKind {
    Intersection,
    Interchange,
    RoadSegment,
}
impl ComponentKind {
    pub fn from_keyword(word: &str) -> Option<Self> {
        match word {
            "intersection" => Some(Self::Intersection),
            "interchange" => Some(Self::Interchange),
            "road_segment" => Some(Self::RoadSegment),
            _ => None,
        }
    }
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Intersection => "intersection",
            Self::Interchange => "interchange",
            Self::RoadSegment => "road_segment",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneRole {
    In,
    Out,
}
impl LaneRole {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Out => "out",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heading {
    North,
    East,
    South,
    West,
}
impl Heading {
    pub fn from_keyword(word: &str) -> Option<Self> {
        match word {
            "north" => Some(Self::North),
            "east" => Some(Self::East),
            "south" => Some(Self::South),
            "west" => Some(Self::West),
            _ => None,
        }
    }
    pub fn keyword(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::East => "east",
            Self::South => "south",
            Self::West => "west",
        }
    }
    pub fn vector(self) -> [f32; 2] {
        match self {
            Self::North => [0., -1.],
            Self::East => [1., 0.],
            Self::South => [0., 1.],
            Self::West => [-1., 0.],
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Port {
    pub name: String,
    pub role: LaneRole,
    pub position: Point,
    pub heading: Heading,
    pub width: Option<f32>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Endpoint {
    Point(Point),
    Reference(String),
}
#[derive(Clone, Debug, PartialEq)]
pub enum PathSegment {
    Line(Endpoint),
    Cubic {
        first: Point,
        second: Point,
        end: Endpoint,
    },
    Arc {
        end: Endpoint,
        radius: f32,
        clockwise: bool,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub name: String,
    pub start: Endpoint,
    pub segments: Vec<PathSegment>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    pub name: String,
    pub path: String,
    pub distance: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Bound {
    Start,
    End,
    Distance(f32),
    Anchor(String),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Traversal {
    pub path: String,
    pub start: Bound,
    pub end: Bound,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Movement {
    pub class: String,
    pub from: String,
    pub to: String,
    pub via: Vec<Traversal>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandKind {
    Surface,
    Marking,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Center,
    Left,
    Right,
}
impl Side {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Center => "center",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    RoadElement(crate::RoadElementKind),
    Asphalt,
    Sidewalk,
    Concrete,
    Verge,
    White,
    Yellow,
    Red,
    Blue,
}
impl Material {
    pub fn from_keyword(word: &str) -> Option<Self> {
        match word {
            "asphalt" => Some(Self::Asphalt),
            "sidewalk" => Some(Self::Sidewalk),
            "concrete" => Some(Self::Concrete),
            "verge" => Some(Self::Verge),
            "white" => Some(Self::White),
            "yellow" => Some(Self::Yellow),
            "red" => Some(Self::Red),
            "blue" => Some(Self::Blue),
            _ => crate::RoadElementKind::from_keyword(word).map(Self::RoadElement),
        }
    }
    pub fn keyword(self) -> &'static str {
        match self {
            Self::RoadElement(kind) => kind.keyword(),
            Self::Asphalt => "asphalt",
            Self::Sidewalk => "sidewalk",
            Self::Concrete => "concrete",
            Self::Verge => "verge",
            Self::White => "white",
            Self::Yellow => "yellow",
            Self::Red => "red",
            Self::Blue => "blue",
        }
    }
    pub fn rgb(self) -> [u8; 3] {
        self.rgb_for_theme(false)
    }
    pub fn rgb_for_theme(self, light: bool) -> [u8; 3] {
        match self {
            Self::RoadElement(kind) => road_color(kind, light),
            Self::Asphalt => road_color(crate::RoadElementKind::CarLane, light),
            Self::Sidewalk => road_color(crate::RoadElementKind::Sidewalk, light),
            Self::Concrete => [151, 159, 165],
            Self::Verge => road_color(crate::RoadElementKind::Verge, light),
            Self::White => [245, 246, 239],
            Self::Yellow => [244, 194, 76],
            Self::Red => [220, 87, 79],
            Self::Blue => [83, 165, 228],
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub name: String,
    pub kind: BandKind,
    pub path: String,
    pub width: f32,
    pub side: Side,
    pub material: Material,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ComponentTemplateBody {
    Network(String),
    Junction(Vec<crate::Call>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentTemplate {
    pub parameters: Vec<crate::BlockParameter>,
    /// Explicit path source or a typed road-profile junction layout.
    pub body: ComponentTemplateBody,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    pub template: Option<ComponentTemplate>,
    pub kind: ComponentKind,
    pub name: Option<String>,
    /// An inline placement; absent for a reusable named definition.
    pub origin: Option<Point>,
    pub ports: Vec<Port>,
    pub paths: Vec<Path>,
    pub anchors: Vec<Anchor>,
    pub movements: Vec<Movement>,
    pub bands: Vec<Band>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedPath {
    pub name: String,
    pub points: Vec<Point>,
}
impl ResolvedPath {
    pub fn length(&self) -> f32 {
        self.points.windows(2).map(|p| p[0].distance(p[1])).sum()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedMovement {
    pub movement: Movement,
    pub points: Vec<Point>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    /// Profile-bound surface quads; absent for constant-width authored bands.
    pub band_meshes: Vec<BandMesh>,
    pub ports: Vec<Port>,
    pub paths: Vec<ResolvedPath>,
    pub anchors: Vec<(String, Point)>,
    pub movements: Vec<ResolvedMovement>,
    pub bands: Vec<Band>,
    pub warnings: Vec<String>,
}
impl Resolved {
    pub fn band_quads(&self, band: &Band) -> Vec<[Point; 4]> {
        self.band_meshes
            .iter()
            .find(|mesh| mesh.name == band.name)
            .map(|mesh| mesh.quads.clone())
            .unwrap_or_else(|| {
                self.path(&band.path)
                    .map(|path| band_quads(&path.points, band.width, band.side))
                    .unwrap_or_default()
            })
    }
    pub fn path(&self, name: &str) -> Option<&ResolvedPath> {
        self.paths.iter().find(|path| path.name == name)
    }
    pub fn bounds(&self) -> [f32; 4] {
        let mut bounds = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for point in self
            .paths
            .iter()
            .flat_map(|path| &path.points)
            .chain(self.ports.iter().map(|port| &port.position))
        {
            bounds[0] = bounds[0].min(point.x);
            bounds[1] = bounds[1].min(point.y);
            bounds[2] = bounds[2].max(point.x);
            bounds[3] = bounds[3].max(point.y);
        }
        if !bounds[0].is_finite() {
            return [-1., -1., 1., 1.];
        }
        for band in &self.bands {
            {
                for quad in self.band_quads(band) {
                    for point in quad {
                        bounds[0] = bounds[0].min(point.x);
                        bounds[1] = bounds[1].min(point.y);
                        bounds[2] = bounds[2].max(point.x);
                        bounds[3] = bounds[3].max(point.y);
                    }
                }
            }
        }
        bounds
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BandMesh {
    pub name: String,
    pub quads: Vec<[Point; 4]>,
}
