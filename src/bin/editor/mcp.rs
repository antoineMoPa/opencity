//! Local editor bridge and stdio MCP adapter. The app owns the socket; MCP clients attach to it.
use std::io::{self, BufRead, BufReader, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::{
    fs::PermissionsExt,
    net::{UnixListener, UnixStream},
};
#[cfg(unix)]
use std::path::PathBuf;

use base64::Engine;
use eframe::egui;
use serde_json::{json, Value};

use super::{map, RoadEditor, SegmentSource, View, WorldPoint};
use opencity::Statement;

struct Request {
    name: String,
    args: Value,
    reply: Sender<Value>,
}

pub struct Bridge {
    requests: Receiver<Request>,
    screenshot: Option<Sender<Value>>,
    #[cfg(unix)]
    socket_path: PathBuf,
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required}})
}

fn tools() -> Value {
    json!({"tools":[
        tool("get_road_state", "Get the current road AST source and projected road segments", json!({}), &[]),
        tool("check_design_rules", "Check road joins and intersection edge cross sections", json!({}), &[]),
        tool("get_viewport_image", "Capture the current editor viewport as a PNG image", json!({}), &[]),
        tool("get_viewport", "Get map center and zoom in pixels per world unit", json!({}), &[]),
        tool("set_viewport", "Set map center and zoom", json!({"center_x":{"type":"number"},"center_y":{"type":"number"},"zoom":{"type":"number"}}), &["center_x","center_y","zoom"]),
        tool("replace_document", "Replace the editor document with valid OpenCity road source as one undoable edit", json!({"source":{"type":"string"}}), &["source"]),
        tool("save_document", "Save the current editor document to an absolute .road path", json!({"path":{"type":"string"}}), &["path"]),
        tool("add_road", "Place a cubic road using an existing road type; optional endpoint references share junctions", json!({"road_type":{"type":"string"},"name":{"type":"string"},"start_ref":{"type":"string"},"end_ref":{"type":"string"},"start_x":{"type":"number"},"start_y":{"type":"number"},"end_x":{"type":"number"},"end_y":{"type":"number"},"control1_x":{"type":"number"},"control1_y":{"type":"number"},"control2_x":{"type":"number"},"control2_y":{"type":"number"}}), &["road_type","start_x","start_y","end_x","end_y"]),
        tool("set_intersection", "Define or remove a square turn or intersection with a fixed edge cross section", json!({"point_ref":{"type":"string"},"kind":{"type":"string","enum":["right_turn","three_way","four_way","none"]},"road_type":{"type":"string"}}), &["point_ref","kind"]),
        tool("move_road", "Translate a placed road and its connected endpoints", json!({"segment_index":{"type":"integer"},"dx":{"type":"number"},"dy":{"type":"number"}}), &["segment_index","dx","dy"]),
        tool("move_control_point", "Set the first or second Bézier control point", json!({"segment_index":{"type":"integer"},"control":{"type":"integer","enum":[1,2]},"x":{"type":"number"},"y":{"type":"number"}}), &["segment_index","control","x","y"]),
        tool("move_endpoint", "Set the start or end point, moving connected road ends too", json!({"segment_index":{"type":"integer"},"end":{"type":"boolean"},"x":{"type":"number"},"y":{"type":"number"}}), &["segment_index","end","x","y"]),
        tool("undo", "Undo the latest document edit", json!({}), &[]),
        tool("redo", "Redo the latest undone edit", json!({}), &[])
    ]})
}

impl Bridge {
    #[cfg(unix)]
    pub fn listen(ctx: &egui::Context) -> Result<Self, String> {
        Self::listen_at(socket_path()?, ctx)
    }

