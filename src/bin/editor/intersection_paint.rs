//! Intersection paint for the city editor.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PortSide {
    East,
    South,
    West,
    North,
}

impl PortSide {
    pub(super) const ALL: [Self; 4] = [Self::East, Self::South, Self::West, Self::North];

    pub(super) fn from_world_direction(direction: WorldPoint) -> Option<Self> {
        match (direction.x as i32, direction.y as i32) {
            (1, 0) => Some(Self::East),
            (0, -1) => Some(Self::South),
            (-1, 0) => Some(Self::West),
            (0, 1) => Some(Self::North),
            _ => None,
        }
    }

    pub(super) fn direction(self) -> egui::Vec2 {
        match self {
            Self::East => egui::vec2(1.0, 0.0),
            Self::South => egui::vec2(0.0, 1.0),
            Self::West => egui::vec2(-1.0, 0.0),
            Self::North => egui::vec2(0.0, -1.0),
        }
    }

    pub(super) fn edge_point(self, center: egui::Pos2, half: f32, lateral: f32) -> egui::Pos2 {
        match self {
            Self::East => center + egui::vec2(half, lateral),
            Self::South => center + egui::vec2(lateral, half),
            Self::West => center + egui::vec2(-half, lateral),
            Self::North => center + egui::vec2(lateral, -half),
        }
    }

    pub(super) fn lateral(self) -> egui::Vec2 {
        match self {
            Self::East | Self::West => egui::vec2(0.0, 1.0),
            Self::South | Self::North => egui::vec2(1.0, 0.0),
        }
    }

    pub(super) fn wedge_planes(self) -> [(f32, f32); 2] {
        match self {
            Self::East => [(1.0, -1.0), (1.0, 1.0)],
            Self::South => [(-1.0, 1.0), (1.0, 1.0)],
            Self::West => [(-1.0, -1.0), (-1.0, 1.0)],
            Self::North => [(-1.0, -1.0), (1.0, -1.0)],
        }
    }
}

pub(super) fn clipped_port_band(
    band: egui::Rect,
    side: PortSide,
    center: egui::Pos2,
) -> Vec<egui::Pos2> {
    let mut polygon = vec![
        band.left_top(),
        band.right_top(),
        band.right_bottom(),
        band.left_bottom(),
    ];
    for (x, y) in side.wedge_planes() {
        let signed = |point: egui::Pos2| x * (point.x - center.x) + y * (point.y - center.y);
        let mut clipped = Vec::new();
        if let Some(mut previous) = polygon.last().copied() {
            for current in polygon {
                let before = signed(previous);
                let after = signed(current);
                if (before < 0.0 && after >= 0.0) || (before >= 0.0 && after < 0.0) {
                    let fraction = before / (before - after);
                    clipped.push(previous + (current - previous) * fraction);
                }
                if after >= 0.0 {
                    clipped.push(current);
                }
                previous = current;
            }
        }
        polygon = clipped;
    }
    polygon
}

pub(super) fn paint_port_band(
    painter: &egui::Painter,
    band: egui::Rect,
    side: PortSide,
    center: egui::Pos2,
    color: Color32,
) {
    let polygon = clipped_port_band(band, side, center);
    if polygon.len() >= 3 {
        painter.add(egui::Shape::convex_polygon(
            polygon,
            color,
            egui::Stroke::NONE,
        ));
    }
}

pub(super) struct IntersectionArm<'a> {
    pub(super) segment: &'a map::Segment,
    pub(super) is_end: bool,
    pub(super) direction: egui::Vec2,
}

pub(super) struct IntersectionPort {
    pub(super) side: PortSide,
    pub(super) bands: Vec<(RoadElementKind, egui::Rect)>,
}

pub(super) struct IntersectionPaint {
    pub(super) square: egui::Rect,
    pub(super) closed: RoadElementKind,
    pub(super) ports: Vec<IntersectionPort>,
    pub(super) mixed: bool,
}

