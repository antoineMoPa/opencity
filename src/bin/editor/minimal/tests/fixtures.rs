use super::*;

pub(super) fn editor() -> RoadEditor {
    let mut editor = crate::tests::sample_editor();
    editor.program = parse(
        "road street: carlane; end; road wide: carlane-carlane; end; main=street(-2,0,-1,0,1,0,2,0); other=street(-2,3,2,3);",
    )
    .unwrap();
    editor.saved_program = editor.program.clone();
    editor.selected_road = Some(0);
    editor.selected_segment = Some(map::project(&editor.program).segments[0].source);
    editor.selected_segments = editor.selected_segment.into_iter().collect();
    editor.camera_view = CameraView::Isometric;
    editor.map_zoom = 80.0;
    editor.map_pan = egui::Vec2::ZERO;
    editor.orbit_target = WorldPoint { x: 0.0, y: 0.0 };
    editor.fit_scene_once = false;
    editor
}

pub(super) fn frame(
    editor: &mut RoadEditor,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    frame_sized(editor, ctx, events, egui::vec2(900.0, 650.0))
}

pub(super) fn frame_sized(
    editor: &mut RoadEditor,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    size: egui::Vec2,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| editor.editor_frame_ui(ui),
    );
    output.textures_delta.clear();
    output
}

pub(super) fn pointer(p: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

pub(super) fn click(editor: &mut RoadEditor, ctx: &egui::Context, p: egui::Pos2) {
    let _ = frame(
        editor,
        ctx,
        vec![egui::Event::PointerMoved(p), pointer(p, true)],
    );
    let _ = frame(editor, ctx, vec![pointer(p, false)]);
}

pub(super) fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

pub(super) fn labels(output: &egui::FullOutput) -> Vec<String> {
    fn collect(s: &egui::Shape, out: &mut Vec<String>) {
        match s {
            egui::Shape::Text(t) => out.push(t.galley.job.text.clone()),
            egui::Shape::Vec(v) => {
                for s in v {
                    collect(s, out)
                }
            }
            _ => {}
        }
    }
    let mut out = vec![];
    for shape in &output.shapes {
        collect(&shape.shape, &mut out);
    }
    out
}

pub(super) fn text_position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
    output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.job.text == label => {
                Some(t.pos + t.galley.size() * 0.5)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {label}: {:?}", labels(output)))
}

pub(super) fn palette_shortcut() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::P,
        physical_key: Some(egui::Key::P),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    }
}
