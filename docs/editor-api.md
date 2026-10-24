# Live editor API

The desktop editor exposes the same Rust implementation through two local interfaces. Start it with `cargo run --features editor --bin editor`. An MCP client can launch `target/debug/editor mcp` as a stdio server; the command connects to the already-open editor. A client in any language, including Python, can also open the editor's Unix domain socket directly. The API does not require a Python package in this repository.

The socket is `~/.opencity-agent/agent.sock` by default. Set `OPENCITY_MCP_SOCKET` to the same path in the editor and client to choose another. The bridge requires Unix sockets. Only one editor owns a socket at a time; the default socket and its directory are owner-only.

## Direct socket protocol

Open a new stream for each call. Write one UTF-8 JSON object followed by a newline, then read one newline-terminated JSON response. The request has a tool `name` and an `arguments` object:

```json
{"name":"get_road_state","arguments":{}}
```

The response is an MCP tool result. For ordinary tools, check `isError`, then parse the JSON string in `content[0].text`. For example, `get_road_state` returns the current `.road` `source`, projected `segments`, `intersections`, `landscape`, `landscape_errors`, and `dirty`. A segment has a `segment_index` when its placement is editable, plus `direction` (`forward`, `reverse`, or `both`), `permits_forward`, and `permits_reverse`. Indices may change after replacing source; refresh state before index-based edits. `get_viewport_image` instead returns base64 PNG bytes in an image content item with `mimeType: "image/png"`.

To place a straight road with an existing type, send:

```json
{"name":"add_road","arguments":{"road_type":"local","start_x":0,"start_y":0,"end_x":2,"end_y":0}}
```

This returns a top-level `segment_index`. Optional `control1_x`, `control1_y`, `control2_x`, and `control2_y` set Bézier controls. Optional `start_ref` and `end_ref` use exact junction references from the current state; their coordinates must match. The `replace_document` tool accepts `{ "source": "..." }` with complete valid `.road` source and applies it as one undoable edit. `save_document` requires an absolute `.road` path. Road coordinates use map world units; viewport zoom is 18 to 2400 pixels per unit.

The other tools are `check_design_rules`, `get_viewport`, `set_viewport`, `set_intersection`, `move_road`, `move_control_point`, `move_endpoint`, `undo`, and `redo`. The Rust MCP `tools/list` response provides their current JSON schemas. The [language and editor guide](../docs.md) explains road source and editing semantics.

## MCP stdio

Configure an MCP client to run the editor binary with argument `mcp` (or `--mcp`). The adapter implements line-delimited JSON-RPC 2.0 with `initialize`, `ping`, `tools/list`, and `tools/call`. For example, send `{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_road_state","arguments":{}}}` followed by a newline. The reply's `result` is the same tool result described above. The editor window must already be open.