pub(super) fn port_sides(arms: &[IntersectionArm<'_>]) -> Vec<PortSide> {
    pub(super) fn assign(
        arms: &[IntersectionArm<'_>],
        current: &mut Vec<PortSide>,
        best: &mut (f32, Vec<PortSide>),
    ) {
        if current.len() == arms.len() {
            let score = current
                .iter()
                .zip(arms)
                .map(|(side, arm)| side.direction().dot(arm.direction))
                .sum();
            if score > best.0 {
                *best = (score, current.clone());
            }
            return;
        }
        for side in PortSide::ALL {
            if !current.contains(&side) {
                current.push(side);
                assign(arms, current, best);
                current.pop();
            }
        }
    }
    let mut best = (f32::NEG_INFINITY, Vec::new());
    assign(arms, &mut Vec::new(), &mut best);
    best.1
}

pub(super) fn intersection_port(
    arm: &IntersectionArm<'_>,
    side: PortSide,
    junction: egui::Pos2,
    half: f32,
    road: &RoadDefinition,
    zoom: f32,
) -> IntersectionPort {
    let normal = screen_normal_at(arm.segment, if arm.is_end { 1.0 } else { 0.0 });
    let sign = if normal.dot(side.lateral()) >= 0.0 {
        1.0
    } else {
        -1.0
    };
    intersection_port_with_sign(side, junction, half, road, zoom, sign)
}

pub(super) fn intersection_port_with_sign(
    side: PortSide,
    junction: egui::Pos2,
    half: f32,
    road: &RoadDefinition,
    zoom: f32,
    sign: f32,
) -> IntersectionPort {
    let depth = half * 0.44;
    let mut offset = -road
        .elements
        .iter()
        .map(|element| map::element_world_width(element.kind))
        .sum::<f32>()
        * zoom
        * 0.5;
    let bands = road
        .elements
        .iter()
        .map(|element| {
            let next = offset + map::element_world_width(element.kind) * zoom;
            let a = side.edge_point(junction, half, offset * sign);
            let b = side.edge_point(junction, half, next * sign);
            let inside = -side.direction() * depth;
            offset = next;
            (element.kind, egui::Rect::from_two_pos(a, b + inside))
        })
        .collect();
    IntersectionPort { side, bands }
}

pub(super) fn intersection_paint(
    program: &Program,
    projection: &map::Projection,
    intersection: &map::Intersection,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
) -> Option<IntersectionPaint> {
    let junction = screen_of(rect, pan, zoom, intersection.position);
    let road = program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Road(road) if road.name == intersection.road_name => Some(road),
            _ => None,
        })?;
    let mut arms = Vec::new();
    for (index, is_end) in map::intersection_arms(projection, &intersection.point_ref) {
        let segment = &projection.segments[index];
        let tangent = segment.tangent_at(if is_end { 1.0 } else { 0.0 });
        let direction = egui::vec2(
            if is_end { -tangent.x } else { tangent.x },
            if is_end { tangent.y } else { -tangent.y },
        );
        arms.push(IntersectionArm {
            segment,
            is_end,
            direction,
        });
    }
    let road_half = program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Road(profile)
                if profile.name == intersection.road_name
                    || intersection
                        .arm_roads
                        .iter()
                        .any(|(_, name)| *name == profile.name) =>
            {
                Some(
                    profile
                        .elements
                        .iter()
                        .map(|element| map::element_world_width(element.kind))
                        .sum::<f32>(),
                )
            }
            _ => None,
        })
        .fold(0.0_f32, f32::max)
        * 0.5;
    let half = map::intersection_footprint_half(intersection, road_half) * zoom;
    let closed = if road
        .elements
        .iter()
        .any(|element| element.kind == RoadElementKind::Sidewalk)
    {
        RoadElementKind::Sidewalk
    } else if road
        .elements
        .iter()
        .any(|element| element.kind == RoadElementKind::Verge)
    {
        RoadElementKind::Verge
    } else {
        RoadElementKind::CarLane
    };
    let ports = if intersection.ports.is_empty() {
        port_sides(&arms)
            .into_iter()
            .zip(&arms)
            .filter_map(|(side, arm)| {
                let name = intersection.road_name_for_segment(arm.segment, arm.is_end);
                let profile = program
                    .statements
                    .iter()
                    .find_map(|statement| match statement {
                        Statement::Road(profile) if profile.name == name => Some(profile),
                        _ => None,
                    })?;
                Some(intersection_port(arm, side, junction, half, profile, zoom))
            })
            .collect()
    } else {
        intersection
            .ports
            .iter()
            .filter_map(|port| {
                let side = PortSide::from_world_direction(port.direction)?;
                Some(intersection_port_with_sign(
                    side, junction, half, road, zoom, 1.0,
                ))
            })
            .collect()
    };
    Some(IntersectionPaint {
        square: egui::Rect::from_center_size(junction, egui::vec2(half * 2.0, half * 2.0)),
        closed,
        ports,
        mixed: !intersection.arm_roads.is_empty(),
    })
}

