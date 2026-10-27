//! Map geometry and editable road model.

use super::*;

#[derive(Clone, Copy)]
pub enum ElementEdit {
    Inserted(usize),
    Swapped(usize, usize),
    Removed(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitMode {
    Dedicated,
    KeepLane,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitSide {
    Left,
    Right,
}

impl ExitSide {
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

pub struct ExitPlacement {
    pub entry: SegmentSource,
    pub full: SegmentSource,
    pub through: Option<SegmentSource>,
    pub end_ref: String,
    pub side: ExitSide,
}

impl SplitMode {
    pub(super) fn identifier(self) -> &'static str {
        match self {
            Self::Dedicated => "dedicated",
            Self::KeepLane => "keep_lane",
        }
    }

    pub(super) fn from_identifier(value: &str) -> Option<Self> {
        match value {
            "dedicated" => Some(Self::Dedicated),
            "keep_lane" => Some(Self::KeepLane),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LaneSplit {
    pub parent: String,
    pub ramp: String,
    pub full_ramp: Option<String>,
    pub element_index: usize,
    pub t: f32,
    pub mode: SplitMode,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldPoint {
    pub x: f32,
    pub y: f32,
}

impl WorldPoint {
    pub fn translated(self, offset: Self) -> Self {
        Self {
            x: self.x + offset.x,
            y: self.y + offset.y,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentSource {
    TopLevel(usize),
    BlockInstance {
        instance: usize,
        block: usize,
        call: usize,
    },
}

impl SegmentSource {
    pub fn editable(self) -> bool {
        matches!(self, Self::TopLevel(_))
    }
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub source: SegmentSource,
    pub name: Option<String>,
    pub road_name: String,
    pub direction: RoadDirection,
    pub start: WorldPoint,
    pub end: WorldPoint,
    pub controls: [WorldPoint; 2],
    pub start_ref: Option<String>,
    pub end_ref: Option<String>,
    pub elevation: f32,
    pub start_z: f32,
    pub end_z: f32,
}

impl Segment {
    pub fn permits_travel(&self, from_start: bool) -> bool {
        self.direction.permits(from_start)
    }
    pub fn height_at(&self, t: f32) -> f32 {
        self.start_z + (self.end_z - self.start_z) * t.clamp(0.0, 1.0)
    }

    pub fn point_at(&self, t: f32) -> WorldPoint {
        let [first, second] = self.controls;
        let u = 1.0 - t;
        WorldPoint {
            x: u.powi(3) * self.start.x
                + 3.0 * u.powi(2) * t * first.x
                + 3.0 * u * t.powi(2) * second.x
                + t.powi(3) * self.end.x,
            y: u.powi(3) * self.start.y
                + 3.0 * u.powi(2) * t * first.y
                + 3.0 * u * t.powi(2) * second.y
                + t.powi(3) * self.end.y,
        }
    }

    pub fn tangent_at(&self, t: f32) -> WorldPoint {
        let u = 1.0 - t;
        WorldPoint {
            x: 3.0 * u * u * (self.controls[0].x - self.start.x)
                + 6.0 * u * t * (self.controls[1].x - self.controls[0].x)
                + 3.0 * t * t * (self.end.x - self.controls[1].x),
            y: 3.0 * u * u * (self.controls[0].y - self.start.y)
                + 6.0 * u * t * (self.controls[1].y - self.controls[0].y)
                + 3.0 * t * t * (self.end.y - self.controls[1].y),
        }
    }
}

#[derive(Clone)]
pub struct Projection {
    pub segments: Vec<Segment>,
    pub points: Vec<MapPoint>,
    pub unresolved: usize,
    // Intersection geometry belongs to the same immutable document projection.
    // Picking and painting must not expand component templates every frame.
    pub(super) intersections: Vec<Intersection>,
}

#[derive(Clone, Debug)]
pub struct MapPoint {
    pub name: String,
    pub position: WorldPoint,
    pub base: Option<WorldPoint>,
    pub base_direction: Option<WorldPoint>,
    pub direction: Option<WorldPoint>,
}

impl MapPoint {
    pub(super) fn control_from_stored(&self, stored: WorldPoint) -> WorldPoint {
        let base = self.base.unwrap_or(self.position);
        let dx = stored.x - base.x;
        let dy = stored.y - base.y;
        if let (Some(from), Some(to)) = (self.base_direction, self.direction) {
            let cosine = from.x * to.x + from.y * to.y;
            let sine = from.x * to.y - from.y * to.x;
            WorldPoint {
                x: self.position.x + dx * cosine - dy * sine,
                y: self.position.y + dx * sine + dy * cosine,
            }
        } else {
            WorldPoint {
                x: self.position.x + dx,
                y: self.position.y + dy,
            }
        }
    }

    pub(super) fn stored_from_control(&self, control: WorldPoint) -> WorldPoint {
        let base = self.base.unwrap_or(self.position);
        let dx = control.x - self.position.x;
        let dy = control.y - self.position.y;
        if let (Some(from), Some(to)) = (self.base_direction, self.direction) {
            let cosine = from.x * to.x + from.y * to.y;
            let sine = from.x * to.y - from.y * to.x;
            WorldPoint {
                x: base.x + dx * cosine + dy * sine,
                y: base.y - dx * sine + dy * cosine,
            }
        } else {
            WorldPoint {
                x: base.x + dx,
                y: base.y + dy,
            }
        }
    }
}
