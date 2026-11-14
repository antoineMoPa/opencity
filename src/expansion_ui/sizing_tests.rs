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
    value_rect(shape, " m")
}
fn value_rect(shape: &egui::Shape, suffix: &str) -> Option<egui::Rect> {
    match shape {
        egui::Shape::Text(text) if text.galley.text().ends_with(suffix) => {
            Some(text.galley.rect.translate(text.pos.to_vec2()))
        }
        egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| value_rect(shape, suffix)),
        _ => None,
    }
}

#[test]
fn typing_length_through_zero_preserves_preview_until_committed() {
    for growth in [false, true] {
        for commit_with_enter in [false, true] {
            let mut program =
                crate::parse("road street: carlane carlane; end a=street(0,0,1,0);").unwrap();
            let before = program.clone();
            let mut picker = Picker::default();
            picker.start();
            picker.refresh(&program);
            picker.choose_end(&program, 1);
            picker.selected = picker
                .proposals
                .iter()
                .position(|p| p.kind == expansion::Kind::CurveLeft)
                .unwrap();
            picker.growth = growth;
            let ctx = egui::Context::default();
            let shapes = frame(&mut picker, &ctx, &mut program, vec![]);
            let pos = shapes
                .iter()
                .find_map(|s| value_rect(&s.shape, " u"))
                .unwrap()
                .center();
            let click = |pos| {
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    },
                ]
            };
            frame(&mut picker, &ctx, &mut program, click(pos));
            frame(&mut picker, &ctx, &mut program, vec![]);
            assert!(ctx.memory(|m| m.focused().is_some()));
            for events in [
                tests::key(egui::Key::Backspace),
                vec![egui::Event::Text("0".into())],
                vec![egui::Event::Text(".5".into())],
            ] {
                frame(&mut picker, &ctx, &mut program, events);
                assert_eq!(picker.length, 1.);
                assert_eq!(picker.selected().unwrap().kind, expansion::Kind::CurveLeft);
                assert_eq!(picker.end, Some(1));
                assert!(picker.open);
                assert_eq!(picker.growth, growth);
                assert_eq!(program, before);
            }
            let events = if commit_with_enter {
                tests::key(egui::Key::Enter)
            } else {
                click(egui::pos2(1200., 100.))
            };
            frame(&mut picker, &ctx, &mut program, events);
            frame(&mut picker, &ctx, &mut program, vec![]);
            assert_eq!(picker.length, 0.5);
            assert_eq!(picker.selected().unwrap().kind, expansion::Kind::CurveLeft);
            assert_eq!(program, before, "committing input must not place a road");
        }
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