pub(super) fn paint_intersection(
    painter: &egui::Painter,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    program: &Program,
    projection: &map::Projection,
    intersection: &map::Intersection,
    palette: EditorPalette,
) {
    let Some(paint) = intersection_paint(program, projection, intersection, rect, pan, zoom) else {
        return;
    };
    if let Some(instance) =
        opencity::network::default_four_way_instance(program, projection, intersection)
            .or_else(|| {
                opencity::network::default_three_way_instance(program, projection, intersection)
            })
            .or_else(|| {
                opencity::network::default_right_turn_instance(program, projection, intersection)
            })
    {
        let screen = |point| {
            let p = instance.world_point(point);
            screen_of(rect, pan, zoom, WorldPoint { x: p.x, y: p.y })
        };
        let clipped = painter.with_clip_rect(paint.square.intersect(painter.clip_rect()));
        network_ui::draw_geometry(
            &clipped,
            &instance.geometry,
            &screen,
            true,
            true,
            false,
            false,
            None,
            palette.theme,
        );
        return;
    }
    painter.rect_filled(
        paint.square,
        0.0,
        kind_color(RoadElementKind::CarLane, palette.theme),
    );
    let center = paint.square.center();
    let half = paint.square.width() * 0.5;
    if paint.ports.len() == 3 && !paint.mixed {
        paint_tee_surface(painter, &paint, palette);
        return;
    }
    if paint.ports.len() == 4 && !paint.mixed {
        // Each corner follows the port's outer bands. The carriageway stays
        // clear across the junction instead of forming four pointed wedges.
        if let Some(north) = paint.ports.iter().find(|port| port.side == PortSide::North) {
            for right in [false, true] {
                let corner = if right {
                    paint.square.right_top()
                } else {
                    paint.square.left_top()
                };
                paint_rounded_corner(
                    painter,
                    north,
                    corner,
                    center.x,
                    right,
                    false,
                    palette.theme,
                );
            }
        }
        if let Some(south) = paint.ports.iter().find(|port| port.side == PortSide::South) {
            for right in [false, true] {
                let corner = if right {
                    paint.square.right_bottom()
                } else {
                    paint.square.left_bottom()
                };
                paint_rounded_corner(painter, south, corner, center.x, right, true, palette.theme);
            }
        }
        return;
    }
    for side in PortSide::ALL {
        if !paint.ports.iter().any(|port| port.side == side) {
            let a = side.edge_point(center, half, -half);
            let b = side.edge_point(center, half, half);
            paint_port_band(
                painter,
                egui::Rect::from_two_pos(a, b - side.direction() * half * 0.44),
                side,
                center,
                kind_color(paint.closed, palette.theme),
            );
        }
    }
    for port in &paint.ports {
        for (kind, band) in &port.bands {
            paint_port_band(
                painter,
                *band,
                port.side,
                center,
                kind_color(*kind, palette.theme),
            );
        }
    }
    for side in PortSide::ALL {
        if !paint.ports.iter().any(|port| port.side == side) {
            painter.line_segment(
                [
                    side.edge_point(center, half, -half),
                    side.edge_point(center, half, half),
                ],
                egui::Stroke::new(1.5, palette.map_outline),
            );
        }
    }
}