    #[cfg(unix)]
    fn listen_at(path: PathBuf, ctx: &egui::Context) -> Result<Self, String> {
        let directory = path.parent().ok_or("MCP socket has no parent directory")?;
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        if std::env::var_os("OPENCITY_MCP_SOCKET").is_none() {
            std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| error.to_string())?;
        }
        if path.exists() {
            if UnixStream::connect(&path).is_ok() {
                return Err(format!("another OpenCity editor owns {}", path.display()));
            }
            std::fs::remove_file(&path).map_err(|error| error.to_string())?;
        }
        let listener = UnixListener::bind(&path).map_err(|error| error.to_string())?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
        let (request_tx, requests) = mpsc::channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            for connection in listener.incoming() {
                let Ok(stream) = connection else { continue };
                let request_tx = request_tx.clone();
                let repaint = repaint.clone();
                std::thread::spawn(move || {
                    let _ = serve_connection(stream, request_tx, repaint);
                });
            }
        });
        Ok(Self {
            requests,
            screenshot: None,
            socket_path: path,
        })
    }

    #[cfg(not(unix))]
    pub fn listen(_ctx: &egui::Context) -> Result<Self, String> {
        Err("The live OpenCity MCP bridge currently requires Unix sockets".into())
    }
}

#[cfg(unix)]
impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[cfg(unix)]
fn socket_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("OPENCITY_MCP_SOCKET") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME").ok_or("HOME must be set for OpenCity MCP")?;
    Ok(PathBuf::from(home)
        .join(".opencity-agent")
        .join("agent.sock"))
}

#[cfg(unix)]
fn serve_connection(
    mut stream: UnixStream,
    requests: Sender<Request>,
    repaint: egui::Context,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(|error| error.to_string())?;
    let mut line = String::new();
    BufReader::new(&stream)
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    let request: Value = serde_json::from_str(&line).map_err(|error| error.to_string())?;
    let name = request["name"]
        .as_str()
        .ok_or("Missing tool name")?
        .to_owned();
    let args = request
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let (reply, response) = mpsc::channel();
    requests
        .send(Request { name, args, reply })
        .map_err(|_| "OpenCity editor is closing".to_string())?;
    repaint.request_repaint();
    let result = response
        .recv_timeout(Duration::from_secs(25))
        .map_err(|error| error.to_string())?;
    writeln!(stream, "{result}").map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(unix)]
fn call_socket(name: &str, args: Value) -> Result<Value, String> {
    call_socket_at(&socket_path()?, name, args)
}

#[cfg(unix)]
fn call_socket_at(path: &std::path::Path, name: &str, args: Value) -> Result<Value, String> {
    let mut stream = UnixStream::connect(path).map_err(|_| {
        "No OpenCity editor is connected. Start the desktop editor first.".to_string()
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(|error| error.to_string())?;
    writeln!(stream, "{}", json!({"name": name, "arguments": args}))
        .map_err(|error| error.to_string())?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    if line.is_empty() {
        return Err("OpenCity editor closed the MCP connection".into());
    }
    serde_json::from_str(&line).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn call_socket(_name: &str, _args: Value) -> Result<Value, String> {
    Err("The live OpenCity MCP bridge currently requires Unix sockets".into())
}

pub fn run_stdio() {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(id) = message.get("id").cloned() else {
            continue;
        };
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let result = match method {
            "initialize" => {
                json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"opencity-road-editor","version":env!("CARGO_PKG_VERSION")}}})
            }
            "ping" => json!({"jsonrpc":"2.0","id":id,"result":{}}),
            "tools/list" => json!({"jsonrpc":"2.0","id":id,"result":tools()}),
            "tools/call" => {
                let params = &message["params"];
                let name = params["name"].as_str().unwrap_or("");
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                match call_socket(name, args) {
                    Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
                    Err(error) => {
                        json!({"jsonrpc":"2.0","id":id,"result":{"isError":true,"content":[{"type":"text","text":error}]}})
                    }
                }
            }
            _ => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}})
            }
        };
        if writeln!(stdout, "{result}").is_err() || stdout.flush().is_err() {
            break;
        }
    }
}

fn tool_reply(result: Result<Value, String>) -> Value {
    let (content, is_error) = match result {
        Ok(value) => (vec![json!({"type":"text","text":value.to_string()})], false),
        Err(error) => (vec![json!({"type":"text","text":error})], true),
    };
    json!({"content":content,"isError":is_error})
}

fn number(args: &Value, key: &str) -> Result<f32, String> {
    let value = args
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("Missing number: {key}"))?;
    if !value.is_finite() || value.abs() > 1_000_000.0 {
        return Err(format!("Invalid number: {key}"));
    }
    Ok(value as f32)
}

