# OpenCity

OpenCity is a Rust crate for reading and writing a small city definition language. It parses road cross sections, blocks of calls, and top level calls into a typed AST. The editor maps Bézier road placements and lets you edit reusable cross sections.

## Road editor

Run the native egui editor with `cargo run --features editor --bin editor`, or open a file immediately with `cargo run --features editor --bin editor -- path/to/city.road`.

The app opens on an editable tiny city with an irregular market square, winding approach streets, neighborhood lanes, and roads reaching into the surrounding countryside. The starter scene is in `examples/starter_city.road`. Use **File > New** (Cmd+N on macOS) for a blank map that keeps the 16 cross section types in the city, countryside, and interstate library. Hover a type on the left to preview its cross section. Choose a type, then click two points to place a road; every new segment is a Bézier that starts straight. Free placement and point dragging follow the pointer; hold Shift to snap to the quarter-unit grid. Nearby junctions and the ends of individual cross section elements appear while drawing. Click one to attach the new road there. Zoom in to pick closely spaced elements. Keep clicking to extend the path; press Enter or Esc to finish. Select a segment to rename it at the top of the inspector, drag its endpoints or control handles, or press Backspace to delete it. Click a centerline end and press E (or use **Extend / join** in the inspector) to continue with the same cross section. Click another road end to fill the gap with a smoothly connected segment; existing coordinate endpoints become shared junctions automatically. Press R on a selected end to rotate its tangent by 15°, or Shift+R to rotate it the other way. Multiple named roads can share one cross section.

To make an exit or merge, select its parent road, choose an element and position in the inspector, then click **Start branch here** or the corresponding map marker and place the branch end. A dedicated exit carries the chosen element and every element outward on that side into a new reusable cross section. A sidewalk or verge in that range stays on the branch; the highway continuation keeps the other elements and gets a thin `barrier` divider. For an interior exit, the parent Bézier is split at the attachment, and the downstream segment uses the reduced cross section. For a terminal exit, the editor adds a short through segment. The exit itself uses two connected Bézier segments with the same carried cross section. Their band widths stay constant instead of collapsing at the seam. **Keep lane on through road** creates an optional branch that copies the outer elements. Select a placed segment to choose another cross section later, so a subsequent highway segment can retain the reduced lane count or gain a lane. The branch follows its source element's position and direction when the parent is reshaped. The map draws cross sections as colored bands. Drag from empty map space or use right drag to pan, and scroll to zoom. **Theme** in the toolbar offers System, Light, and Dark; System follows your computer's appearance.

New road placements are calls with shared `map_point` junction identifiers and two numeric Bézier controls. A `lane_point` call attaches a junction to a named parent road, cross section element, and position along the curve; new attachments also store the source direction so the branch turns with the parent. A `lane_split(parent_placement, entry_placement, dedicated|keep_lane, full_placement)` call records the two-segment exit and its lane ownership. The older three-argument form remains readable, and lane attachments without a split call are treated as dedicated exits. Existing four-value straight calls and eight-value numeric Bézier calls remain readable and editable. Road calls inside one level of numerically placed blocks are visible but read only on the map.

Switch to **Cross section** to edit the selected road type's ordered elements, including `shoulder` and `verge`, or create a new type. Click an element to change its kind or optional `$n` capture, move it, or remove it. The inspector can delete a type after confirmation; calls that name it remain in the document. **File** offers New, Open, Save, and Save As; on macOS these commands are also in the system menu with Cmd+N, Cmd+O, Cmd+S, and Shift+Cmd+S. The source preview shows the serialized AST. New documents include the built-in library, and saving writes the whole document.

**Undo** and **Redo** are in the toolbar. Use Cmd+Z / Shift+Cmd+Z on macOS, or Ctrl+Z / Ctrl+Shift+Z (also Ctrl+Y) elsewhere. A point drag is one undo step. History is limited to 100 edits, resets on New or Open, and tracks the saved document so undoing back to the saved state clears the unsaved marker.

### Live road MCP

Build with `cargo build --features editor --bin editor`, then start the desktop editor normally. It listens on a local, owner-only Unix socket while its window is open. Configure an MCP client to launch `<repository>/target/debug/editor mcp` (`--mcp` is also accepted). That command is a stdio adapter: it connects to the **already-open editor** and does not open another window. Start the editor before calling tools, and restart it once after upgrading from an older build that lacked the socket bridge. The socket defaults to `~/.opencity-agent/agent.sock`; set `OPENCITY_MCP_SOCKET` to the same path in the editor and client to override it. One editor owns the socket at a time. A stale socket is replaced at startup.

The MCP tools are `get_road_state` (serialized AST and projected segments), `check_design_rules` (straight road joins with mismatched edges), `get_viewport`, `set_viewport`, `get_viewport_image` (map PNG), `replace_document` (load road source into the open window as one undoable edit), `save_document` (write an absolute `.road` path), `add_road`, `move_road`, `move_control_point`, `move_endpoint`, `undo`, and `redo`. `add_road` accepts optional `name`, `start_ref`, and `end_ref`; use the exact junction references returned by `get_road_state` to keep roads connected. Coordinates use the map's world units; `zoom` is pixels per unit, from 18 to 420. Segment indices are top-level AST statement indices returned by `get_road_state` or `add_road`. Road edits through MCP share the editor's undo history. The map inspector also lists design issues; click one to center that join. When a straight join changes between compatible cross sections, the renderer gradually widens or narrows the extra bands at the seam.

The map uses illustrative fixed widths for cross section elements; physical lane widths and deeper block evaluation are not represented by the current language.

## Use the crate

Add the local crate to your project's `Cargo.toml`:

```toml
[dependencies]
opencity = { path = "../opencity" }
```

```rust
use opencity::{parse_road_file, Statement};

let program = parse_road_file("definitions/city.road")?;
for statement in &program.statements {
    if let Statement::Road(road) = statement {
        println!("{} has {} elements", road.name, road.elements.len());
    }
}
program.write_road_file("output/definitions/city.road")?;
# Ok::<(), opencity::Error>(())
```

`parse(&str)` and `tokenize(&str)` are also public. `Program::to_road_source()` formats the AST without writing a file. `write_road_file` creates missing parent directories. The writer keeps statement order and number spelling, and its output can be parsed back into the same AST.

The crate is organized under `src/` by responsibility: `lexer.rs`, `parser.rs`, `ast.rs`, `writer.rs`, and `error.rs`. A complete sample input lives at `examples/city.road`. Output locations are explicit: passing `output/definitions/city.road` creates that folder and writes one self-contained road file there.

## Definition language

```text
road normal_road:
    buildingedge($1)-sidewalk-parkinglane-bikelane-buslane-carlane-middle-carlane-buslane-buildingedge($2);
end;

block normal_block:
    a, b = normal_road(0.0, 0.0, 1.0, 0.0);
    buildings(a, b);
end;

normal_block(0.0, 1.0);
```

Road elements are `busstop`, `sidewalk`, `parkinglane`, `bikelane`, `buslane`, `middle`, `carlane`, `buildingedge`, `shoulder`, and `verge`. A road element may capture a numeric parameter such as `$1`. Calls accept numeric literals, including negative coordinates, or identifiers and may assign results to names. A semicolon after `end` is optional when reading and always emitted when writing.

Run the Rust checks with `cargo test`.
