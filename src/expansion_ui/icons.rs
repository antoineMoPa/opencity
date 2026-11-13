use crate::expansion::Kind;
use egui::{vec2, Color32, Rect, Response, Sense, Stroke, Ui};

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Proposal(Kind),
    Previous,
    Next,
    End,
    Apply,
    Close,
    Message,
}

pub(super) fn order(kind: Kind) -> usize {
    match kind {
        Kind::Continue => 0,
        Kind::CurveLeft => 1,
        Kind::CurveRight => 2,
        Kind::BranchLeft => 3,
        Kind::BranchRight => 4,
        Kind::Rise => 5,
        Kind::Fall => 6,
        Kind::AddElement => 7,
        Kind::RemoveElement => 8,
        Kind::SplitLeft => 9,
        Kind::SplitRight => 10,
        Kind::ThreeWay => 11,
        Kind::FourWay => 12,
        Kind::Stop => 13,
    }
}

pub(super) fn button(ui: &mut Ui, icon: Icon, selected: bool, help: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(28., 28.), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), help));
    let visuals = ui.style().interact(&response);
    let (fill, stroke, ink) = if selected {
        (
            Color32::from_rgb(65, 76, 52),
            Stroke::new(1., Color32::from_rgb(172, 157, 100)),
            Color32::from_rgb(241, 213, 144),
        )
    } else {
        (visuals.bg_fill, Stroke::NONE, visuals.fg_stroke.color)
    };
    if selected || response.hovered() || response.has_focus() {
        ui.painter()
            .rect(rect.shrink(0.5), 3., fill, stroke, egui::StrokeKind::Inside);
    }
    paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), vec2(18., 18.)),
        icon,
        ink,
    );
    response.on_hover_text(help)
}

fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let at = |x: f32, y: f32| rect.min + vec2(x, y) * (rect.width() / 24.);
    let stroke = Stroke::new(1.5, color);
    let line = |points: &[(f32, f32)]| {
        painter.add(egui::Shape::line(
            points.iter().map(|&(x, y)| at(x, y)).collect(),
            stroke,
        ));
    };
    let curve = |points: [(f32, f32); 3]| {
        painter.add(egui::epaint::QuadraticBezierShape::from_points_stroke(
            points.map(|(x, y)| at(x, y)),
            false,
            Color32::TRANSPARENT,
            stroke,
        ));
    };
    match icon {
        Icon::Previous => line(&[(15., 5.), (8., 12.), (15., 19.)]),
        Icon::Next => line(&[(9., 5.), (16., 12.), (9., 19.)]),
        Icon::Apply => line(&[(5., 12.), (9., 16.), (19., 6.)]),
        Icon::Close => {
            line(&[(6., 6.), (18., 18.)]);
            line(&[(6., 18.), (18., 6.)]);
        }
        Icon::End => {
            painter.circle_stroke(at(12., 9.), rect.width() * 0.28, stroke);
            painter.circle_filled(at(12., 9.), 2., color);
            line(&[(7., 14.), (12., 22.), (17., 14.)]);
        }
        Icon::Message => {
            painter.circle_stroke(at(12., 12.), rect.width() * 0.4, stroke);
            line(&[(12., 6.), (12., 13.)]);
            painter.circle_filled(at(12., 17.), 1., color);
        }
        Icon::Proposal(kind) => match kind {
            Kind::ThreeWay | Kind::FourWay => {
                // Road edges leave an open, connected T or cross at the centre.
                line(&[(8., 21.), (8., 16.), (3., 16.)]);
                line(&[(16., 21.), (16., 16.), (21., 16.)]);
                if matches!(kind, Kind::FourWay) {
                    line(&[(3., 8.), (8., 8.), (8., 3.)]);
                    line(&[(21., 8.), (16., 8.), (16., 3.)]);
                } else {
                    line(&[(3., 8.), (21., 8.)]);
                }
            }
            Kind::Continue => {
                line(&[(12., 21.), (12., 3.)]);
                line(&[(7., 8.), (12., 3.), (17., 8.)]);
            }
            Kind::CurveLeft => {
                line(&[(17., 21.), (17., 14.)]);
                curve([(17., 14.), (17., 6.), (9., 6.)]);
                line(&[(9., 6.), (3., 6.)]);
                line(&[(8., 1.), (3., 6.), (8., 11.)]);
            }
            Kind::CurveRight => {
                line(&[(7., 21.), (7., 14.)]);
                curve([(7., 14.), (7., 6.), (15., 6.)]);
                line(&[(15., 6.), (21., 6.)]);
                line(&[(16., 1.), (21., 6.), (16., 11.)]);
            }
            Kind::BranchLeft | Kind::BranchRight => {
                let (x, branch, tip) = match kind {
                    Kind::BranchLeft => (15., 4., 10.),
                    _ => (9., 20., 14.),
                };
                line(&[(x, 21.), (x, 3.)]);
                line(&[(x - 4., 7.), (x, 3.), (x + 4., 7.)]);
                line(&[(x, 16.), (branch, 6.), (tip, 6.)]);
                line(&[(branch, 6.), (branch, 12.)]);
            }
            Kind::Rise => {
                line(&[(3., 18.), (19., 6.), (12., 6.)]);
                line(&[(19., 6.), (19., 13.)]);
                line(&[(3., 21.), (21., 21.)]);
            }
            Kind::Fall => {
                line(&[(3., 6.), (19., 18.), (12., 18.)]);
                line(&[(19., 18.), (19., 11.)]);
                line(&[(3., 21.), (21., 21.)]);
            }
            Kind::AddElement => {
                line(&[(12., 5.), (12., 19.)]);
                line(&[(5., 12.), (19., 12.)]);
            }
            Kind::RemoveElement => line(&[(5., 12.), (19., 12.)]),
            Kind::SplitLeft | Kind::SplitRight => {
                let (x, branch) = match kind {
                    Kind::SplitLeft => (15., 6.),
                    _ => (9., 18.),
                };
                line(&[(x, 21.), (x, 3.)]);
                line(&[(x - 4., 7.), (x, 3.), (x + 4., 7.)]);
                curve([(x, 17.), (branch, 17.), (branch, 7.)]);
                line(&[(branch, 7.), (branch, 3.)]);
                line(&[(branch - 4., 7.), (branch, 3.), (branch + 4., 7.)]);
            }
            Kind::Stop => {
                line(&[(12., 21.), (12., 8.)]);
                line(&[(5., 4.), (19., 4.), (19., 8.), (5., 8.), (5., 4.)]);
            }
        },
    }
}