fn point(args: &Value, x: &str, y: &str) -> Result<WorldPoint, String> {
    Ok(WorldPoint {
        x: number(args, x)?,
        y: number(args, y)?,
    })
}

fn source(args: &Value, editor: &RoadEditor) -> Result<SegmentSource, String> {
    let index = args["segment_index"]
        .as_u64()
        .ok_or("Missing segment_index")? as usize;
    let source = SegmentSource::TopLevel(index);
    if map::project(&editor.program)
        .segments
        .iter()
        .any(|segment| segment.source == source)
    {
        Ok(source)
    } else {
        Err(format!("No placed road at statement index {index}"))
    }
}

impl RoadEditor {
    pub(super) fn handle_mcp(&mut self, ctx: &egui::Context) {
        let Some(bridge) = &self.mcp else { return };
        if let Ok(request) = bridge.requests.try_recv() {
            if request.name == "get_viewport_image" {
                if let Some(bridge) = &mut self.mcp {
                    if bridge.screenshot.is_some() {
                        let _ = request
                            .reply
                            .send(tool_reply(Err("Screenshot already pending".into())));
                    } else {
                        bridge.screenshot = Some(request.reply);
                        self.view = View::Map;
                        ctx.send_viewport_cmd(
                            egui::ViewportCommand::Screenshot(Default::default()),
                        );
                    }
                }
            } else {
                let before = self.program.clone();
                let result = self.call_mcp(&request.name, &request.args);
                self.record_history(before, false);
                let _ = request.reply.send(tool_reply(result));
            }
            ctx.request_repaint();
        }
        let screenshot = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = screenshot {
            if let Some(bridge) = &mut self.mcp {
                if let Some(reply) = bridge.screenshot.take() {
                    let result = encode_image(&image, self.map_rect, ctx.pixels_per_point())
                        .map(|data| json!({"content":[{"type":"image","data":data,"mimeType":"image/png"}]}));
                    match result {
                        Ok(image_result) => {
                            let _ = reply.send(image_result);
                        }
                        Err(error) => {
                            let _ = reply.send(tool_reply(Err(error)));
                        }
                    }
                }
            }
        }
    }

