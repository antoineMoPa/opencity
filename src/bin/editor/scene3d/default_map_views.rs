//! Named close-ups for reviewing the complete default map and its population.
use super::*;

pub(super) fn views(
    center: WorldPoint,
    fit: f32,
) -> Vec<(&'static str, CameraView, WorldPoint, f32)> {
    vec![
        ("top-full", CameraView::Top, center, fit),
        ("iso-full", CameraView::Isometric, center, fit),
        (
            "top-ramp-shoulders",
            CameraView::Top,
            WorldPoint { x: 14.95, y: 6.95 },
            1100.0,
        ),
        (
            "iso-ramp-shoulders",
            CameraView::Isometric,
            WorldPoint { x: 14.95, y: 6.95 },
            1100.0,
        ),
        (
            "top-cloverleaf",
            CameraView::Top,
            WorldPoint { x: -13.8, y: 7.2 },
            130.0,
        ),
        (
            "iso-cloverleaf",
            CameraView::Isometric,
            WorldPoint { x: -13.8, y: 7.2 },
            130.0,
        ),
        (
            "top-west-access",
            CameraView::Top,
            WorldPoint { x: -26.1, y: 7.2 },
            85.0,
        ),
        (
            "iso-west-access",
            CameraView::Isometric,
            WorldPoint { x: -26.1, y: 7.2 },
            85.0,
        ),
        (
            "top-east-access",
            CameraView::Top,
            WorldPoint { x: 20.4, y: 7.2 },
            70.0,
        ),
        (
            "iso-east-access",
            CameraView::Isometric,
            WorldPoint { x: 20.4, y: 7.2 },
            70.0,
        ),
        (
            "top-west-town",
            CameraView::Top,
            WorldPoint { x: -24.0, y: 0.6 },
            70.0,
        ),
        (
            "top-east-town",
            CameraView::Top,
            WorldPoint { x: 21.6, y: 0.6 },
            70.0,
        ),
        (
            "vehicle-channel-bridge",
            CameraView::Vehicular,
            WorldPoint { x: 0.0, y: 6.025 },
            1.0,
        ),
        (
            "iso-cloverleaf-reverse",
            CameraView::Isometric,
            WorldPoint { x: -13.8, y: 7.2 },
            130.0,
        ),
        (
            "iso-channel-bridge",
            CameraView::Isometric,
            WorldPoint { x: 3.0, y: 6.15 },
            55.0,
        ),
        (
            "iso-south-bridge",
            CameraView::Isometric,
            WorldPoint { x: -12.0, y: -8.4 },
            80.0,
        ),
        (
            "iso-west-city",
            CameraView::Isometric,
            WorldPoint {
                x: -23.475,
                y: 0.275,
            },
            80.0,
        ),
        (
            "iso-east-city",
            CameraView::Isometric,
            WorldPoint {
                x: 22.43864,
                y: 1.22727,
            },
            85.0,
        ),
        (
            "iso-west-center",
            CameraView::Isometric,
            WorldPoint {
                x: -23.475,
                y: 0.275,
            },
            200.0,
        ),
        (
            "iso-east-center",
            CameraView::Isometric,
            WorldPoint {
                x: 22.43864,
                y: 1.22727,
            },
            200.0,
        ),
        (
            "iso-west-port",
            CameraView::Isometric,
            WorldPoint { x: -33.5, y: 0.6 },
            160.0,
        ),
        (
            "iso-east-port",
            CameraView::Isometric,
            WorldPoint { x: 30.1, y: 0.3 },
            180.0,
        ),
        (
            "iso-tee-detail",
            CameraView::Isometric,
            WorldPoint { x: -23.3, y: 3.6 },
            800.0,
        ),
        (
            "iso-fourway-detail",
            CameraView::Isometric,
            WorldPoint { x: -23.3, y: 2.9 },
            800.0,
        ),
        (
            "iso-turn-detail",
            CameraView::Isometric,
            WorldPoint { x: -24.0, y: -4.8 },
            800.0,
        ),
        (
            "top-turn-detail",
            CameraView::Top,
            WorldPoint { x: -24.0, y: -4.8 },
            800.0,
        ),
    ]
}