pub(super) fn tee_closed_bands(
    paint: &IntersectionPaint,
    side: PortSide,
) -> Vec<(RoadElementKind, f32)> {
    let center = paint.square.center();
    let (port_side, positive) = match side {
        PortSide::North => (PortSide::East, false),
        PortSide::South => (PortSide::East, true),
        PortSide::West => (PortSide::North, false),
        PortSide::East => (PortSide::North, true),
    };
    let Some(port) = paint.ports.iter().find(|port| port.side == port_side) else {
        return Vec::new();
    };
    let horizontal = matches!(port_side, PortSide::North | PortSide::South);
    let mut bands: Vec<_> = port
        .bands
        .iter()
        .filter_map(|(kind, band)| {
            let offset = if horizontal {
                band.center().x - center.x
            } else {
                band.center().y - center.y
            };
            ((offset >= 0.0) == positive).then_some((
                offset.abs(),
                *kind,
                if horizontal {
                    band.width()
                } else {
                    band.height()
                },
            ))
        })
        .collect();
    bands.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut depth = 0.0;
    bands
        .into_iter()
        .take_while(|(_, kind, _)| {
            !matches!(
                kind,
                RoadElementKind::CarLane | RoadElementKind::BusLane | RoadElementKind::Middle
            )
        })
        .map(|(_, kind, width)| {
            depth += width;
            (kind, depth)
        })
        .collect()
}

pub(super) fn paint_tee_surface(
    painter: &egui::Painter,
    paint: &IntersectionPaint,
    palette: EditorPalette,
) {
    let center = paint.square.center();
    let half = paint.square.width() * 0.5;
    let Some(closed) = PortSide::ALL
        .into_iter()
        .find(|side| !paint.ports.iter().any(|port| port.side == *side))
    else {
        return;
    };
    // Only corners between two open streets bend. Along the closed side,
    // through-street bands keep their exact widths all the way across.
    for (vertical, horizontal, right, bottom) in [
        (PortSide::North, PortSide::West, false, false),
        (PortSide::North, PortSide::East, true, false),
        (PortSide::South, PortSide::West, false, true),
        (PortSide::South, PortSide::East, true, true),
    ] {
        if horizontal == closed || vertical == closed {
            continue;
        }
        if let Some(port) = paint.ports.iter().find(|port| port.side == vertical) {
            let corner = egui::pos2(
                if right {
                    paint.square.right()
                } else {
                    paint.square.left()
                },
                if bottom {
                    paint.square.bottom()
                } else {
                    paint.square.top()
                },
            );
            paint_rounded_corner(
                painter,
                port,
                corner,
                center.x,
                right,
                bottom,
                palette.theme,
            );
        }
    }
    let a = closed.edge_point(center, half, -half);
    let b = closed.edge_point(center, half, half);
    for (kind, depth) in tee_closed_bands(paint, closed).into_iter().rev() {
        painter.rect_filled(
            egui::Rect::from_two_pos(a, b - closed.direction() * depth),
            0.0,
            kind_color(kind, palette.theme),
        );
    }
}

pub(super) fn paint_rounded_corner(
    painter: &egui::Painter,
    port: &IntersectionPort,
    corner: egui::Pos2,
    middle_x: f32,
    right: bool,
    bottom: bool,
    theme: egui::Theme,
) {
    for (kind, radius) in corner_disks(port, corner.x, middle_x, right)
        .into_iter()
        .rev()
    {
        let mut points = Vec::with_capacity(18);
        points.push(corner);
        for step in 0..=16 {
            let angle = std::f32::consts::FRAC_PI_2 * step as f32 / 16.0;
            points.push(
                corner
                    + egui::vec2(
                        if right { -1.0 } else { 1.0 } * radius * angle.cos(),
                        if bottom { -1.0 } else { 1.0 } * radius * angle.sin(),
                    ),
            );
        }
        painter.add(egui::Shape::convex_polygon(
            points,
            kind_color(kind, theme),
            egui::Stroke::NONE,
        ));
    }
}
