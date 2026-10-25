//! Explicit mapping from road cross-sections to component approach bands.
use super::*;
use crate::{map, Program, RoadDefinition, RoadDirection, RoadElementKind, Statement};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ArmSide {
    North,
    East,
    South,
    West,
}
impl ArmSide {
    pub const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];
    pub fn index(self) -> usize {
        match self {
            Self::North => 0,
            Self::East => 1,
            Self::South => 2,
            Self::West => 3,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::North => "N",
            Self::East => "E",
            Self::South => "S",
            Self::West => "W",
        }
    }
    pub fn world(self) -> map::WorldPoint {
        match self {
            Self::North => map::WorldPoint { x: 0., y: 1. },
            Self::East => map::WorldPoint { x: 1., y: 0. },
            Self::South => map::WorldPoint { x: 0., y: -1. },
            Self::West => map::WorldPoint { x: -1., y: 0. },
        }
    }
    pub fn position(self, lateral: f32, distance: f32) -> Point {
        let (x, y) = match self {
            Self::North => (lateral, -distance),
            Self::East => (distance, lateral),
            Self::South => (lateral, distance),
            Self::West => (-distance, lateral),
        };
        Point { x, y, z: 0. }
    }
    fn matches(self, p: map::WorldPoint) -> bool {
        let d = self.world();
        if p.x.abs() > p.y.abs() {
            d.x != 0. && d.x.signum() == p.x.signum()
        } else {
            d.y != 0. && d.y.signum() == p.y.signum()
        }
    }
    fn incoming_low(self) -> bool {
        matches!(self, Self::North | Self::East)
    }
}
#[derive(Clone, Debug)]
pub(super) struct ProfileBand {
    pub kind: RoadElementKind,
    pub low: f32,
    pub high: f32,
    pub incoming: bool,
    pub outgoing: bool,
}
#[derive(Clone, Debug)]
pub(super) struct ArmProfile {
    pub side: ArmSide,
    pub bands: Vec<ProfileBand>,
    pub drive_low: f32,
    pub drive_high: f32,
    pub half: f32,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct OuterBand {
    pub kind: RoadElementKind,
    pub width: f32,
}
fn vehicle(kind: RoadElementKind) -> bool {
    matches!(kind, RoadElementKind::CarLane | RoadElementKind::BusLane)
}
impl ArmProfile {
    pub fn bind(
        program: &Program,
        projection: &map::Projection,
        intersection: &map::Intersection,
        side: ArmSide,
    ) -> Option<Self> {
        let selected_port = intersection
            .ports
            .iter()
            .find(|port| side.matches(port.direction));
        let segment = projection.segments.iter().find_map(|segment| {
            for end in [false, true] {
                let reference = if end {
                    &segment.end_ref
                } else {
                    &segment.start_ref
                };
                let belongs = reference.as_deref() == Some(intersection.point_ref.as_str())
                    || selected_port
                        .is_some_and(|port| reference.as_deref() == Some(port.point_ref.as_str()));
                let mut tangent = segment.tangent_at(if end { 1. } else { 0. });
                if end {
                    tangent.x = -tangent.x;
                    tangent.y = -tangent.y;
                }
                if belongs && side.matches(tangent) {
                    return Some((segment, end));
                }
            }
            None
        });
        let mapped = intersection.road_name_for_direction(side.world());
        let name = if mapped != intersection.road_name {
            mapped
        } else {
            segment
                .map(|(segment, _)| segment.road_name.as_str())
                .unwrap_or(mapped)
        };
        let road = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == name => Some(road),
                _ => None,
            })?;
        Self::from_road(road, side, segment)
    }
    fn from_road(
        road: &RoadDefinition,
        side: ArmSide,
        segment: Option<(&map::Segment, bool)>,
    ) -> Option<Self> {
        let half = road
            .elements
            .iter()
            .map(|e| map::element_world_width(e.kind))
            .sum::<f32>()
            / 2.;
        if half <= 0. {
            return None;
        }
        let sign = segment
            .map(|(segment, end)| {
                let tangent = segment.tangent_at(if end { 1. } else { 0. });
                let component = match side {
                    ArmSide::North | ArmSide::South => tangent.y,
                    ArmSide::East | ArmSide::West => tangent.x,
                };
                if component >= 0. {
                    1.
                } else {
                    -1.
                }
            })
            .unwrap_or_else(|| {
                if matches!(side, ArmSide::North | ArmSide::East) {
                    1.
                } else {
                    -1.
                }
            });
        let mut offset = -half;
        let mut bands = Vec::new();
        for (index, element) in road.elements.iter().enumerate() {
            let next = offset + map::element_world_width(element.kind);
            let a = offset * sign;
            let b = next * sign;
            let (low, high) = (a.min(b), a.max(b));
            let is_end = segment.is_some_and(|(_, end)| end);
            let (incoming, outgoing) = match road.lane_direction(index) {
                Some(RoadDirection::Forward) => (is_end, !is_end),
                Some(RoadDirection::Reverse) => (!is_end, is_end),
                Some(RoadDirection::Both) => {
                    let incoming = if side.incoming_low() {
                        low < 0.
                    } else {
                        high > 0.
                    };
                    (incoming, true)
                }
                None => (false, false),
            };
            bands.push(ProfileBand {
                kind: element.kind,
                low,
                high,
                incoming,
                outgoing,
            });
            offset = next;
        }
        bands.sort_by(|a, b| a.low.total_cmp(&b.low));
        let drive_low = bands
            .iter()
            .filter(|b| vehicle(b.kind))
            .map(|b| b.low)
            .min_by(f32::total_cmp)
            .unwrap_or(-half);
        let drive_high = bands
            .iter()
            .filter(|b| vehicle(b.kind))
            .map(|b| b.high)
            .max_by(f32::total_cmp)
            .unwrap_or(half);
        Some(Self {
            side,
            bands,
            drive_low,
            drive_high,
            half,
        })
    }
    pub fn outer(&self, positive: bool) -> Vec<OuterBand> {
        let selected = self.bands.iter().filter(|band| {
            if positive {
                band.low >= self.drive_high - 0.000001
            } else {
                band.high <= self.drive_low + 0.000001
            }
        });
        let mut result: Vec<_> = selected
            .map(|band| OuterBand {
                kind: band.kind,
                width: band.high - band.low,
            })
            .collect();
        if !positive {
            result.reverse();
        }
        result
    }
    pub fn lane_bounds(&self, incoming: bool) -> Option<(f32, f32)> {
        let mut bounds = (f32::INFINITY, f32::NEG_INFINITY);
        for band in self.bands.iter().filter(|band| {
            vehicle(band.kind)
                && (if incoming {
                    band.incoming
                } else {
                    band.outgoing
                })
        }) {
            let (mut low, mut high) = (band.low, band.high);
            if band.incoming && band.outgoing {
                let low_half = if incoming {
                    self.side.incoming_low()
                } else {
                    !self.side.incoming_low()
                };
                if low_half {
                    high = high.min(0.);
                } else {
                    low = low.max(0.);
                }
            }
            if high > low {
                bounds.0 = bounds.0.min(low);
                bounds.1 = bounds.1.max(high);
            }
        }
        (bounds.1 > bounds.0).then_some(bounds)
    }
}
/// Align outer bands while preserving each arm's inner-to-outer element order.
pub(super) fn pair_bands(a: &[OuterBand], b: &[OuterBand]) -> Vec<(RoadElementKind, f32, f32)> {
    let mut common = vec![vec![0; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            common[i][j] = if a[i].kind == b[j].kind {
                1 + common[i + 1][j + 1]
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut result = Vec::new();
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i].kind == b[j].kind {
            result.push((a[i].kind, a[i].width, b[j].width));
            i += 1;
            j += 1;
        } else if i < a.len() && (j == b.len() || common[i + 1][j] >= common[i][j + 1]) {
            result.push((a[i].kind, a[i].width, 0.));
            i += 1;
        } else {
            result.push((b[j].kind, 0., b[j].width));
            j += 1;
        }
    }
    result
}
