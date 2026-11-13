use super::*;

fn frame(
    picker: &mut Picker,
    ctx: &egui::Context,
    program: &mut Program,
    events: Vec<egui::Event>,
) -> Vec<egui::epaint::ClippedShape> {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 800.),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            picker.show(ui, program, true);
        },
    );
    output.textures_delta.clear();
    output.shapes
}
fn metres(shape: &egui::Shape) -> Option<egui::Rect> {
    match shape {
        egui::Shape::Text(text) if text.galley.text().ends_with(" m") => {
            Some(text.galley.rect.translate(text.pos.to_vec2()))
        }
        egui::Shape::Vec(shapes) => shapes.iter().find_map(metres),
        _ => None,
    }
}
#[test]
fn reducing_junction_length_to_zero_keeps_the_icon_end_and_editor_active() {
    for kind in [expansion::Kind::ThreeWay, expansion::Kind::FourWay] {
        let mut program = crate::parse(include_str!(
            "../../tests/fixtures/expansion_junctions.road"
        ))
        .unwrap();
        let before = program.clone();
        let mut picker = Picker::default();
        assert_eq!(picker.junction_extension, 0.01, "default is one metre");
        picker.start();
        picker.refresh(&program);
        picker.choose_end(&program, 1);
        picker.selected = picker
            .proposals
            .iter()
            .position(|p| p.kind == kind)
            .unwrap();
        let ctx = egui::Context::default();
        let shapes = frame(&mut picker, &ctx, &mut program, vec![]);
        let pos = shapes
            .iter()
            .find_map(|s| metres(&s.shape))
            .unwrap()
            .center();
        frame(
            &mut picker,
            &ctx,
            &mut program,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        let end = pos - egui::vec2(50., 0.);
        frame(
            &mut picker,
            &ctx,
            &mut program,
            vec![egui::Event::PointerMoved(end)],
        );
        frame(
            &mut picker,
            &ctx,
            &mut program,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        assert_eq!(picker.junction_extension, 0.);
        assert_eq!(picker.selected().unwrap().kind, kind);
        assert_eq!(picker.end, Some(1));
        assert!(picker.open);
        assert_eq!(picker.length, 1., "ordinary street length is independent");
        assert_eq!(program, before, "dragging changes the preview only");
    }
}
