//! Continuous full-profile bands for native two-arm bends.
use super::profile_binding::{pair_bands, ArmProfile, ArmSide, OuterBand};
use super::*;
use crate::{map, Program};

const STEPS: usize = 48;

fn arm_side(direction: map::WorldPoint) -> ArmSide {
    if direction.x.abs() > direction.y.abs() {
        if direction.x > 0. {
            ArmSide::East
        } else {
            ArmSide::West
        }
    } else if direction.y > 0. {
        ArmSide::North
    } else {
        ArmSide::South
    }
}

pub(super) fn connected_sides(
    projection: &map::Projection,
    intersection: &map::Intersection,
) -> Vec<ArmSide> {
    if !intersection.ports.is_empty() {
        return intersection
            .ports
            .iter()
            .map(|port| arm_side(port.direction))
            .collect();
    }
    map::intersection_arms(projection, &intersection.point_ref)
        .into_iter()
        .map(|(index, end)| {
            let mut direction = projection.segments[index].tangent_at(if end { 1. } else { 0. });
            if end {
                direction.x = -direction.x;
                direction.y = -direction.y;
            }
            arm_side(direction)
        })
        .collect()
}

/// Order each complete cross-section from the inside corner outwards.
fn radial_bands(arm: &ArmProfile, sign: f32) -> Vec<OuterBand> {
    let mut bands: Vec<_> = arm
        .bands
        .iter()
        .map(|band| OuterBand {
            kind: band.kind,
            width: band.high - band.low,
        })
        .collect();
    if sign > 0. {
        bands.reverse();
    }
    bands
}

struct Bend {
    half: f32,
    sx: f32,
    sy: f32,
}
impl Bend {
    /// Quarter ellipses have axis-aligned tangents at both approach seams.
    /// Different arm widths vary the two radii without moving either seam.
    fn points(&self, vertical_lateral: f32, horizontal_lateral: f32) -> Vec<Point> {
        let rx = (self.half - self.sx * vertical_lateral).max(0.);
        let ry = (self.half - self.sy * horizontal_lateral).max(0.);
        (0..=STEPS)
            .map(|index| {
                if index == 0 {
                    return Point {
                        x: vertical_lateral,
                        y: self.sy * self.half,
                        z: 0.,
                    };
                }
                if index == STEPS {
                    return Point {
                        x: self.sx * self.half,
                        y: horizontal_lateral,
                        z: 0.,
                    };
                }
                let angle = index as f32 / STEPS as f32 * std::f32::consts::FRAC_PI_2;
                Point {
                    x: self.sx * (self.half - rx * angle.cos()),
                    y: self.sy * (self.half - ry * angle.sin()),
                    z: 0.,
                }
            })
            .collect()
    }
}
fn heading(side: ArmSide, incoming: bool) -> Heading {
    match (side, incoming) {
        (ArmSide::North, false) | (ArmSide::South, true) => Heading::North,
        (ArmSide::East, false) | (ArmSide::West, true) => Heading::East,
        (ArmSide::South, false) | (ArmSide::North, true) => Heading::South,
        (ArmSide::West, false) | (ArmSide::East, true) => Heading::West,
    }
}
fn ports_and_movements(
    geometry: &mut Resolved,
    bend: &Bend,
    vertical: &ArmProfile,
    horizontal: &ArmProfile,
) {
    for arm in [vertical, horizontal] {
        for (incoming, role) in [(true, LaneRole::In), (false, LaneRole::Out)] {
            if let Some((low, high)) = arm.lane_bounds(incoming) {
                geometry.ports.push(Port {
                    name: arm.side.name().into(),
                    role,
                    position: arm.side.position((low + high) / 2., bend.half),
                    heading: heading(arm.side, incoming),
                    width: Some(high - low),
                });
            }
        }
    }
    for reverse in [false, true] {
        let Some((vlow, vhigh)) = vertical.lane_bounds(!reverse) else {
            continue;
        };
        let Some((hlow, hhigh)) = horizontal.lane_bounds(reverse) else {
            continue;
        };
        let (from, to) = if reverse {
            (horizontal, vertical)
        } else {
            (vertical, horizontal)
        };
        let name = format!("{}_{}", from.side.name(), to.side.name());
        let mut points = bend.points((vlow + vhigh) / 2., (hlow + hhigh) / 2.);
        if reverse {
            points.reverse();
        }
        geometry.paths.push(ResolvedPath {
            name: name.clone(),
            points: points.clone(),
        });
        geometry.movements.push(ResolvedMovement {
            movement: Movement {
                class: "vehicle".into(),
                from: format!("{}.in", from.side.name()),
                to: format!("{}.out", to.side.name()),
                via: vec![Traversal {
                    path: name,
                    start: Bound::Start,
                    end: Bound::End,
                }],
            },
            points,
        });
    }
}

