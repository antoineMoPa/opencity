//! Shared `.road` props with native editor selection, history and floating controls.
use super::*;
use glam::{Mat4, Vec2, Vec3, Vec4};
use opencity::{
    props::{Entry, Kind, Placement, Row, Shape},
    runtime::{City, SurfaceKind},
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Single,
    Row,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Member {
    entry: usize,
    index: usize,
}
#[derive(Clone)]
struct Sample {
    member: Member,
    placement: Placement,
}
enum Gesture {
    Move {
        before: Vec<Entry>,
        anchor: Vec2,
        members: Vec<Member>,
    },
    Handle {
        before: Vec<Entry>,
        entry: usize,
        index: usize,
    },
    MoveRow {
        before: Vec<Entry>,
        entry: usize,
        anchor: Vec2,
    },
    Draw {
        press: egui::Pos2,
    },
    Box {
        start: egui::Pos2,
        current: egui::Pos2,
        initial: Vec<Member>,
    },
}
pub(super) struct Editor {
    pub(super) active: bool,
    tool: Tool,
    pub(super) kind: Kind,
    spacing: f32,
    cached: Option<Program>,
    entries: Vec<Entry>,
    selected: Vec<Member>,
    row: Option<usize>,
    draft: Option<Entry>,
    gesture: Option<Gesture>,
    error: String,
    last: Option<egui::Pos2>,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            active: false,
            tool: Tool::Select,
            kind: Kind::Cone,
            spacing: 2.5,
            cached: None,
            entries: Vec::new(),
            selected: Vec::new(),
            row: None,
            draft: None,
            gesture: None,
            error: String::new(),
            last: None,
        }
    }
}
impl Editor {
    pub(super) fn cancel(&mut self) {
        if let Some(
            Gesture::Move { before, .. }
            | Gesture::Handle { before, .. }
            | Gesture::MoveRow { before, .. },
        ) = self.gesture.take()
        {
            self.entries = before;
        }
        self.draft = None;
    }
    pub(super) fn close(&mut self) {
        self.cancel();
        self.active = false;
    }
    pub(super) fn dragging(&self) -> bool {
        self.gesture.is_some()
    }
    fn sync(&mut self, program: &Program) {
        if self.cached.as_ref() == Some(program) {
            return;
        }
        self.cancel();
        match opencity::props::read(program) {
            Ok(entries) => {
                self.entries = entries;
                self.error.clear();
            }
            Err(e) => {
                self.entries.clear();
                self.error = e;
            }
        }
        self.selected.clear();
        self.row = None;
        self.cached = Some(program.clone());
    }
    fn start(&mut self, program: &Program, kind: Kind) {
        self.sync(program);
        self.active = true;
        self.kind = kind;
        self.spacing = kind.spacing();
        self.tool = Tool::Single;
    }
    fn name(&self, program: &Program) -> String {
        (1..)
            .map(|i| format!("placed_prop_{i}"))
            .find(|n| {
                !self.entries.iter().any(|e| e.name == *n)
                    && !program
                        .statements
                        .iter()
                        .any(|s| matches!(s,Statement::Call(c)if c.assigns.contains(n)))
            })
            .unwrap()
    }
    fn samples(&self, city: &City) -> Vec<Sample> {
        self.entries
            .iter()
            .enumerate()
            .flat_map(|(entry, e)| {
                e.placements(city)
                    .unwrap_or_default()
                    .into_iter()
                    .map(move |(index, placement)| Sample {
                        member: Member { entry, index },
                        placement,
                    })
            })
            .collect()
    }
    fn active_members(&self, city: &City) -> Vec<Member> {
        if let Some(entry) = self.row {
            self.samples(city)
                .into_iter()
                .filter(|s| s.member.entry == entry)
                .map(|s| s.member)
                .collect()
        } else {
            self.selected.clone()
        }
    }
    fn commit(&mut self, program: &mut Program, city: &City, before: Vec<Entry>) {
        let result = (|| {
            for entry in &self.entries {
                entry.placements(city)?;
            }
            opencity::props::apply(program, &self.entries)
        })();
        if let Err(error) = result {
            self.entries = before;
            self.selected.clear();
            self.row = None;
            self.error = error;
        } else {
            self.entries = opencity::props::read(program).unwrap_or_default();
            self.cached = Some(program.clone());
            self.error.clear();
        }
    }
    fn move_members(&mut self, city: &City, members: &[Member], delta: Vec2) {
        for m in members {
            match &mut self.entries[m.entry].shape {
                Shape::Single(p) => {
                    p.position += delta.extend(0.);
                    if let Some(s) =
                        city.surface_at(p.position.truncate().to_array(), p.position.z + 0.08)
                    {
                        p.position.z = s.height;
                    }
                }
                Shape::Row { row, .. } => {
                    *row.offsets.entry(m.index).or_default() += delta.extend(0.)
                }
            }
        }
    }
    fn delete(&mut self, program: &mut Program, city: &City) {
        let before = self.entries.clone();
        if let Some(index) = self.row {
            self.entries.remove(index);
        } else {
            let mut removed = BTreeSet::new();
            for m in &self.selected {
                match &mut self.entries[m.entry].shape {
                    Shape::Single(_) => {
                        removed.insert(m.entry);
                    }
                    Shape::Row { row, .. } => {
                        row.hidden.insert(m.index);
                    }
                }
            }
            self.entries = std::mem::take(&mut self.entries)
                .into_iter()
                .enumerate()
                .filter_map(|(i, e)| (!removed.contains(&i)).then_some(e))
                .collect();
        }
        self.selected.clear();
        self.row = None;
        self.commit(program, city, before);
    }
    fn duplicate(&mut self, program: &mut Program, city: &City) {
        let before = self.entries.clone();
        if let Some(index) = self.row {
            let mut e = self.entries[index].clone();
            e.name = self.name(program);
            e.translate(Vec2::splat(0.01));
            self.entries.push(e);
            self.row = Some(self.entries.len() - 1);
        } else {
            let points: Vec<_> = self
                .samples(city)
                .into_iter()
                .filter(|s| self.selected.contains(&s.member))
                .collect();
            self.selected.clear();
            for s in points {
                let mut p = s.placement;
                p.position += Vec3::new(0.01, 0.01, 0.);
                let member = Member {
                    entry: self.entries.len(),
                    index: 0,
                };
                self.entries.push(Entry {
                    name: self.name(program),
                    shape: Shape::Single(p),
                });
                self.selected.push(member);
            }
        }
        self.commit(program, city, before);
    }
}
struct CityCache {
    program: Program,
    base: Option<PathBuf>,
    map: Option<Arc<opencity::heightmap::ElevationMap>>,
    city: Arc<City>,
}
pub(super) fn city(program: &Program, base: Option<&Path>) -> Result<Arc<City>, String> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<CityCache>>> =
        std::sync::OnceLock::new();
    let program = Program {
        statements: program
            .statements
            .iter()
            .filter(
                |s| !matches!(s,Statement::Call(c)if matches!(c.name.as_str(),"prop"|"prop_row")),
            )
            .cloned()
            .collect(),
    };
    let map = super::elevation_map::load(&program, base)?;
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "Prop surface cache unavailable")?;
    if let Some(c) = cache.as_ref() {
        let same_map = match (&c.map, &map) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if c.program == program && c.base.as_deref() == base && same_map {
            return Ok(c.city.clone());
        }
    }
    let city = Arc::new(City::from_program_with_elevation_loader(&program, |_| {
        map.as_deref()
            .cloned()
            .ok_or("Missing prop elevation map".into())
    })?);
    *cache = Some(CityCache {
        program,
        base: base.map(Path::to_path_buf),
        map,
        city: city.clone(),
    });
    Ok(city)
}
#[derive(Clone, Copy)]
struct Projection {
    matrix: Mat4,
    rect: egui::Rect,
    vehicle_eye: Option<Vec3>,
}
// Road bands have an editor-only lift to separate them from terrain. Keep
// prop rendering and picking on that same visible surface; saves retain the
// physical elevation shared with the game.
fn surface_render_offset(kind: SurfaceKind) -> f32 {
    match kind {
        SurfaceKind::Asphalt | SurfaceKind::Sidewalk | SurfaceKind::Marking => {
            scene3d::PAVEMENT_OFFSET
        }
        _ => 0.,
    }
}
fn display_position(city: &City, position: Vec3) -> Vec3 {
    let lift = city
        .surface_at(position.truncate().to_array(), position.z + 0.0001)
        .map_or(0., |s| surface_render_offset(s.kind));
    position + Vec3::Z * lift
}
impl Projection {
    fn new(editor: &RoadEditor, rect: egui::Rect) -> Self {
        Self {
            matrix: Mat4::from_cols_array_2d(&editor.camera_clip_matrix(rect)).transpose(),
            rect,
            vehicle_eye: (editor.camera_view == CameraView::Vehicular).then_some(Vec3::new(
                editor.vehicle_position.x,
                editor.vehicle_position.y,
                editor.vehicle_eye_height,
            )),
        }
    }
    fn screen(self, p: Vec3) -> Option<egui::Pos2> {
        let q = self.matrix * p.extend(1.);
        if q.w <= 0. {
            return None;
        }
        let p = q.truncate() / q.w;
        (p.z >= 0. && p.z <= 1.).then(|| {
            egui::pos2(
                self.rect.center().x + p.x * self.rect.width() / 2.,
                self.rect.center().y - p.y * self.rect.height() / 2.,
            )
        })
    }
    fn ray(self, pos: egui::Pos2) -> (Vec3, Vec3) {
        let x = (pos.x - self.rect.left()) / self.rect.width() * 2. - 1.;
        let y = 1. - (pos.y - self.rect.top()) / self.rect.height() * 2.;
        let inverse = self.matrix.inverse();
        // Reversed-Z puts the perspective far plane at infinity (w=0).
        // Start at the near plane and use a finite distant endpoint instead.
        let a = inverse * Vec4::new(x, y, 1., 1.);
        let b = inverse * Vec4::new(x, y, 0.0001, 1.);
        let a = a.truncate() / a.w;
        (a, b.truncate() / b.w - a)
    }
    fn ground(self, pos: egui::Pos2, city: &City) -> Option<Vec3> {
        let (a, d) = self.ray(pos);
        if !a.is_finite() || !d.is_finite() || d.z.abs() < 1e-10 {
            return None;
        }
        let support = self.vehicle_eye.and_then(|eye| {
            city.surface_at(eye.truncate().to_array(), eye.z)
                .map(|s| (eye, s))
        });
        let mut height = support.map_or(0., |(_, s)| s.height + surface_render_offset(s.kind));
        let mut physical_height = 0.;
        let mut p = Vec3::ZERO;
        let mut t = if let Some((eye, surface)) = support {
            let gradient = Vec2::from_array(surface.gradient);
            (height - a.z + gradient.dot(a.truncate() - eye.truncate()))
                / (d.z - gradient.dot(d.truncate()))
        } else {
            (height - a.z) / d.z
        };
        for _ in 0..8 {
            if !(0. ..=1.).contains(&t) {
                return None;
            }
            p = a + d * t;
            let ceiling = if self.vehicle_eye.is_some() {
                p.z + 0.03
            } else {
                f32::INFINITY
            };
            let surface = city.surface_at(p.truncate().to_array(), ceiling)?;
            if surface.kind == SurfaceKind::Water {
                return None;
            }
            physical_height = surface.height;
            height = physical_height + surface_render_offset(surface.kind);
            if (height - p.z).abs() < 0.00001 {
                return Some(p.truncate().extend(physical_height));
            }
            // Intersect the local sloped surface, rather than repeatedly
            // projecting onto horizontal planes near the vehicle horizon.
            let gradient = Vec2::from_array(surface.gradient);
            let slope = d.z - gradient.dot(d.truncate());
            if slope.abs() < 1e-10 {
                return None;
            }
            t += (height - p.z) / slope;
        }
        Some(p.truncate().extend(physical_height))
    }
}
fn hit_triangle(a: Vec3, d: Vec3, p: [Vec3; 3]) -> Option<f32> {
    let e1 = p[1] - p[0];
    let e2 = p[2] - p[0];
    let h = d.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-12 {
        return None;
    }
    let t = a - p[0];
    let u = t.dot(h) / det;
    let q = t.cross(e1);
    let v = d.dot(q) / det;
    let along = e2.dot(q) / det;
    (u >= 0. && v >= 0. && u + v <= 1. && (0. ..=1.).contains(&along)).then_some(along)
}
fn pick(samples: &[Sample], view: Projection, pos: egui::Pos2, city: &City) -> Option<Member> {
    let (a, d) = view.ray(pos);
    samples
        .iter()
        .filter_map(|s| {
            let mut placement = s.placement.clone();
            placement.position = display_position(city, placement.position);
            let matrix = placement.transform();
            s.placement
                .kind
                .parts()
                .iter()
                .flat_map(|mesh| mesh.vertices.chunks_exact(3))
                .filter_map(|tri| {
                    hit_triangle(
                        a,
                        d,
                        std::array::from_fn(|i| {
                            matrix.transform_point3(Vec3::from_array(tri[i].position))
                        }),
                    )
                })
                .min_by(f32::total_cmp)
                .map(|t| (s.member, t))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(m, _)| m)
        .or_else(|| {
            samples
                .iter()
                .filter_map(|s| {
                    view.screen(display_position(city, s.placement.position))
                        .map(|p| (s.member, p.distance(pos)))
                })
                .filter(|(_, d)| *d < 9.)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(m, _)| m)
        })
}
impl RoadEditor {
    pub(super) fn start_props(&mut self, kind: Kind) {
        self.cancel_placement();
        self.selected_segment = None;
        self.selected_segments.clear();
        self.selected_landscape = None;
        self.minimal.rename_source = None;
        self.minimal.props.start(&self.program, kind);
        self.view = View::Map;
    }
    pub(super) fn prop_toolbar(&mut self, root: &mut egui::Ui) {
        let mut state = std::mem::take(&mut self.minimal.props);
        state.sync(&self.program);
        if !state.active {
            self.minimal.props = state;
            return;
        }
        let city = city(&self.program, self.path.as_deref().and_then(Path::parent));
        let ctx = root.ctx().clone();
        let mut history = None;
        let main = egui::Area::new(egui::Id::new("prop-tools"))
            .order(egui::Order::Foreground)
            .fixed_pos(
                root.max_rect().min
                    + egui::vec2(
                        12.,
                        if root.max_rect().width() < 740. {
                            54.
                        } else {
                            12.
                        },
                    ),
            )
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.add_enabled_ui(!self.minimal.input_suspended, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.set_max_width((root.max_rect().width() - 270.).max(260.));
                            egui::ComboBox::from_id_salt("prop-kind")
                                .width(72.)
                                .selected_text(state.kind.label())
                                .show_ui(ui, |ui| {
                                    for kind in Kind::ALL {
                                        if ui
                                            .selectable_label(state.kind == kind, kind.label())
                                            .clicked()
                                        {
                                            state.kind = kind;
                                            state.spacing = kind.spacing();
                                            state.cancel();
                                        }
                                    }
                                });
                            for (tool, label) in [
                                (Tool::Select, "Select"),
                                (Tool::Single, "Single"),
                                (Tool::Row, "Row"),
                            ] {
                                if ui.selectable_label(state.tool == tool, label).clicked() {
                                    state.cancel();
                                    state.tool = tool;
                                    state.selected.clear();
                                    state.row = None;
                                }
                            }
                            if state.tool == Tool::Row {
                                ui.label("Spacing");
                                ui.add(
                                    egui::DragValue::new(&mut state.spacing)
                                        .speed(0.1)
                                        .range(0.6..=50.)
                                        .suffix(" m"),
                                );
                            }
                            if ui.button("Undo").clicked() {
                                history = Some(true);
                            }
                            if ui.button("Redo").clicked() {
                                history = Some(false);
                            }
                            if ui.button("Done").clicked() {
                                state.close();
                            }
                        });
                    });
                    if let Err(e) = &city {
                        ui.colored_label(ui.visuals().error_fg_color, e);
                    } else if !state.error.is_empty() {
                        ui.colored_label(ui.visuals().error_fg_color, &state.error);
                    }
                });
            })
            .response
            .rect;
        let mut bounds = main;
        if let Ok(city) = &city {
            let members = state.active_members(city);
            if !members.is_empty() {
                let panel = egui::Area::new(egui::Id::new("prop-selection"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(egui::pos2(main.left(), main.bottom() + 7.))
                    .show(&ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.add_enabled_ui(!self.minimal.input_suspended, |ui| {
                                ui.set_max_width((root.max_rect().width() - 24.).min(520.));
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(format!("{} props", members.len()));
                                    if let Some(index) = state.row {
                                        let before = state.entries.clone();
                                        if let Shape::Row { row, .. } =
                                            &mut state.entries[index].shape
                                        {
                                            ui.label("Spacing");
                                            if ui
                                                .add(
                                                    egui::DragValue::new(&mut row.spacing)
                                                        .speed(0.1)
                                                        .range(0.6..=50.)
                                                        .suffix(" m"),
                                                )
                                                .changed()
                                            {
                                                state.commit(&mut self.program, city, before);
                                            }
                                        }
                                    } else if state.selected.len() == 1 {
                                        let m = state.selected[0];
                                        let p = state
                                            .samples(city)
                                            .into_iter()
                                            .find(|s| s.member == m)
                                            .map(|s| s.placement);
                                        if let Some(p) = p {
                                            let mut angle = p.heading;
                                            if ui
                                                .add(
                                                    egui::DragValue::new(&mut angle)
                                                        .speed(1.)
                                                        .suffix("°"),
                                                )
                                                .changed()
                                                && angle.is_finite()
                                            {
                                                let before = state.entries.clone();
                                                match &mut state.entries[m.entry].shape {
                                                    Shape::Single(p) => p.heading = angle,
                                                    Shape::Row { row, .. } => {
                                                        row.offsets.entry(m.index).or_default().z +=
                                                            angle - p.heading
                                                    }
                                                }
                                                state.commit(&mut self.program, city, before);
                                            }
                                        }
                                        if matches!(state.entries[m.entry].shape, Shape::Row { .. })
                                            && ui.button("Select row").clicked()
                                        {
                                            state.row = Some(m.entry);
                                            state.selected.clear();
                                        }
                                    }
                                    if ui.button("Duplicate").clicked() {
                                        state.duplicate(&mut self.program, city);
                                    }
                                    if ui.button("Delete").clicked() {
                                        state.delete(&mut self.program, city);
                                    }
                                });
                            });
                        });
                    })
                    .response
                    .rect;
                bounds = bounds.union(panel);
            }
        }
        self.minimal.toolbar_rect = Some(bounds);
        self.minimal.props = state;
        if let Some(undo) = history {
            self.restore_history(undo);
        }
    }
    pub(super) fn prop_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
    ) -> bool {
        let mut state = std::mem::take(&mut self.minimal.props);
        state.sync(&self.program);
        if self.minimal.input_suspended || self.camera_transition.is_some() {
            state.cancel();
            let active = state.active;
            self.minimal.props = state;
            return active;
        }
        let samples_present = !state.entries.is_empty();
        if !state.active && (!samples_present || self.map_tool != MapTool::Select) {
            self.minimal.props = state;
            return false;
        }
        let city = match city(&self.program, self.path.as_deref().and_then(Path::parent)) {
            Ok(c) => c,
            Err(e) => {
                state.error = e;
                let active = state.active;
                self.minimal.props = state;
                return active;
            }
        };
        let view = Projection::new(self, rect);
        let result = state.interact(ui, response, view, &city, &mut self.program);
        self.minimal.props = state;
        result
    }
    pub(super) fn paint_props(&self, ui: &egui::Ui, rect: egui::Rect) {
        let state = &self.minimal.props;
        if !state.active {
            return;
        }
        let Ok(city) = city(&self.program, self.path.as_deref().and_then(Path::parent)) else {
            return;
        };
        let palette = editor_palette(ui.ctx().theme());
        state.paint(
            ui,
            Projection::new(self, rect),
            &city,
            palette.map_selected,
            palette.map_preview,
        );
    }
}
impl Editor {
    fn interact(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        view: Projection,
        city: &City,
        program: &mut Program,
    ) -> bool {
        if self.active && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.gesture.is_some() || self.draft.is_some() {
                self.cancel();
            } else {
                self.close();
            }
            return true;
        }
        if self.active
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
        {
            self.delete(program, city);
            return true;
        }
        let Some(pos) = ui.input(|i| i.pointer.interact_pos()) else {
            return self.active;
        };
        let moved = self.last != Some(pos);
        self.last = Some(pos);
        let in_view = view.rect.contains(pos)
            && ui
                .ctx()
                .layer_id_at(pos)
                .is_none_or(|l| l.order == egui::Order::Background);
        let point = view.ground(pos, city);
        let pressed = ui.input(|i| i.pointer.primary_pressed());
        let released = ui.input(|i| i.pointer.primary_released());
        let samples = self.samples(city);
        if pressed && in_view {
            let member = pick(&samples, view, pos, city);
            if !self.active {
                if member.is_none() {
                    return false;
                }
                self.active = true;
                self.tool = Tool::Select;
            }
            ui.ctx().memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id)
                }
            });
            self.error.clear();
            let handle = self.row.and_then(|index| {
                let Shape::Row { row, .. } = &self.entries[index].shape else {
                    return None;
                };
                [row.a, row.b, row.control]
                    .iter()
                    .enumerate()
                    .find_map(|(i, p)| {
                        view.screen(display_position(city, p.extend(row.elevation + 0.002)))
                            .filter(|p| p.distance(pos) < 10.)
                            .map(|_| (index, i))
                    })
            });
            if let Some((entry, index)) = handle {
                self.gesture = Some(Gesture::Handle {
                    before: self.entries.clone(),
                    entry,
                    index,
                });
            } else if let Some(member) = member {
                if ui.input(|i| {
                    i.pointer
                        .button_double_clicked(egui::PointerButton::Primary)
                }) && matches!(self.entries[member.entry].shape, Shape::Row { .. })
                {
                    self.row = Some(member.entry);
                    self.selected.clear();
                    self.tool = Tool::Select;
                    return true;
                }
                self.row = None;
                if ui.input(|i| i.modifiers.shift) {
                    if self.selected.contains(&member) {
                        self.selected.retain(|m| *m != member);
                    } else {
                        self.selected.push(member);
                    }
                } else if !self.selected.contains(&member) {
                    self.selected = vec![member];
                }
                if let Some(p) = point {
                    self.gesture = Some(Gesture::Move {
                        before: self.entries.clone(),
                        anchor: p.truncate(),
                        members: self.selected.clone(),
                    });
                }
            } else if let Some(p) = point {
                match self.tool {
                    Tool::Single => {
                        let before = self.entries.clone();
                        self.entries.push(Entry {
                            name: self.name(program),
                            shape: Shape::Single(Placement {
                                kind: self.kind,
                                position: p,
                                heading: 0.,
                            }),
                        });
                        self.selected = vec![Member {
                            entry: self.entries.len() - 1,
                            index: 0,
                        }];
                        self.row = None;
                        self.commit(program, city, before);
                    }
                    Tool::Row => {
                        if let Some(Entry {
                            shape: Shape::Row { row, .. },
                            ..
                        }) = &mut self.draft
                        {
                            row.b = p.truncate();
                            row.control = (row.a + row.b) * 0.5;
                            self.finish_draft(program, city);
                        } else {
                            self.draft = Some(Entry {
                                name: self.name(program),
                                shape: Shape::Row {
                                    kind: self.kind,
                                    row: Row {
                                        a: p.truncate(),
                                        b: p.truncate(),
                                        control: p.truncate(),
                                        elevation: p.z,
                                        spacing: self.spacing,
                                        heading: 0.,
                                        offsets: BTreeMap::new(),
                                        hidden: BTreeSet::new(),
                                    },
                                },
                            });
                            self.gesture = Some(Gesture::Draw { press: pos });
                        }
                    }
                    Tool::Select => {
                        let path = self.row.is_some_and(|index| {
                            if let Shape::Row { row, .. } = &self.entries[index].shape {
                                (0..64).any(|i| {
                                    view.screen(display_position(
                                        city,
                                        row.point(i as f32 / 63.).extend(row.elevation + 0.002),
                                    ))
                                    .is_some_and(|p| p.distance(pos) < 6.)
                                })
                            } else {
                                false
                            }
                        });
                        if path {
                            self.gesture = Some(Gesture::MoveRow {
                                before: self.entries.clone(),
                                entry: self.row.unwrap(),
                                anchor: p.truncate(),
                            });
                        } else {
                            let initial = if ui.input(|i| i.modifiers.shift) {
                                self.selected.clone()
                            } else {
                                Vec::new()
                            };
                            self.selected = initial.clone();
                            self.row = None;
                            self.gesture = Some(Gesture::Box {
                                start: pos,
                                current: pos,
                                initial,
                            });
                        }
                    }
                }
            } else {
                self.error = "No supported surface here".into();
            }
        }
        if !self.active {
            return false;
        }
        if moved && self.gesture.is_none() && in_view {
            if let (
                Some(p),
                Some(Entry {
                    shape: Shape::Row { row, .. },
                    ..
                }),
            ) = (point, &mut self.draft)
            {
                row.b = p.truncate();
                row.control = (row.a + row.b) * 0.5;
            }
        }
        if let Some(mut gesture) = self.gesture.take() {
            let mut keep = true;
            match &mut gesture {
                Gesture::Move {
                    before,
                    anchor,
                    members,
                } => {
                    if moved && response.dragged_by(egui::PointerButton::Primary) {
                        if let Some(p) = point {
                            self.entries = before.clone();
                            self.move_members(city, members, p.truncate() - *anchor);
                        }
                    }
                    if released {
                        self.commit(program, city, before.clone());
                        keep = false;
                    }
                }
                Gesture::Handle {
                    before,
                    entry,
                    index,
                } => {
                    if moved {
                        if let (Some(p), Shape::Row { row, .. }) =
                            (point, &mut self.entries[*entry].shape)
                        {
                            match index {
                                0 => row.a = p.truncate(),
                                1 => row.b = p.truncate(),
                                _ => row.control = p.truncate(),
                            }
                        }
                    }
                    if released {
                        self.commit(program, city, before.clone());
                        keep = false;
                    }
                }
                Gesture::MoveRow {
                    before,
                    entry,
                    anchor,
                } => {
                    if moved {
                        if let Some(p) = point {
                            self.entries = before.clone();
                            self.entries[*entry].translate(p.truncate() - *anchor);
                        }
                    }
                    if released {
                        self.commit(program, city, before.clone());
                        keep = false;
                    }
                }
                Gesture::Draw { press } => {
                    if moved {
                        if let (
                            Some(p),
                            Some(Entry {
                                shape: Shape::Row { row, .. },
                                ..
                            }),
                        ) = (point, &mut self.draft)
                        {
                            row.b = p.truncate();
                            row.control = (row.a + row.b) * 0.5;
                        }
                    }
                    if released {
                        if pos.distance(*press) > 4. {
                            self.finish_draft(program, city);
                        }
                        keep = false;
                    }
                }
                Gesture::Box {
                    start,
                    current,
                    initial,
                } => {
                    *current = pos;
                    if released {
                        let rect = egui::Rect::from_two_pos(*start, pos);
                        self.selected = initial.clone();
                        for s in &samples {
                            if view
                                .screen(s.placement.position)
                                .is_some_and(|p| rect.contains(p))
                                && !self.selected.contains(&s.member)
                            {
                                self.selected.push(s.member);
                            }
                        }
                        keep = false;
                    }
                }
            }
            if keep {
                self.gesture = Some(gesture);
            }
            if moved || released {
                ui.ctx().request_repaint();
            }
        }
        self.active
    }
    fn finish_draft(&mut self, program: &mut Program, city: &City) {
        if let Some(entry) = self.draft.take() {
            if matches!(&entry.shape,Shape::Row{row,..}if row.a.distance(row.b)<0.003) {
                self.error = "Row is too short".into();
                return;
            }
            let before = self.entries.clone();
            self.entries.push(entry);
            self.row = Some(self.entries.len() - 1);
            self.selected.clear();
            self.tool = Tool::Select;
            self.commit(program, city, before);
        }
    }
    fn paint(
        &self,
        ui: &egui::Ui,
        view: Projection,
        city: &City,
        selected: Color32,
        preview: Color32,
    ) {
        let painter = ui.painter_at(view.rect);
        let samples = self.samples(city);
        let members = self.active_members(city);
        for s in samples.iter().filter(|s| members.contains(&s.member)) {
            let r = s.placement.kind.radius() + 0.0005;
            let points: Option<Vec<_>> = (0..24)
                .map(|i| {
                    let t = i as f32 * std::f32::consts::TAU / 24.;
                    view.screen(
                        display_position(city, s.placement.position)
                            + Vec3::new(t.cos() * r, t.sin() * r, 0.001),
                    )
                })
                .collect();
            if let Some(points) = points {
                painter.add(egui::Shape::closed_line(
                    points,
                    egui::Stroke::new(2., selected),
                ));
            }
        }
        if let Some(index) = self.row {
            if let Shape::Row { row, .. } = &self.entries[index].shape {
                let points: Option<Vec<_>> = (0..65)
                    .map(|i| {
                        view.screen(display_position(
                            city,
                            row.point(i as f32 / 64.).extend(row.elevation + 0.002),
                        ))
                    })
                    .collect();
                if let Some(points) = points {
                    painter.add(egui::Shape::line(points, egui::Stroke::new(1., selected)));
                }
                for (i, p) in [row.a, row.b, row.control].iter().enumerate() {
                    if let Some(p) =
                        view.screen(display_position(city, p.extend(row.elevation + 0.002)))
                    {
                        painter.circle_filled(p, 5., ui.visuals().panel_fill);
                        painter.circle_stroke(p, 5., egui::Stroke::new(2., selected));
                        if i == 2 {
                            painter.text(
                                p + egui::vec2(9., 0.),
                                egui::Align2::LEFT_CENTER,
                                "Bend",
                                egui::FontId::proportional(11.),
                                selected,
                            );
                        }
                    }
                }
            }
        }
        if let Some(Gesture::Box { start, current, .. }) = &self.gesture {
            painter.rect_stroke(
                egui::Rect::from_two_pos(*start, *current),
                0.,
                egui::Stroke::new(1., selected),
                egui::StrokeKind::Inside,
            );
        }
        let ghost: Vec<_> = if let Some(draft) = &self.draft {
            draft
                .placements(city)
                .unwrap_or_default()
                .into_iter()
                .map(|(_, p)| p)
                .collect()
        } else if self.gesture.is_some() {
            samples
                .iter()
                .filter(|s| members.contains(&s.member))
                .map(|s| s.placement.clone())
                .collect()
        } else {
            Vec::new()
        };
        for mut p in ghost.into_iter().take(32) {
            p.position = display_position(city, p.position);
            for part in p.parts() {
                for tri in part.mesh.vertices.chunks_exact(3) {
                    let points: Option<Vec<_>> = tri
                        .iter()
                        .map(|v| {
                            view.screen(
                                part.transform
                                    .transform_point3(Vec3::from_array(v.position)),
                            )
                        })
                        .collect();
                    if let Some(points) = points {
                        painter.add(egui::Shape::convex_polygon(
                            points,
                            preview.gamma_multiply(0.22),
                            egui::Stroke::NONE,
                        ));
                    }
                }
            }
        }
    }
}
/// The editor's world renderer uses the same catalogue and placement geometry as the game.
pub(super) fn append_scene(scene: &mut scene3d::Scene, program: &Program, base: Option<&Path>) {
    let Ok(entries) = opencity::props::read(program) else {
        return;
    };
    if entries.is_empty() {
        return;
    }
    let Ok(city) = city(program, base) else {
        return;
    };
    for entry in entries {
        if let Ok(placements) = entry.placements(&city) {
            for (_, mut p) in placements {
                p.position = display_position(&city, p.position);
                for part in p.parts() {
                    for vertex in &part.mesh.vertices {
                        let p = part
                            .transform
                            .transform_point3(Vec3::from_array(vertex.position));
                        scene.vertices.push(scene3d::WorldVertex {
                            point: WorldPoint { x: p.x, y: p.y },
                            height: p.z,
                            depth_bias: -0.001,
                            color: Color32::from_rgb(
                                (vertex.color[0].clamp(0., 1.) * 255.) as u8,
                                (vertex.color[1].clamp(0., 1.) * 255.) as u8,
                                (vertex.color[2].clamp(0., 1.) * 255.) as u8,
                            ),
                            surface: [0.; 4],
                        });
                    }
                }
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    const SOURCE: &str = "road r: carlane carlane; end r(-1,0,1,0); land(-2,-2,2,-2,2,2,-2,2);";
    fn editor(view: CameraView) -> RoadEditor {
        let mut e = crate::tests::sample_editor();
        e.program = parse(SOURCE).unwrap();
        e.saved_program = e.program.clone();
        e.selected_segment = None;
        e.selected_segments.clear();
        e.camera_view = view;
        e.camera_transition = None;
        e.map_pan = egui::Vec2::ZERO;
        e.orbit_target = WorldPoint { x: 0., y: 0. };
        e.map_zoom = 2000.;
        e.fit_scene_once = false;
        e.start_props(Kind::Cone);
        e
    }
    fn frame(e: &mut RoadEditor, ctx: &egui::Context, events: Vec<egui::Event>) {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 700.),
                )),
                events,
                ..Default::default()
            },
            |ui| e.editor_frame_ui(ui),
        );
        out.textures_delta.clear();
    }
    fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            },
        ]
    }
    #[test]
    fn single_placement_uses_native_history_and_source_round_trip() {
        for view in [CameraView::Top, CameraView::Isometric] {
            let mut e = editor(view);
            let ctx = egui::Context::default();
            frame(&mut e, &ctx, vec![]);
            let rect = e.map_rect.unwrap();
            let p = Projection::new(&e, rect)
                .screen(Vec3::new(0., 0., 0.001))
                .unwrap();
            frame(&mut e, &ctx, pointer(p, true));
            frame(&mut e, &ctx, pointer(p, false));
            let entries = opencity::props::read(&e.program).unwrap();
            assert_eq!(entries.len(), 1, "{}", e.minimal.props.error);
            assert!(e.undo_stack.len() > 0);
            let source = e.program.to_road_source();
            assert_eq!(
                opencity::props::read(&parse(&source).unwrap()).unwrap(),
                entries
            );
            assert!(e.restore_history(true));
            assert!(opencity::props::read(&e.program).unwrap().is_empty());
            assert!(e.restore_history(false));
            assert_eq!(opencity::props::read(&e.program).unwrap(), entries);
        }
    }
    #[test]
    fn vehicle_view_places_and_picks_on_flat_raised_and_sloped_roads() {
        for (base, rise) in [(0., 0.), (0.2, 0.), (0.2, 0.12)] {
            let mut e = editor(CameraView::Vehicular);
            e.program = parse(&format!(
                "road r: carlane carlane; end a=r(-1,0,1,0); grade(a,{base},{}); land(-2,-2,2,-2,2,2,-2,2);",
                base + rise
            )).unwrap();
            e.saved_program = e.program.clone();
            e.start_props(Kind::Cone);
            e.vehicle_position = WorldPoint { x: -0.2, y: 0. };
            e.vehicle_heading = 0.;
            e.vehicle_eye_height = base + rise * 0.4 + 0.027;
            let ctx = egui::Context::default();
            frame(&mut e, &ctx, vec![]);
            let city = city(&e.program, None).unwrap();
            let view = Projection::new(&e, e.map_rect.unwrap());
            for (x, y) in [(-0.08, -0.02), (0.03, 0.02), (0.15, -0.025)] {
                let surface = city.surface_at([x, y], f32::INFINITY).unwrap();
                let target = Vec3::new(x, y, surface.height);
                let pos = view.screen(display_position(&city, target)).unwrap();
                assert!(view.rect.contains(pos));
                let (origin, direction) = view.ray(pos);
                assert!(origin.is_finite() && direction.is_finite());
                let hit = view.ground(pos, &city).unwrap_or_else(|| {
                    panic!("visible road is supported: base={base}, rise={rise}, target={target:?}")
                });
                assert!(hit.distance(target) < 0.0001, "{hit:?} vs {target:?}");
                frame(&mut e, &ctx, pointer(pos, true));
                frame(&mut e, &ctx, pointer(pos, false));
                assert!(
                    e.minimal.props.error.is_empty(),
                    "{}",
                    e.minimal.props.error
                );
                assert!(pick(&e.minimal.props.samples(&city), view, pos, &city).is_some());
            }
            assert_eq!(opencity::props::read(&e.program).unwrap().len(), 3);
            assert!(view
                .ground(view.rect.center() - egui::vec2(0., 30.), &city)
                .is_none());
        }
    }
    #[test]
    fn arbitrary_row_and_bend_edit_commit_only_on_release() {
        for mode in [CameraView::Top, CameraView::Isometric] {
            let mut e = editor(mode);
            e.minimal.props.tool = Tool::Row;
            let ctx = egui::Context::default();
            frame(&mut e, &ctx, vec![]);
            let view = Projection::new(&e, e.map_rect.unwrap());
            let a = view.screen(Vec3::new(-0.06, -0.03, 0.001)).unwrap();
            let b = view.screen(Vec3::new(0.05, 0.02, 0.001)).unwrap();
            let before = e.program.clone();
            frame(&mut e, &ctx, pointer(a, true));
            frame(&mut e, &ctx, vec![egui::Event::PointerMoved(b)]);
            assert_eq!(e.program, before);
            frame(&mut e, &ctx, pointer(b, false));
            assert_eq!(
                opencity::props::read(&e.program).unwrap().len(),
                1,
                "{}",
                e.minimal.props.error
            );
            let entry = opencity::props::read(&e.program).unwrap().remove(0);
            let Shape::Row { row, .. } = entry.shape else {
                panic!()
            };
            assert!((row.a.x - row.b.x).abs() > 0.05 && (row.a.y - row.b.y).abs() > 0.03);
            let view = Projection::new(&e, e.map_rect.unwrap());
            let handle = view
                .screen(row.control.extend(row.elevation + 0.002))
                .unwrap();
            let end = view
                .screen((row.control + Vec2::new(0.014, -0.013)).extend(row.elevation + 0.002))
                .unwrap();
            let before = e.program.clone();
            frame(&mut e, &ctx, pointer(handle, true));
            frame(&mut e, &ctx, vec![egui::Event::PointerMoved(end)]);
            assert_eq!(e.program, before);
            frame(&mut e, &ctx, pointer(end, false));
            let changed = opencity::props::read(&e.program).unwrap();
            let Shape::Row { row: new, .. } = &changed[0].shape else {
                panic!()
            };
            assert!(new.control.distance(row.control) > 0.01);
        }
    }
    #[test]
    fn group_member_offsets_delete_and_duplicate_share_the_game_schema() {
        let mut program=parse("road r: carlane; end r(-1,0,1,0); land(-2,-2,2,-2,2,2,-2,2); line=prop_row(cone,-0.1,0,0,0.03,0.1,0,{spacing:2.5,elevation:0.001});").unwrap();
        let city = city(&program, None).unwrap();
        let mut state = Editor::default();
        state.sync(&program);
        let members: Vec<_> = state
            .samples(&city)
            .iter()
            .take(2)
            .map(|s| s.member)
            .collect();
        let before = state.entries.clone();
        state.move_members(&city, &members, Vec2::new(0.004, 0.007));
        state.commit(&mut program, &city, before);
        assert!(state.error.is_empty());
        let Shape::Row { row, .. } = &opencity::props::read(&program).unwrap()[0].shape else {
            panic!()
        };
        assert_eq!(row.offsets.len(), 2);
        state.selected = members;
        state.duplicate(&mut program, &city);
        assert_eq!(opencity::props::read(&program).unwrap().len(), 3);
        state.delete(&mut program, &city);
        assert_eq!(opencity::props::read(&program).unwrap().len(), 1);
    }
    #[test]
    fn stored_props_are_in_native_and_incremental_scene_geometry() {
        let program=parse("road r: carlane; end r(-1,0,1,0); land(-2,-2,2,-2,2,2,-2,2); a=prop(cone,0,0,{elevation:0.001});").unwrap();
        let mut scene = scene3d::Scene::default();
        append_scene(&mut scene, &program, None);
        assert_eq!(scene.vertices.len(), Kind::Cone.parts()[0].vertices.len());
        let base = scene
            .vertices
            .iter()
            .map(|v| v.height)
            .fold(f32::INFINITY, f32::min);
        assert!(
            (base - (0.001 + scene3d::PAVEMENT_OFFSET)).abs() < 0.00001,
            "the rubber foot must sit on the editor's visible pavement"
        );
        assert!(scene.vertices.iter().any(|v| v.height > 0.0109));
        let mut plain = program.clone();
        plain.statements.pop();
        let projection = map::project(&program);
        let palette = editor_palette(egui::Theme::Dark);
        let with = scene3d::build_scene(&program, &projection, palette, &[], None);
        let without = scene3d::build_scene(&plain, &projection, palette, &[], None);
        assert_eq!(
            with.vertices.len() - without.vertices.len(),
            Kind::Cone.parts()[0].vertices.len()
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl RoadEditor {
    pub(super) fn prop_api(
        &mut self,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        use serde_json::json;
        if name == "set_prop_tool" {
            let kind = Kind::from_keyword(args["kind"].as_str().unwrap_or("cone"))
                .ok_or("Expected cone, tree, bench or lamp")?;
            let tool = match args["mode"].as_str().unwrap_or("single") {
                "select" => Tool::Select,
                "single" => Tool::Single,
                "row" => Tool::Row,
                _ => return Err("Expected select, single or row".into()),
            };
            let spacing = args
                .get("spacing")
                .map(|v| v.as_f64().ok_or("Spacing must be a number"))
                .transpose()?
                .map(|n| n as f32)
                .unwrap_or(kind.spacing());
            if !spacing.is_finite() || !(0.6..=50.).contains(&spacing) {
                return Err("Spacing must be between 0.6 and 50 metres".into());
            }
            let view = match args["view"].as_str() {
                None => None,
                Some("top") => Some(CameraView::Top),
                Some("isometric") => Some(CameraView::Isometric),
                Some("vehicular") => Some(CameraView::Vehicular),
                _ => return Err("Expected top, isometric or vehicular".into()),
            };
            self.start_props(kind);
            self.minimal.props.tool = tool;
            self.minimal.props.spacing = spacing;
            if let Some(view) = view {
                self.set_camera_view(view);
            }
        }
        let entries = opencity::props::read(&self.program)?;
        let city = city(&self.program, self.path.as_deref().and_then(Path::parent))?;
        let mut props = Vec::new();
        for entry in entries {
            let points:Vec<_>=entry.placements(&city)?.into_iter().map(|(index,p)|json!({"index":index,"position":p.position.to_array(),"heading":p.heading})).collect();
            props.push(json!({"name":entry.name,"kind":entry.kind().keyword(),"row":matches!(entry.shape,Shape::Row{..}),"placements":points}));
        }
        Ok(
            json!({"props":props,"active":self.minimal.props.active,"kind":self.minimal.props.kind.keyword(),"mode":match self.minimal.props.tool{Tool::Select=>"select",Tool::Single=>"single",Tool::Row=>"row"},"spacing":self.minimal.props.spacing}),
        )
    }
}
