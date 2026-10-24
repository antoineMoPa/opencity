//! Markings for the city editor.
use super::*;

pub(super) fn corner_disks(
    port: &IntersectionPort,
    corner_x: f32,
    middle_x: f32,
    right: bool,
) -> Vec<(RoadElementKind, f32)> {
    let mut bands: Vec<_> = port
        .bands
        .iter()
        .filter_map(|(kind, band)| {
            if (band.center().x >= middle_x) != right {
                return None;
            }
            let distance = (corner_x - band.center().x).abs();
            Some((distance, *kind, band.width()))
        })
        .collect();
    bands.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut radius = 0.0;
    bands
        .into_iter()
        .take_while(|(_, kind, _)| {
            !matches!(
                kind,
                RoadElementKind::CarLane | RoadElementKind::BusLane | RoadElementKind::Middle
            )
        })
        .map(|(_, kind, width)| {
            radius += width;
            (kind, radius)
        })
        .collect()
}

pub(super) fn paint_airfield_markings(
    painter: &egui::Painter,
    spine: &[egui::Pos2],
    normals: &[egui::Vec2],
    center: f32,
    width: f32,
    kind: RoadElementKind,
) {
    match kind {
        RoadElementKind::Runway => {
            let white = Color32::from_rgb(235, 236, 224);
            let stroke = egui::Stroke::new((width * 0.022).clamp(0.8, 2.5), white);
            for side in [-1.0, 1.0] {
                let edge: Vec<_> = spine
                    .iter()
                    .zip(normals)
                    .map(|(point, normal)| *point + *normal * (center + side * width * 0.43))
                    .collect();
                painter.add(egui::Shape::line(edge, stroke));
            }
            for step in (3..spine.len().saturating_sub(3)).step_by(4) {
                painter.line_segment(
                    [
                        spine[step] + normals[step] * center,
                        spine[(step + 2).min(spine.len() - 1)]
                            + normals[(step + 2).min(spine.len() - 1)] * center,
                    ],
                    egui::Stroke::new((width * 0.06).clamp(1.0, 5.0), white),
                );
            }
            for step in [2, spine.len().saturating_sub(3)] {
                if let (Some(point), Some(normal)) = (spine.get(step), normals.get(step)) {
                    painter.line_segment(
                        [
                            *point + *normal * (center - width * 0.30),
                            *point + *normal * (center + width * 0.30),
                        ],
                        egui::Stroke::new((width * 0.05).clamp(1.0, 4.0), white),
                    );
                }
            }
        }
        RoadElementKind::Taxiway => {
            let path: Vec<_> = spine
                .iter()
                .zip(normals)
                .map(|(point, normal)| *point + *normal * center)
                .collect();
            painter.add(egui::Shape::line(
                path,
                egui::Stroke::new(
                    (width * 0.1).clamp(1.5, 2.5),
                    Color32::from_rgb(233, 191, 73),
                ),
            ));
        }
        _ => {}
    }
}

pub(super) fn paint_track_markings(
    painter: &egui::Painter,
    spine: &[egui::Pos2],
    normals: &[egui::Vec2],
    center: f32,
    width: f32,
    kind: RoadElementKind,
    theme: egui::Theme,
) {
    let rail_color = match theme {
        egui::Theme::Dark => Color32::from_rgb(220, 221, 214),
        egui::Theme::Light => Color32::from_rgb(49, 53, 57),
    };
    let rail_stroke = egui::Stroke::new((width * 0.11).clamp(0.8, 2.4), rail_color);
    let half_gauge = width * 0.23;
    for side in [-1.0, 1.0] {
        let rail: Vec<_> = spine
            .iter()
            .zip(normals)
            .map(|(point, normal)| *point + *normal * (center + side * half_gauge))
            .collect();
        if kind == RoadElementKind::SubwayTrack {
            for pair in rail.windows(2).enumerate() {
                if pair.0 % 3 != 2 {
                    painter.line_segment([pair.1[0], pair.1[1]], rail_stroke);
                }
            }
        } else {
            painter.add(egui::Shape::line(rail, rail_stroke));
        }
    }
    if kind == RoadElementKind::TrainTrack {
        for (point, normal) in spine.iter().zip(normals).step_by(3) {
            painter.line_segment(
                [
                    *point + *normal * (center - width * 0.34),
                    *point + *normal * (center + width * 0.34),
                ],
                egui::Stroke::new((width * 0.08).clamp(0.7, 2.2), rail_color),
            );
        }
    }
}

pub(super) fn paint_grid(
    painter: &egui::Painter,
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    palette: EditorPalette,
) {
    let left = world_of(rect, pan, zoom, rect.left_top()).x.floor() as i32;
    let right = world_of(rect, pan, zoom, rect.right_top()).x.ceil() as i32;
    let bottom = world_of(rect, pan, zoom, rect.left_bottom()).y.floor() as i32;
    let top = world_of(rect, pan, zoom, rect.left_top()).y.ceil() as i32;
    for x in left..=right {
        let screen = screen_of(
            rect,
            pan,
            zoom,
            WorldPoint {
                x: x as f32,
                y: 0.0,
            },
        )
        .x;
        let color = if x == 0 {
            palette.map_axis
        } else if x % 5 == 0 {
            palette.map_major
        } else {
            palette.map_minor
        };
        painter.line_segment(
            [
                egui::pos2(screen, rect.top()),
                egui::pos2(screen, rect.bottom()),
            ],
            egui::Stroke::new(1.0, color),
        );
    }
    for y in bottom..=top {
        let screen = screen_of(
            rect,
            pan,
            zoom,
            WorldPoint {
                x: 0.0,
                y: y as f32,
            },
        )
        .y;
        let color = if y == 0 {
            palette.map_axis
        } else if y % 5 == 0 {
            palette.map_major
        } else {
            palette.map_minor
        };
        painter.line_segment(
            [
                egui::pos2(rect.left(), screen),
                egui::pos2(rect.right(), screen),
            ],
            egui::Stroke::new(1.0, color),
        );
    }
}
