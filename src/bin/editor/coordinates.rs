//! Coordinates for the city editor.
use super::*;

pub(super) fn screen_of(
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    point: WorldPoint,
) -> egui::Pos2 {
    rect.center() + pan + egui::vec2(point.x * zoom, -point.y * zoom)
}

pub(super) fn world_of(
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    point: egui::Pos2,
) -> WorldPoint {
    let relative = point - rect.center() - pan;
    WorldPoint {
        x: relative.x / zoom,
        y: -relative.y / zoom,
    }
}

pub(super) const ISO_ELEVATION: f32 = 0.61548;
pub(super) const MIN_ISO_ELEVATION: f32 = 15.0_f32.to_radians();
pub(super) const MAX_ISO_ELEVATION: f32 = 75.0_f32.to_radians();
pub(super) const ISO_ELEVATION_STEP: f32 = 5.0_f32.to_radians();

pub(super) fn iso_screen(
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    target: WorldPoint,
    angle: f32,
    elevation: f32,
    point: WorldPoint,
    height: f32,
) -> egui::Pos2 {
    let dx = point.x - target.x;
    let dy = point.y - target.y;
    rect.center()
        + pan
        + egui::vec2(
            (dx * angle.cos() + dy * angle.sin()) * zoom,
            ((dx * angle.sin() - dy * angle.cos()) * elevation.sin() - height * elevation.cos())
                * zoom,
        )
}

pub(super) fn iso_ground_point(
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    target: WorldPoint,
    angle: f32,
    elevation: f32,
    screen: egui::Pos2,
) -> WorldPoint {
    iso_world_at_height(rect, pan, zoom, target, angle, elevation, screen, 0.0)
}

pub(super) fn iso_zoom_at_pointer(
    rect: egui::Rect,
    pan: &mut egui::Vec2,
    zoom: &mut f32,
    target: WorldPoint,
    angle: f32,
    elevation: f32,
    pointer: egui::Pos2,
    scroll: f32,
) {
    let anchored = iso_ground_point(rect, *pan, *zoom, target, angle, elevation, pointer);
    *zoom = (*zoom * (1.0 + scroll * 0.002)).clamp(MIN_MAP_ZOOM, MAX_MAP_ZOOM);
    *pan += pointer - iso_screen(rect, *pan, *zoom, target, angle, elevation, anchored, 0.0);
}

pub(super) fn iso_world_at_height(
    rect: egui::Rect,
    pan: egui::Vec2,
    zoom: f32,
    target: WorldPoint,
    angle: f32,
    elevation: f32,
    screen: egui::Pos2,
    height: f32,
) -> WorldPoint {
    let delta = (screen - rect.center() - pan) / zoom;
    let vertical = (delta.y + height * elevation.cos()) / elevation.sin();
    WorldPoint {
        x: target.x + delta.x * angle.cos() + vertical * angle.sin(),
        y: target.y + delta.x * angle.sin() - vertical * angle.cos(),
    }
}