    pub(super) fn call_mcp(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        match name {
            "check_design_rules" => {
                let projection = map::project(&self.program);
                let profiles = super::paint_profiles(&self.program, &projection);
                let issues: Vec<_> = super::design_rule_issues(&self.program, &projection, &profiles)
                    .into_iter()
                    .map(|issue| json!({"code":issue.code,"message":issue.message,"at":[issue.at.x,issue.at.y],"roads":issue.roads}))
                    .collect();
                Ok(json!({"passed":issues.is_empty(),"issues":issues}))
            }
            "get_road_state" => {
                let projection = map::project(&self.program);
                let intersections: Vec<_> = map::intersections(&self.program, &projection)
                    .iter()
                    .map(|intersection| {
                        json!({"point_ref":intersection.point_ref,"kind":match intersection.kind {
                        map::IntersectionKind::RightTurn => "right_turn",
                        map::IntersectionKind::ThreeWay => "three_way",
                        map::IntersectionKind::FourWay => "four_way",
                    },"road_type":intersection.road_name,"arm_road_types":intersection.arm_roads.iter().map(|(direction, name)| json!({
                        "direction":[direction.x,direction.y],"road_type":name
                    })).collect::<Vec<_>>(),"position":[intersection.position.x,intersection.position.y],
                    "ports":intersection.ports.iter().map(|port| json!({
                        "point_ref":port.point_ref,"position":[port.position.x,port.position.y],
                        "direction":[port.direction.x,port.direction.y]
                    })).collect::<Vec<_>>()})
                    })
                    .collect();
                let segments: Vec<_> = projection.segments.iter().map(|segment| {
                    json!({"segment_index": match segment.source { SegmentSource::TopLevel(index) => Some(index), _ => None },
                        "name":segment.name,"road_type":segment.road_name,"elevation":segment.elevation,
                        "start_z":segment.start_z,"end_z":segment.end_z,
                        "start_ref":segment.start_ref,"end_ref":segment.end_ref,
                        "start":[segment.start.x,segment.start.y],"control1":[segment.controls[0].x,segment.controls[0].y],
                        "control2":[segment.controls[1].x,segment.controls[1].y],"end":[segment.end.x,segment.end.y]})
                }).collect();
                let landscape = opencity::project_landscape_features(&self.program);
                let features: Vec<_> = landscape
                    .features
                    .iter()
                    .map(|feature| {
                        json!({
                            "kind":feature.kind.call_name(),"name":feature.name,
                            "points":feature.points,"elevation":feature.elevation,
                            "height":feature.height,"terrain_height":feature.terrain_height,"model":feature.model,
                            "vegetation_type":feature.vegetation_type
                        })
                    })
                    .collect();
                Ok(
                    json!({"source":self.program.to_road_source(),"segments":segments,
                    "intersections":intersections,"landscape":features,
                    "landscape_errors":landscape.errors.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    "dirty":self.program != self.saved_program}),
                )
            }
            "get_viewport" => {
                let center = self.viewport_center();
                Ok(json!({"center_x":center.x,"center_y":center.y,"zoom":self.map_zoom}))
            }
            "set_viewport" => {
                let center = point(args, "center_x", "center_y")?;
                let zoom = number(args, "zoom")?;
                if !(18.0..=420.0).contains(&zoom) {
                    return Err("zoom must be between 18 and 420".into());
                }
                self.map_zoom = zoom;
                self.map_pan = egui::vec2(-center.x * zoom, center.y * zoom);
                self.fit_scene_once = false;
                self.view = View::Map;
                Ok(json!({"center_x":center.x,"center_y":center.y,"zoom":zoom}))
            }
            "replace_document" => {
                let source = args["source"].as_str().ok_or("Missing source string")?;
                let program = opencity::parse(source).map_err(|error| error.to_string())?;
                let projection = map::project(&program);
                if projection.unresolved != 0 {
                    return Err(format!(
                        "Document has {} unresolved map calls",
                        projection.unresolved
                    ));
                }
                self.selected_road = program
                    .statements
                    .iter()
                    .position(|statement| matches!(statement, Statement::Road(_)));
                self.selected_segment = None;
                self.selected_endpoint = None;
                self.lane_end_choice = None;
                self.map_start = None;
                self.map_start_ref = None;
                self.path = None;
                self.program = program;
                self.road_name_draft = self
                    .road()
                    .map(|road| road.name.clone())
                    .unwrap_or_default();
                self.view = View::Map;
                self.fit_scene_once = true;
                self.status = "Replaced map from MCP".into();
                Ok(json!({"segments":projection.segments.len()}))
            }
            "save_document" => {
                let path = args["path"].as_str().ok_or("Missing path string")?;
                let path = std::path::PathBuf::from(path);
                if !path.is_absolute() {
                    return Err("path must be absolute".into());
                }
                self.program
                    .write_road_file(&path)
                    .map_err(|error| error.to_string())?;
                self.path = Some(path.clone());
                self.saved_program = self.program.clone();
                self.status = format!("Saved {}", path.display());
                Ok(json!({"path":path}))
            }
            "add_road" => {
                let road_type = args["road_type"].as_str().ok_or("Missing road_type")?;
                let name = args
                    .get("name")
                    .map(|value| value.as_str().ok_or("name must be a string"))
                    .transpose()?;
                if let Some(name) = name {
                    if !RoadEditor::valid_name(name) {
                        return Err("Invalid road segment name".into());
                    }
                }
                let start_ref = args
                    .get("start_ref")
                    .map(|value| value.as_str().ok_or("start_ref must be a string"))
                    .transpose()?;
                let end_ref = args
                    .get("end_ref")
                    .map(|value| value.as_str().ok_or("end_ref must be a string"))
                    .transpose()?;
                let road = self
                    .program
                    .statements
                    .iter()
                    .find_map(|statement| match statement {
                        Statement::Road(road) if road.name == road_type => Some(road.clone()),
                        _ => None,
                    })
                    .ok_or_else(|| format!("Unknown road type: {road_type}"))?;
                let start = point(args, "start_x", "start_y")?;
                let end = point(args, "end_x", "end_y")?;
                let projection = map::project(&self.program);
                let intersections = map::intersections(&self.program, &projection);
                for (reference, position) in [(start_ref, start), (end_ref, end)] {
                    if let Some(reference) = reference {
                        let attached = projection
                            .points
                            .iter()
                            .find(|point| point.name == reference)
                            .ok_or_else(|| format!("Unknown junction: {reference}"))?;
                        if (attached.position.x - position.x).abs() > 0.001
                            || (attached.position.y - position.y).abs() > 0.001
                        {
                            return Err(format!("Junction {reference} has different coordinates"));
                        }
                        if let Some(intersection) = intersections.iter().find(|intersection| {
                            intersection
                                .ports
                                .iter()
                                .any(|port| port.point_ref == reference)
                        }) {
                            if intersection.road_name != road_type {
                                return Err(format!(
                                    "Port {reference} requires {}",
                                    intersection.road_name
                                ));
                            }
                            if map::port_in_use(&projection, reference) {
                                return Err(format!("Port {reference} already has a road"));
                            }
                        }
                    }
                }
                for (reference, other) in [(start_ref, end), (end_ref, start)] {
                    if let Some(reference) = reference {
                        if !map::port_faces_road(&self.program, reference, other) {
                            return Err(format!(
                                "Road must approach port {reference} from outside the intersection"
                            ));
                        }
                    }
                }
                let first = if args.get("control1_x").is_some() || args.get("control1_y").is_some()
                {
                    point(args, "control1_x", "control1_y")?
                } else {
                    WorldPoint {
                        x: start.x + (end.x - start.x) / 3.0,
                        y: start.y + (end.y - start.y) / 3.0,
                    }
                };
                let second = if args.get("control2_x").is_some() || args.get("control2_y").is_some()
                {
                    point(args, "control2_x", "control2_y")?
                } else {
                    WorldPoint {
                        x: start.x + 2.0 * (end.x - start.x) / 3.0,
                        y: start.y + 2.0 * (end.y - start.y) / 3.0,
                    }
                };
                let before = self.program.clone();
                let index = if start_ref.is_some() || end_ref.is_some() {
                    map::append_connected_curve(
                        &mut self.program,
                        &road,
                        start,
                        start_ref,
                        first,
                        second,
                        end,
                        end_ref,
                    )
                    .0
                } else {
                    map::append_curve(&mut self.program, &road, start, first, second, end)
                };
                if let Some(name) = name {
                    if !map::set_name(
                        &mut self.program,
                        SegmentSource::TopLevel(index),
                        name.to_owned(),
                    ) {
                        self.program = before;
                        return Err(format!("Segment name is already in use: {name}"));
                    }
                }
                self.view = View::Map;
                Ok(json!({"segment_index":index}))
            }
            "set_intersection" => {
                let point_ref = args["point_ref"].as_str().ok_or("Missing point_ref")?;
                let kind = match args["kind"].as_str().ok_or("Missing kind")? {
                    "right_turn" => Some(map::IntersectionKind::RightTurn),
                    "three_way" => Some(map::IntersectionKind::ThreeWay),
                    "four_way" => Some(map::IntersectionKind::FourWay),
                    "none" => None,
                    _ => return Err("kind must be right_turn, three_way, four_way, or none".into()),
                };
                let road_type = args.get("road_type").and_then(Value::as_str);
                map::set_intersection(&mut self.program, point_ref, kind, road_type)?;
                Ok(json!({"point_ref":point_ref,"kind":args["kind"],"road_type":road_type}))
            }
            "move_road" => {
                let source = source(args, self)?;
                let offset = point(args, "dx", "dy")?;
                let segment = map::project(&self.program)
                    .segments
                    .into_iter()
                    .find(|s| s.source == source)
                    .unwrap();
                let before = self.program.clone();
                if !map::set_connected_endpoint(
                    &mut self.program,
                    source,
                    false,
                    segment.start.translated(offset),
                ) || !map::set_connected_endpoint(
                    &mut self.program,
                    source,
                    true,
                    segment.end.translated(offset),
                ) || !map::set_control(
                    &mut self.program,
                    source,
                    false,
                    segment.controls[0].translated(offset),
                ) || !map::set_control(
                    &mut self.program,
                    source,
                    true,
                    segment.controls[1].translated(offset),
                ) {
                    self.program = before;
                    return Err("Road has an anchored endpoint or cannot be moved".into());
                }
                Ok(json!({"moved":true}))
            }
            "move_control_point" => {
                let source = source(args, self)?;
                let second = match args["control"].as_u64() {
                    Some(1) => false,
                    Some(2) => true,
                    _ => return Err("control must be 1 or 2".into()),
                };
                let position = point(args, "x", "y")?;
                if !map::set_control(&mut self.program, source, second, position) {
                    return Err("Control point cannot be moved".into());
                }
                Ok(json!({"moved":true}))
            }
            "move_endpoint" => {
                let source = source(args, self)?;
                let end = args["end"].as_bool().ok_or("Missing end boolean")?;
                let position = point(args, "x", "y")?;
                if !map::set_connected_endpoint(&mut self.program, source, end, position) {
                    return Err("Endpoint cannot be moved".into());
                }
                Ok(json!({"moved":true}))
            }
            "undo" => Ok(json!({"undone":self.restore_history(true)})),
            "redo" => Ok(json!({"redone":self.restore_history(false)})),
            _ => Err(format!("Unknown tool: {name}")),
        }
    }

