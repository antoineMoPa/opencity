use super::*;

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required
        }
    })
}

pub(super) fn tools() -> Value {
    let tools = [
        tool(
            "get_road_state",
            "Get the current road AST source and projected road segments",
            json!({}),
            &[],
        ),
        tool(
            "check_design_rules",
            "Check road joins and intersection edge cross sections",
            json!({}),
            &[],
        ),
        tool(
            "get_viewport_image",
            "Capture the current editor viewport as a PNG image; include_ui \
            captures the entire editor window",
            json!({
                "include_ui": {
                    "type": "boolean",
                    "default": false
                }
            }),
            &[],
        ),
        tool(
            "get_viewport",
            "Get map center and zoom in pixels per world unit",
            json!({}),
            &[],
        ),
        tool(
            "set_viewport",
            "Set map center and zoom",
            json!({
                "center_x": {
                    "type": "number"
                },
                "center_y": {
                    "type": "number"
                },
                "zoom": {
                    "type": "number"
                }
            }),
            &["center_x", "center_y", "zoom"],
        ),
        tool(
            "replace_document",
            "Replace the editor document with valid OpenCity road source as \
            one undoable edit",
            json!({
                "source": {
                    "type": "string"
                }
            }),
            &["source"],
        ),
        tool(
            "save_document",
            "Save the current editor document to an absolute .road path",
            json!({
                "path": {
                    "type": "string"
                }
            }),
            &["path"],
        ),
        tool(
            "add_road",
            "Place a cubic road using an existing road type; optional endpoint \
            references share junctions",
            json!({
                "road_type": {
                    "type": "string"
                },
                "name": {
                    "type": "string"
                },
                "start_ref": {
                    "type": "string"
                },
                "end_ref": {
                    "type": "string"
                },
                "start_x": {
                    "type": "number"
                },
                "start_y": {
                    "type": "number"
                },
                "end_x": {
                    "type": "number"
                },
                "end_y": {
                    "type": "number"
                },
                "control1_x": {
                    "type": "number"
                },
                "control1_y": {
                    "type": "number"
                },
                "control2_x": {
                    "type": "number"
                },
                "control2_y": {
                    "type": "number"
                }
            }),
            &["road_type", "start_x", "start_y", "end_x", "end_y"],
        ),
        tool(
            "get_connection_workspace",
            "List stable connection IDs, exact points, ports, lane attachments \
            and cached design issues",
            json!({}),
            &[],
        ),
        tool(
            "open_connection_editor",
            "Compatibility command: focus a connection in the full 3D \
            viewport; no workspace is opened",
            json!({
                "connection_id": {
                    "type": "string"
                },
                "camera": {
                    "type": "string",
                    "enum": [
                        "top",
                        "isometric",
                        "road_level"
                    ]
                }
            }),
            &[],
        ),
        tool(
            "edit_connection",
            "Atomically edit a local connection. Point edits require segment,\
            x,y; grade requires segment,start_height,end_height; attachment \
            requires reference,lane,t; intersection requires x,y,kind,\
            profile. Shared/anchored references are preserved. Run \
            check_design_rules after edits.",
            json!({
                "connection_id": {
                    "type": "string"
                },
                "edit": {
                    "type": "string",
                    "enum": [
                        "start",
                        "handle1",
                        "handle2",
                        "end",
                        "grade",
                        "attachment",
                        "intersection"
                    ]
                },
                "segment": {
                    "type": "string"
                },
                "reference": {
                    "type": "string"
                },
                "lane": {
                    "type": "integer",
                    "minimum": 0
                },
                "t": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1
                },
                "x": {
                    "type": "number"
                },
                "y": {
                    "type": "number"
                },
                "start_height": {
                    "type": "number"
                },
                "end_height": {
                    "type": "number"
                },
                "kind": {
                    "type": "string",
                    "enum": [
                        "right_turn",
                        "three_way",
                        "four_way"
                    ]
                },
                "profile": {
                    "type": "string"
                }
            }),
            &["connection_id", "edit"],
        ),
        tool(
            "set_intersection",
            "Define or remove a square turn or intersection with a fixed edge \
            cross section",
            json!({
                "point_ref": {
                    "type": "string"
                },
                "kind": {
                    "type": "string",
                    "enum": [
                        "right_turn",
                        "three_way",
                        "four_way",
                        "none"
                    ]
                },
                "road_type": {
                    "type": "string"
                }
            }),
            &["point_ref", "kind"],
        ),
        tool(
            "move_road",
            "Translate a placed road and its connected endpoints",
            json!({
                "segment_index": {
                    "type": "integer"
                },
                "dx": {
                    "type": "number"
                },
                "dy": {
                    "type": "number"
                }
            }),
            &["segment_index", "dx", "dy"],
        ),
        tool(
            "move_control_point",
            "Set the first or second Bézier control point",
            json!({
                "segment_index": {
                    "type": "integer"
                },
                "control": {
                    "type": "integer",
                    "enum": [
                        1,
                        2
                    ]
                },
                "x": {
                    "type": "number"
                },
                "y": {
                    "type": "number"
                }
            }),
            &["segment_index", "control", "x", "y"],
        ),
        tool(
            "move_endpoint",
            "Set the start or end point, moving connected road ends too",
            json!({
                "segment_index": {
                    "type": "integer"
                },
                "end": {
                    "type": "boolean"
                },
                "x": {
                    "type": "number"
                },
                "y": {
                    "type": "number"
                }
            }),
            &["segment_index", "end", "x", "y"],
        ),
        tool("undo", "Undo the latest document edit", json!({}), &[]),
        tool("redo", "Redo the latest undone edit", json!({}), &[]),
    ];
    json!({"tools": tools})
}