/// Bind a native two-way bend to both roads, preserving every profile band.
/// The geometry uses local +Y south, like `default_four_way_instance`.
pub fn default_right_turn_instance(
    program: &Program,
    projection: &map::Projection,
    intersection: &map::Intersection,
) -> Option<NetworkInstance> {
    if intersection.kind != map::IntersectionKind::RightTurn {
        return None;
    }
    let sides = connected_sides(projection, intersection);
    if sides.len() != 2 {
        return None;
    }
    let vertical_side = sides
        .iter()
        .copied()
        .find(|side| matches!(side, ArmSide::North | ArmSide::South))?;
    let horizontal_side = sides
        .iter()
        .copied()
        .find(|side| matches!(side, ArmSide::East | ArmSide::West))?;
    let vertical = ArmProfile::bind(program, projection, intersection, vertical_side)?;
    let horizontal = ArmProfile::bind(program, projection, intersection, horizontal_side)?;
    let half = map::intersection_footprint_half(intersection, vertical.half.max(horizontal.half));
    if half <= 0. {
        return None;
    }
    let bend = Bend {
        half,
        sx: if horizontal_side == ArmSide::East {
            1.
        } else {
            -1.
        },
        sy: if vertical_side == ArmSide::South {
            1.
        } else {
            -1.
        },
    };
    let mut geometry = Resolved {
        band_meshes: Vec::new(),
        ports: Vec::new(),
        paths: Vec::new(),
        anchors: Vec::new(),
        movements: Vec::new(),
        bands: Vec::new(),
        warnings: Vec::new(),
    };
    let mut v_offset = vertical.half;
    let mut h_offset = horizontal.half;
    for (index, (kind, v_width, h_width)) in pair_bands(
        &radial_bands(&vertical, bend.sx),
        &radial_bands(&horizontal, bend.sy),
    )
    .into_iter()
    .enumerate()
    {
        let inner = bend.points(bend.sx * v_offset, bend.sy * h_offset);
        v_offset -= v_width;
        h_offset -= h_width;
        let outer = bend.points(bend.sx * v_offset, bend.sy * h_offset);
        let name = format!("turn_{index}_{}", kind.keyword());
        geometry.band_meshes.push(BandMesh {
            name: name.clone(),
            quads: inner
                .windows(2)
                .zip(outer.windows(2))
                .map(|(a, b)| [a[0], a[1], b[1], b[0]])
                .collect(),
        });
        geometry.paths.push(ResolvedPath {
            name: name.clone(),
            points: inner
                .iter()
                .zip(&outer)
                .map(|(a, b)| a.lerp(*b, 0.5))
                .collect(),
        });
        geometry.bands.push(Band {
            path: name.clone(),
            name,
            width: v_width.max(h_width),
            side: Side::Center,
            kind: BandKind::Surface,
            material: Material::RoadElement(kind),
        });
    }
    ports_and_movements(&mut geometry, &bend, &vertical, &horizontal);
    Some(NetworkInstance {
        rotation: 0.,
        id: format!("profile:{}", intersection.point_ref),
        name: intersection.point_ref.clone(),
        definition: usize::MAX,
        origin: Point {
            x: intersection.position.x,
            y: intersection.position.y,
            z: 0.,
        },
        geometry,
    })
}