    fn viewport_center(&self) -> WorldPoint {
        WorldPoint {
            x: -self.map_pan.x / self.map_zoom,
            y: self.map_pan.y / self.map_zoom,
        }
    }
}

fn encode_image(
    image: &egui::ColorImage,
    rect: Option<egui::Rect>,
    scale: f32,
) -> Result<String, String> {
    let rect = rect.unwrap_or_else(|| {
        egui::Rect::from_min_max(
            egui::Pos2::ZERO,
            egui::pos2(image.size[0] as f32 / scale, image.size[1] as f32 / scale),
        )
    });
    let left = (rect.left() * scale).floor().max(0.0) as usize;
    let top = (rect.top() * scale).floor().max(0.0) as usize;
    let right = (rect.right() * scale).ceil().min(image.size[0] as f32) as usize;
    let bottom = (rect.bottom() * scale).ceil().min(image.size[1] as f32) as usize;
    if right <= left || bottom <= top {
        return Err("Map viewport is unavailable".into());
    }
    let width = right - left;
    let height = bottom - top;
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        let pixels: Vec<u8> = (top..bottom)
            .flat_map(|y| {
                (left..right).flat_map(move |x| image.pixels[y * image.size[0] + x].to_array())
            })
            .collect();
        writer
            .write_image_data(&pixels)
            .map_err(|error| error.to_string())?;
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn socket_client_reaches_the_existing_editor_bridge() {
        let directory = std::env::temp_dir().join(format!(
            "opencity-mcp-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = directory.join("agent.sock");
        let ctx = egui::Context::default();
        let bridge = Bridge::listen_at(path.clone(), &ctx).unwrap();
        assert!(Bridge::listen_at(path.clone(), &ctx).is_err());
        let client = std::thread::spawn({
            let path = path.clone();
            move || call_socket_at(&path, "get_viewport", json!({})).unwrap()
        });
        let request = bridge
            .requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert_eq!(request.name, "get_viewport");
        assert_eq!(request.args, json!({}));
        request
            .reply
            .send(tool_reply(Ok(json!({"zoom": 90}))))
            .unwrap();
        let response = client.join().unwrap();
        assert_eq!(response["content"][0]["text"], r#"{"zoom":90}"#);
        drop(bridge);
        assert!(!path.exists());
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn screenshot_contains_only_map_rectangle() {
        let image = egui::ColorImage::filled([4, 4], egui::Color32::RED);
        let rectangle = egui::Rect::from_min_max(egui::pos2(1.0, 1.0), egui::pos2(3.0, 3.0));
        let encoded = encode_image(&image, Some(rectangle), 1.0).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let reader = decoder.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (2, 2));
    }
}
