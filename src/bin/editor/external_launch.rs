//! Optional external launch action for the current map and a viewport coordinate.
use super::*;

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct LaunchFeedback {
    message: String,
    completion: Option<std::sync::mpsc::Receiver<LaunchEvent>>,
    waiting: bool,
}

#[cfg(not(target_arch = "wasm32"))]
enum LaunchEvent {
    Ready,
    Finished(Result<(), String>),
}

/// A sibling snapshot preserves relative asset paths without overwriting the document.
#[cfg(not(target_arch = "wasm32"))]
struct LaunchSnapshot(PathBuf, Option<PathBuf>);

#[cfg(not(target_arch = "wasm32"))]
impl LaunchSnapshot {
    fn create(program: &Program, map: Option<&Path>) -> Result<Self, String> {
        use std::io::Write;
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let directory = match map {
            Some(path) => std::path::absolute(path)
                .map_err(|e| e.to_string())?
                .parent()
                .unwrap()
                .to_path_buf(),
            None => std::env::current_dir().map_err(|e| e.to_string())?,
        };
        loop {
            let path = directory.join(format!(
                ".opencity-play-{}-{}.road",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    let mut snapshot = Self(path, None);
                    let mut contents = program.clone();
                    // Match elevation_map::load: new default documents use the embedded PNG,
                    // regardless of whether the working directory contains an image by that name.
                    if map.is_none()
                        && opencity::heightmap::ElevationMapSpec::from_program(program)?
                            .is_some_and(|spec| spec.path == "default_elevation.png")
                    {
                        let asset = snapshot.0.with_extension("png");
                        let mut image = std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&asset)
                            .map_err(|e| format!("Cannot prepare elevation snapshot: {e}"))?;
                        snapshot.1 = Some(asset.clone());
                        image
                            .write_all(include_bytes!("../../../examples/default_elevation.png"))
                            .map_err(|e| e.to_string())?;
                        for statement in &mut contents.statements {
                            if let Statement::Call(call) = statement {
                                if call.name == "elevation_map" {
                                    call.args[0] = opencity::Argument::String(format!(
                                        "\"{}\"",
                                        asset.file_name().unwrap().to_str().unwrap()
                                    ));
                                }
                            }
                        }
                    }
                    file.write_all(contents.to_road_source().as_bytes())
                        .map_err(|e| e.to_string())?;
                    return Ok(snapshot);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("Cannot prepare play snapshot: {error}")),
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for LaunchSnapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        if let Some(asset) = &self.1 {
            let _ = std::fs::remove_file(asset);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub(super) struct LaunchAction {
    label: String,
    program: String,
    args: Vec<String>,
    config_dir: PathBuf,
    ready_message: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
impl LaunchAction {
    pub(super) fn from_environment() -> Result<Option<Self>, String> {
        Self::load(
            std::env::var_os("OPENCITY_LAUNCH_ACTION")
                .as_deref()
                .map(std::path::Path::new),
        )
    }

    pub(super) fn load(path: Option<&std::path::Path>) -> Result<Option<Self>, String> {
        let Some(path) = path else { return Ok(None) };
        let path = std::path::absolute(path).map_err(|error| error.to_string())?;
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("Cannot read launch action {}: {error}", path.display()))?;
        Self::parse(&source, path.parent().unwrap().to_path_buf())
            .map(Some)
            .map_err(|error| format!("Invalid launch action {}: {error}", path.display()))
    }

    pub(super) fn parse(source: &str, config_dir: PathBuf) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_str(source).map_err(|error| error.to_string())?;
        let object = value.as_object().ok_or("expected an object")?;
        if let Some(key) = object
            .keys()
            .find(|key| !matches!(key.as_str(), "label" | "program" | "args" | "ready_message"))
        {
            return Err(format!("unknown field {key}"));
        }
        let string = |key| {
            value[key]
                .as_str()
                .filter(|text| !text.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("{key} must be a nonempty string"))
        };
        let label = string("label")?;
        let program = string("program")?;
        let ready_message = object
            .contains_key("ready_message")
            .then(|| string("ready_message"))
            .transpose()?;
        let args = value["args"]
            .as_array()
            .ok_or("args must be an array of strings")?
            .iter()
            .map(|arg| {
                arg.as_str()
                    .map(str::to_owned)
                    .ok_or("args must contain only strings")
            })
            .collect::<Result<Vec<_>, _>>()?;
        for text in std::iter::once(&program).chain(args.iter()) {
            expand_tokens(text, "map", "x", "y", "directory")?;
        }
        Ok(Self {
            label,
            program,
            args,
            config_dir,
            ready_message,
        })
    }

    fn command(
        &self,
        map: &std::path::Path,
        point: [f32; 2],
    ) -> Result<std::process::Command, String> {
        let map = std::path::absolute(map).map_err(|error| error.to_string())?;
        let map = map.to_str().ok_or("map path must be valid UTF-8")?;
        let directory = self
            .config_dir
            .to_str()
            .ok_or("configuration path must be valid UTF-8")?;
        let x = point[0].to_string();
        let y = point[1].to_string();
        let mut command =
            std::process::Command::new(expand_tokens(&self.program, map, &x, &y, directory)?);
        for argument in &self.args {
            command.arg(expand_tokens(argument, map, &x, &y, directory)?);
        }
        Ok(command)
    }
}

/// Expand the four explicit tokens once; inserted path text is never interpreted.
#[cfg(not(target_arch = "wasm32"))]
fn expand_tokens(
    text: &str,
    map: &str,
    x: &str,
    y: &str,
    directory: &str,
) -> Result<String, String> {
    let mut output = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        let end = rest[start..]
            .find('}')
            .map(|end| start + end)
            .ok_or_else(|| format!("unclosed token in {text:?}"))?;
        output.push_str(match &rest[start..=end] {
            "{map}" => map,
            "{x}" => x,
            "{y}" => y,
            "{config_dir}" => directory,
            token => return Err(format!("unknown token {token}")),
        });
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

impl RoadEditor {
    pub(super) fn map_context_menu(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
    ) {
        let id = egui::Id::new("map-context-point");
        if response.secondary_clicked() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let point = match self.camera_view {
                    CameraView::Isometric => {
                        terrain_placement::iso_pick_with_base(
                            &self.program,
                            self.path.as_deref().and_then(Path::parent),
                            |height| {
                                iso_world_at_height(
                                    rect,
                                    self.map_pan,
                                    self.map_zoom,
                                    self.orbit_target,
                                    self.orbit_angle,
                                    self.orbit_elevation,
                                    pointer,
                                    height,
                                )
                            },
                        )
                        .0
                    }
                    _ => world_of(rect, self.map_pan, self.map_zoom, pointer),
                };
                ui.ctx()
                    .data_mut(|data| data.insert_temp(id, [point.x, point.y]));
            }
        }
        response.context_menu(|ui| {
            let point = ui.ctx().data(|data| data.get_temp::<[f32; 2]>(id));
            if let Some(point) = point {
                ui.label(format!("Coordinates: {:.3}, {:.3}", point[0], point[1]));
                #[cfg(not(target_arch = "wasm32"))]
                match &self.launch_action {
                    Ok(Some(action)) => {
                        if ui.button(&action.label).clicked() {
                            self.launch_at(point);
                            ui.close();
                        }
                    }
                    Err(error) => {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    Ok(None) => {}
                }
                if ui.button("Copy coordinates").clicked() {
                    ui.ctx().copy_text(format!("{},{}", point[0], point[1]));
                    self.status = "Copied map coordinates".into();
                    ui.close();
                }
            }
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn launch_at(&mut self, point: [f32; 2]) {
        let action = match &self.launch_action {
            Ok(Some(action)) => action,
            Ok(None) => return,
            Err(error) => {
                self.status = error.clone();
                return;
            }
        };
        let snapshot = if self.dirty || self.program != self.saved_program || self.path.is_none() {
            match LaunchSnapshot::create(&self.program, self.path.as_deref()) {
                Ok(snapshot) => Some(snapshot),
                Err(error) => {
                    self.status = error;
                    self.minimal.launch_feedback = Some(LaunchFeedback {
                        message: self.status.clone(),
                        completion: None,
                        waiting: false,
                    });
                    return;
                }
            }
        } else {
            None
        };
        let map = snapshot
            .as_ref()
            .map(|snapshot| &snapshot.0)
            .or(self.path.as_ref())
            .unwrap();
        match action.command(map, point).and_then(|mut command| {
            command
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|error| error.to_string())
        }) {
            Ok(mut child) => {
                let (sender, receiver) = std::sync::mpsc::channel();
                let mut ready_message = action.ready_message.clone();
                std::thread::spawn(move || {
                    use std::io::Read;
                    // Drain stderr while retaining only its tail, including cargo and startup errors.
                    let mut tail = std::collections::VecDeque::new();
                    let mut buffer = [0; 4096];
                    if let Some(mut stderr) = child.stderr.take() {
                        while let Ok(count) = stderr.read(&mut buffer) {
                            if count == 0 {
                                break;
                            }
                            tail.extend(&buffer[..count]);
                            while tail.len() > 8192 {
                                tail.pop_front();
                            }
                            if ready_message.as_ref().is_some_and(|message| {
                                String::from_utf8_lossy(tail.make_contiguous()).contains(message)
                            }) {
                                let _ = sender.send(LaunchEvent::Ready);
                                ready_message = None;
                            }
                        }
                    }
                    let message = match child.wait() {
                        Ok(status) if status.success() => Ok(()),
                        Ok(status) => Err(format!(
                            "External application failed ({status}):\n{}",
                            String::from_utf8_lossy(&tail.into_iter().collect::<Vec<_>>())
                        )),
                        Err(error) => {
                            Err(format!("Could not wait for external application: {error}"))
                        }
                    };
                    drop(snapshot);
                    let _ = sender.send(LaunchEvent::Finished(message));
                });
                self.status = format!(
                    "Starting {}. Building and loading the map may take a moment.",
                    action.label
                );
                self.minimal.launch_feedback = Some(LaunchFeedback {
                    message: self.status.clone(),
                    completion: Some(receiver),
                    waiting: true,
                });
            }
            Err(error) => {
                self.status = format!("Could not launch {}: {error}", action.label);
                self.minimal.launch_feedback = Some(LaunchFeedback {
                    message: self.status.clone(),
                    completion: None,
                    waiting: false,
                });
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn launch_feedback_ui(&mut self, ctx: &egui::Context) {
        let Some(feedback) = &mut self.minimal.launch_feedback else {
            return;
        };
        if let Some(receiver) = &feedback.completion {
            match receiver.try_recv() {
                Ok(LaunchEvent::Ready) => feedback.waiting = false,
                Ok(LaunchEvent::Finished(Ok(()))) => {
                    self.minimal.launch_feedback = None;
                    return;
                }
                Ok(LaunchEvent::Finished(Err(message))) => {
                    self.status = message.clone();
                    feedback.message = message;
                    feedback.completion = None;
                    feedback.waiting = false;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(250))
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    feedback.message = "Launch monitoring stopped unexpectedly.".into();
                    feedback.completion = None;
                    feedback.waiting = false;
                }
            }
        }
        if feedback.completion.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        if feedback.waiting {
            egui::Area::new(egui::Id::new("external-launch-spinner"))
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.spinner();
                });
            return;
        }
        if feedback.completion.is_some() {
            return;
        }
        let mut open = true;
        egui::Window::new("External application")
            .id(egui::Id::new("external-launch-error"))
            .open(&mut open)
            .default_width(460.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        ui.label(&feedback.message);
                    });
            });
        if !open {
            self.minimal.launch_feedback = None;
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "external_launch/tests.rs"]
mod tests;
