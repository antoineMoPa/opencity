# Live editor API

The desktop editor exposes the same Rust implementation through two local interfaces. Start it with
`cargo run --features editor --bin editor`. An MCP client can launch `target/debug/editor mcp` as a
stdio server; the command connects to the already-open editor. A client in any language, including
Python, can also open the editor's Unix domain socket directly. The API does not require a Python
package in this repository.

The socket is `~/.opencity-agent/agent.sock` by default. Set `OPENCITY_MCP_SOCKET` to the same path
in the editor and client to choose another. The bridge requires Unix sockets. Only one editor owns a
socket at a time; the default socket and its directory are owner-only.

## Direct socket protocol

Open a new stream for each call. Write one UTF-8 JSON object followed by a newline, then read one
newline-terminated JSON response. The request has a tool `name` and an `arguments` object:

```json
{ "name": "get_road_state", "arguments": {} }
```

The response is an MCP tool result. For ordinary tools, check `isError`, then parse the JSON string
in `content[0].text`. For example, `get_road_state` returns the current `.road` `source`, projected
`segments`, `intersections`, `landscape`, `landscape_errors`, and `dirty`. A segment has a
`segment_index` when its placement is editable, plus `direction` (`forward`, `reverse`, or `both`),
`permits_forward`, and `permits_reverse`. Indices may change after replacing source; refresh state
before index-based edits. `get_viewport_image` instead returns base64 PNG bytes in an image content
item with `mimeType: "image/png"`.

To place a straight road with an existing type, send:

```json
{
  "name": "add_road",
  "arguments": { "road_type": "local", "start_x": 0, "start_y": 0, "end_x": 2, "end_y": 0 }
}
```

This returns a top-level `segment_index`. Optional `control1_x`, `control1_y`, `control2_x`, and
`control2_y` set Bézier controls. Optional `start_ref` and `end_ref` use exact junction references
from the current state; their coordinates must match. The `replace_document` tool accepts
`{ "source": "..." }` with complete valid `.road` source and applies it as one undoable edit.
`save_document` requires an absolute `.road` path. Road coordinates use map world units; viewport
zoom is 18 to 2400 pixels per unit for ordinary maps, with a 0.1 minimum for documents containing
explicit intersection components.

The other tools are `check_design_rules`, `get_viewport`, `set_viewport`, `set_intersection`,
`move_road`, `move_control_point`, `move_endpoint`, `undo`, and `redo`. The Rust MCP `tools/list`
response provides their current JSON schemas. The [language and editor guide](../docs.md) explains
road source and editing semantics.

## Connection queries and precision edits

`get_connection_workspace` lists named merges and intersections with stable connection IDs, their
member roads, ports, lane attachments, and design-check status. `open_connection_editor` is a
compatibility command: an optional `connection_id` selects and focuses a connection in the full
isometric viewport. It never opens a panel or filters the map. The legacy `camera` argument is
accepted; this compatibility command focuses the isometric view. Top, Isometric and Vehicular remain
available through the view switch. `edit_connection` performs validated, undoable AST edits. See the
[connection editing guide][connection-guide] for arguments and limitations.

`get_viewport_image` accepts `{"include_ui":true}` to capture the entire editor window including the
selected road name field. Omit this option for the map rectangle. Check `design_checks` before
interpreting cached issues; `pending` does not mean that geometry is clear. The abandoned
`cut_connection_segment` and `close_connection_editor` tools have been removed. There is no
workspace to cut within or close; `workspace` is returned as null for query compatibility. Precision
editing remains stateless and uses the full document.

Red map annotations are top-level source calls with unique assigned IDs. Use the existing
source/document API to create or edit them; they do not count as roads, landscape, obstacles or
design-rule geometry:

```road
note_1 = annotation_segment(0, 0, 2, 1);
note_2 = annotation_text("Red label: \"quoted\" \\ path 東京", 1, 2);
```

Coordinates are ground-plane world coordinates. Labels are screen-facing red text; segments are red
overlays. String arguments retain quoted source tokens, including JSON-compatible escapes. These
calls survive save/reopen, document replacement, undo and redo. The palette provides Annotation
segment (two clicks) and Annotation text (click anchor, type, Enter). Escape cancels; clicking
outside an active inline label cancels. No annotation edit/selection API is added; edit the source
or undo to correct markup.

The minimal app's File Clear action (including the legacy internal New command ID and Cmd/Ctrl+N)
always opens a confirmation modal. Confirmation removes top-level calls, keeps road/block
definitions, preserves path/saved baseline and records one undoable edit. Cancel leaves AST/history
and pending placement untouched. The confirmation uses a compact, viewport-bounded modal with
wrapped copy, generous inner spacing and right-aligned equal-height actions; Cancel retains default
keyboard focus. This differs from the backend new-document operation, which still creates a blank
document for automation. Cross-section editing is a dedicated full-window mode reached through the
palette; its profile edits use the same document/history. The viewport Top/Isometric/Vehicular
switch, revealed by hovering the top 100 logical pixels, changes only camera state. Its 320ms
transition preserves geographic focus and scale, and geometry editing pauses until the destination
projection is ready; Box select changes only selection.

[connection-guide]:
  road-building-guide-for-agents.md#8-inspect-and-edit-connections-through-the-rust-api
