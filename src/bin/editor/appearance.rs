//! Appearance for the city editor.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct EditorPalette {
    pub(super) theme: egui::Theme,
    pub(super) warning: Color32,
    pub(super) map_background: Color32,
    pub(super) map_axis: Color32,
    pub(super) map_major: Color32,
    pub(super) map_minor: Color32,
    pub(super) map_text: Color32,
    pub(super) map_subtle: Color32,
    pub(super) map_outline: Color32,
    pub(super) map_selected: Color32,
    pub(super) map_preview: Color32,
    pub(super) map_centerline: Color32,
    pub(super) tile_text: Color32,
}

pub(super) fn editor_palette(theme: egui::Theme) -> EditorPalette {
    match theme {
        egui::Theme::Dark => EditorPalette {
            theme,
            warning: Color32::from_rgb(245, 150, 110),
            map_background: Color32::from_rgb(7, 112, 172),
            map_axis: Color32::from_rgb(28, 48, 55),
            map_major: Color32::from_rgb(18, 36, 43),
            map_minor: Color32::from_rgb(12, 30, 36),
            map_text: Color32::from_rgb(226, 237, 235),
            map_subtle: Color32::from_rgb(153, 177, 175),
            map_outline: Color32::from_rgb(57, 68, 67),
            map_selected: Color32::from_rgb(124, 207, 245),
            map_preview: Color32::from_rgb(127, 216, 199),
            map_centerline: Color32::from_white_alpha(155),
            tile_text: Color32::WHITE,
        },
        egui::Theme::Light => EditorPalette {
            theme,
            warning: Color32::from_rgb(153, 69, 26),
            map_background: Color32::from_rgb(7, 112, 172),
            map_axis: Color32::from_rgb(155, 184, 179),
            map_major: Color32::from_rgb(196, 216, 210),
            map_minor: Color32::from_rgb(222, 234, 228),
            map_text: Color32::from_rgb(36, 60, 59),
            map_subtle: Color32::from_rgb(80, 108, 106),
            map_outline: Color32::from_rgb(72, 86, 88),
            map_selected: Color32::from_rgb(15, 105, 151),
            map_preview: Color32::from_rgb(22, 124, 107),
            map_centerline: Color32::from_black_alpha(90),
            tile_text: Color32::from_rgb(25, 43, 49),
        },
    }
}

pub(super) fn editor_visuals(theme: egui::Theme) -> egui::Visuals {
    match theme {
        egui::Theme::Dark => {
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = Color32::from_rgb(24, 29, 37);
            visuals.window_fill = Color32::from_rgb(24, 29, 37);
            visuals.widgets.active.bg_fill = Color32::from_rgb(57, 87, 111);
            visuals
        }
        egui::Theme::Light => {
            let mut visuals = egui::Visuals::light();
            visuals.panel_fill = Color32::from_rgb(250, 252, 252);
            visuals.window_fill = Color32::from_rgb(250, 252, 252);
            visuals.widgets.active.bg_fill = Color32::from_rgb(190, 216, 224);
            visuals
        }
    }
}

pub(super) fn toggle_theme(ctx: &egui::Context) {
    let next = match ctx.theme() {
        egui::Theme::Light => egui::Theme::Dark,
        egui::Theme::Dark => egui::Theme::Light,
    };
    ctx.set_theme(next);
    ctx.request_repaint();
}

pub(super) fn empty_profile_icon(tint: Color32) -> egui::Image<'static> {
    egui::Image::from_bytes(
        "bytes://lucide/box.svg",
        include_bytes!("../../../assets/lucide/box.svg"),
    )
    .fit_to_exact_size(egui::vec2(16.0, 16.0))
    .tint(tint)
}

pub(super) fn kind_label(kind: RoadElementKind) -> &'static str {
    match kind {
        RoadElementKind::BuildingEdge => "Building edge",
        RoadElementKind::Sidewalk => "Sidewalk",
        RoadElementKind::BikeLane => "Bike lane",
        RoadElementKind::ParkingLane => "Parking lane",
        RoadElementKind::BusLane => "Bus lane",
        RoadElementKind::CarLane => "Car lane",
        RoadElementKind::BusStop => "Bus stop",
        RoadElementKind::Middle => "Median",
        RoadElementKind::Verge => "Verge",
        RoadElementKind::Barrier => "Divider wall",
        RoadElementKind::TrainTrack => "Train track",
        RoadElementKind::TramTrack => "Tram track",
        RoadElementKind::SubwayTrack => "Subway track",
        RoadElementKind::Runway => "Runway",
        RoadElementKind::Taxiway => "Taxiway",
        RoadElementKind::Apron => "Apron",
    }
}

pub(super) fn kind_short(kind: RoadElementKind) -> &'static str {
    match kind {
        RoadElementKind::BuildingEdge => "EDGE",
        RoadElementKind::Sidewalk => "WALK",
        RoadElementKind::BikeLane => "BIKE",
        RoadElementKind::ParkingLane => "PARK",
        RoadElementKind::BusLane => "BUS",
        RoadElementKind::CarLane => "CAR",
        RoadElementKind::BusStop => "STOP",
        RoadElementKind::Middle => "MID",
        RoadElementKind::Verge => "VERGE",
        RoadElementKind::Barrier => "WALL",
        RoadElementKind::TrainTrack => "TRAIN",
        RoadElementKind::TramTrack => "TRAM",
        RoadElementKind::SubwayTrack => "SUBWAY",
        RoadElementKind::Runway => "RUNWAY",
        RoadElementKind::Taxiway => "TAXI",
        RoadElementKind::Apron => "APRON",
    }
}

pub(super) fn kind_color(kind: RoadElementKind, theme: egui::Theme) -> Color32 {
    let [r, g, b] = opencity::network::road_color(kind, theme == egui::Theme::Light);
    Color32::from_rgb(r, g, b)
}
pub(super) fn element_width(kind: RoadElementKind) -> f32 {
    match kind {
        RoadElementKind::BuildingEdge => 72.0,
        RoadElementKind::Sidewalk => 112.0,
        RoadElementKind::BikeLane => 94.0,
        RoadElementKind::ParkingLane => 103.0,
        RoadElementKind::BusLane => 105.0,
        RoadElementKind::CarLane => 108.0,
        RoadElementKind::BusStop => 96.0,
        RoadElementKind::Middle => 78.0,
        RoadElementKind::Verge => 86.0,
        RoadElementKind::Barrier => 30.0,
        RoadElementKind::TrainTrack => 122.0,
        RoadElementKind::TramTrack => 112.0,
        RoadElementKind::SubwayTrack => 118.0,
        RoadElementKind::Runway => 380.0,
        RoadElementKind::Taxiway => 188.0,
        RoadElementKind::Apron => 480.0,
    }
}
