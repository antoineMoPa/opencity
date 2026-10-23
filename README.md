# OpenCity

OpenCity is a Rust crate for reading and writing a small city definition language. It parses road cross sections, blocks of calls, and top level calls into a typed AST. The editor maps Bézier road placements and lets you edit reusable cross sections.

## Road editor

Clone with `git clone --recurse-submodules https://github.com/antoineMoPa/opencity.git`, or run `git submodule update --init --recursive` in an existing checkout. The editor uses the reusable [egui_frames](https://github.com/antoineMoPa/egui_frames) and [egui_command_palette](https://github.com/antoineMoPa/egui_command_palette) repositories as submodules.

Run the native egui editor with `cargo run --features editor --bin editor`, or open a file immediately with `cargo run --features editor --bin editor -- path/to/city.road`.

### Browser build

Install the `wasm32-unknown-unknown` Rust target and `wasm-bindgen-cli` version `0.2.128`, then run `make build` and `make serve`. Open `http://localhost:8877`; override the port with `make serve PORT=9000` if needed. Running `make` builds and serves in one step. The browser editor starts with the same editable starter city and viewport as the native app. Open uses a local `.road` file; Save, Save As, and GLB export download files through browser dialogs. The page fills the browser viewport, including when the window is resized. WebGPU is required.

The workflow in `.github/workflows/pages.yml` builds and deploys `web/dist` from `master`. In the repository's **Settings > Pages**, choose **GitHub Actions** as the build and deployment source; the first push to `master` or a manual workflow run publishes the site. The page uses relative asset paths so it also works under the repository's GitHub Pages subpath.

The workspace has draggable tabs for the road library, viewport, and inspector. Drag a tab to an edge to split the workspace, or close a side panel and reopen it from the command palette. Use the **File**, **Edit**, **View**, and **Panels** menus for document actions, undo/redo, copy/paste, view settings, and panel placement. Open the command palette with Cmd+Shift+P or **View > Command Palette** to search those actions and tools. The viewport stays open.

Top and Isometric use the same depth-tested wgpu scene, including detailed road bands, ramps, intersections, grades, land, and water. Choose **Top** for a plan view or **Isometric** for an angled view. In Isometric, drag empty space to pan, Ctrl+left drag rotates around the last selection, and the wheel zooms. Shift+left drag or middle drag also pans. Use Control rather than Command on macOS. Shift drag box-selects in Top; Alt/Option drag or hold B while dragging box-selects in Isometric. Both views let you drag road endpoints and curve controls, select or move intersections, extend a selected endpoint with E, rotate its tangent with R, and start a branch from the selected road's marker. The **Draft pen** places a chain of independent straight segments using the selected road type; Enter or Esc finishes it. Cmd+C copies selected segments and Cmd+V pastes them with their curves and heights, offset by one map unit. **File > Export 3D scene as GLB** writes road bands, elevated side walls, junction surfaces, land, lakes, and rivers for use in a 3D engine. The placed segment list was removed from the sidebar; select placements in the viewport to inspect them.

Use **Terrain & Water** in the library to draw land, lakes, or rivers: click points, then press Enter to finish or Esc to cancel. A reusable block is placed at the viewport center and can be moved from its inspector. Road and block elevations are relative to the ground plane, with negative values below it. When editing a selected road end, Shift+R rotates its tangent the other way; when extending, click another road end to join it. The branch inspector chooses a cross section element and position: a dedicated branch moves the outer elements to the exit, while **Keep lane on through road** copies them and preserves the through road's cross section.

The app opens on an editable tiny city with an irregular market square, winding approach streets, neighborhood lanes, and roads reaching into the surrounding countryside. The starter scene is in `examples/starter_city.road`. Use **File > New** (Cmd+N on macOS) for a blank map that keeps the 20 cross section types in the city, countryside, interstate, and transport library. Hover a type on the left to preview its cross section. Choose a type, then click two points to place a road; every new segment is a Bézier that starts straight. Free placement and point dragging follow the pointer; hold Shift to snap to the quarter-unit grid. Nearby junctions and the ends of individual cross section elements appear while drawing. Click one to attach the new road there. Zoom in to pick closely spaced elements. Keep clicking to extend the path; press Enter or Esc to finish. Select a segment to rename it at the top of the inspector, drag its endpoints or control handles, or press Backspace to delete it. Click a centerline end and press E (or use **Extend / join** in the inspector) to continue with the same cross section. Click another road end to fill the gap with a smoothly connected segment; existing coordinate endpoints become shared junctions automatically. Press R on a selected end to rotate its tangent by 15°, or Shift+R to rotate it the other way. Multiple named roads can share one cross section.

To make an exit or merge, select its parent road, choose an element and position in the inspector, then click **Start branch here** or the corresponding map marker and place the branch end. A dedicated exit carries the chosen element and every element outward on that side into a new reusable cross section. A sidewalk or verge in that range stays on the branch; the highway continuation keeps the other elements and gets a thin `barrier` divider. For an interior exit, the parent Bézier is split at the attachment, and the downstream segment uses the reduced cross section. For a terminal exit, the editor adds a short through segment. The exit itself uses two connected Bézier segments with the same carried cross section. Their band widths stay constant instead of collapsing at the seam. **Keep lane on through road** creates an optional branch that copies the outer elements. Select a placed segment to choose another cross section later, so a subsequent highway segment can retain the reduced lane count or gain a lane. The branch follows its source element's position and direction when the parent is reshaped. The map draws cross sections as colored bands. In Top view, drag from empty map space or use right drag to pan, and scroll to zoom. The editor starts in Dark mode; the moon/sun button on the workspace tab strip or Cmd+J switches themes.

New road placements are calls with shared `map_point` junction identifiers and two numeric Bézier controls. A `lane_point` call attaches a junction to a named parent road, cross section element, and position along the curve; new attachments also store the source direction so the branch turns with the parent. A `lane_split(parent_placement, entry_placement, dedicated|keep_lane, full_placement)` call records the two-segment exit and its lane ownership. The older three-argument form remains readable, and lane attachments without a split call are treated as dedicated exits. Existing four-value straight calls and eight-value numeric Bézier calls remain readable and editable. Road calls inside one level of numerically placed blocks are visible but read only on the map.

Click **+ 90° turn**, **+ 3-way intersection**, or **+ 4-way intersection** under **INTERSECTIONS** in the Road Library to place a standalone junction at the center of the visible map. The selected road type fixes its width and edge cross section; choose a type first if you want a different one. Each open edge has a visible port. Click a port to start a matching road, then click its other end; a road drawn toward a port snaps to that edge and finishes there. Ports accept one matching road approaching from outside the junction. You can also drag an existing matching road end onto a port. Select the junction body to see its ports in the inspector, drag it to move it with attached road ends, rotate a turn or three-way junction by 90°, or delete it. The design checker reports roads whose cross sections differ from the intersection edge. Older road files with `intersection2(point_name, road_type);`, `intersection3(point_name, road_type);`, or `intersection4(point_name, road_type);` remain readable as shared-center junctions; newly placed intersections store separate named points for their edge ports. The starter city's street intersections now connect through edge ports; its highway interchange has four loop ramps and four direct connectors with lane-specific entry and exit mouths.

Switch to **Cross section** to edit the selected road type's ordered elements, including `verge`, or create a new type. Click an element to change its kind or optional `$n` capture, move it, or remove it. The inspector can delete a type after confirmation; calls that name it remain in the document. **File** offers New, Open, Save, and Save As; on macOS these commands are also in the system menu with Cmd+N, Cmd+O, Cmd+S, and Shift+Cmd+S. The source preview shows the serialized AST. New documents include the built-in library, and saving writes the whole document.

**Undo** and **Redo** are in the toolbar. Use Cmd+Z / Shift+Cmd+Z on macOS, or Ctrl+Z / Ctrl+Shift+Z (also Ctrl+Y) elsewhere. A point drag is one undo step. History is limited to 100 edits, resets on New or Open, and tracks the saved document so undoing back to the saved state clears the unsaved marker.

### Live road MCP

Build with `cargo build --features editor --bin editor`, then start the desktop editor normally. It listens on a local, owner-only Unix socket while its window is open. Configure an MCP client to launch `<repository>/target/debug/editor mcp` (`--mcp` is also accepted). That command is a stdio adapter: it connects to the **already-open editor** and does not open another window. Start the editor before calling tools, and restart it once after upgrading from an older build that lacked the socket bridge. The socket defaults to `~/.opencity-agent/agent.sock`; set `OPENCITY_MCP_SOCKET` to the same path in the editor and client to override it. One editor owns the socket at a time. A stale socket is replaced at startup.

The MCP tools are `get_road_state` (serialized AST, projected segments, and intersections), `check_design_rules` (road joins and intersection edge mismatches), `get_viewport`, `set_viewport`, `get_viewport_image` (map PNG), `replace_document` (load road source into the open window as one undoable edit), `save_document` (write an absolute `.road` path), `add_road`, `set_intersection`, `move_road`, `move_control_point`, `move_endpoint`, `undo`, and `redo`. `set_intersection` takes `point_ref`, `kind` (`right_turn`, `three_way`, `four_way`, or `none`), and `road_type` when defining a junction; the point must have the matching number of attached road ends. `add_road` accepts optional `name`, `start_ref`, and `end_ref`; use the exact junction references returned by `get_road_state` to keep roads connected. Coordinates use the map's world units; `zoom` is pixels per unit, from 18 to 420. Segment indices are top-level AST statement indices returned by `get_road_state` or `add_road`. Road edits through MCP share the editor's undo history. The map inspector also lists design issues; click one to center that join. When a straight join changes between compatible cross sections, the renderer gradually widens or narrows the extra bands at the seam.

The map uses illustrative fixed widths for cross section elements; physical lane widths are not represented by the current language.

### Land, water, elevation, and reusable crossings

`land` and `lake` use three or more `(x, y)` pairs to form closed boundaries. `river` uses two or more pairs to form an open path. Assign a name to a feature if you want to set its height. Map units are shared with roads.

In the editor's **Terrain & Water** section, choose a feature, click its points on the map, then press Enter to finish or Esc to cancel. Select a feature on the map or in the sidebar to change its elevation or delete it. The **Reusable Blocks** section places a saved block at the viewport center; its inspector moves, raises, duplicates, or deletes the instance. A named road placement has elevation and start/end height controls in its inspector.

```text
shore = land(-3, -2, 3, -2, 3, 2, -3, 2);
pond = lake(-1, 0, 0, 0.5, 1, 0);
creek = river(-3, 1, -1, 0.5, 1, 0.6, 3, 1);
elevation(pond, -0.1);
```

Named road placements use the same `elevation(name, z)` call for a constant height. Use `grade(name, start_z, end_z)` to change height linearly from the road's start to its end; for example, `grade(exit_ramp, 0, 0.25)` rises from ground to an overpass. If both calls target a road, `grade` sets its heights. Ground is `0`; positive heights are above ground and negative heights below it. The map uses a road's midpoint height for whole-road paint order, except attached ramp mouths paint at their parent road's layer so the joins stay visible. Selecting a road at a crossing uses its height at the pointer, and selecting a ramp shows its endpoint heights. A block can contain roads and land or water features. Place it with `block_name(x, y)` or `block_name(x, y, z)`; the optional third value raises the whole instance. A named block instance can also use `elevation(instance_name, z)`. Road calls inside blocks use local numeric Bézier coordinates. Local `elevation` and `grade` calls inside a block give its component placements relative heights, which add to the instance height. See [interstate_crossing.road](examples/interstate_crossing.road) and [landscape_and_mobility.road](examples/landscape_and_mobility.road).

Road cross sections also accept `traintrack`, `tramtrack`, and `subwaytrack` elements. The built-in library includes rail, tram, subway, and dedicated bike path profiles. These are drawn as map corridors and can be placed like roads; their widths are illustrative.

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

Road elements are `busstop`, `sidewalk`, `parkinglane`, `bikelane`, `buslane`, `middle`, `carlane`, `buildingedge`, `verge`, `barrier`, `traintrack`, `tramtrack`, and `subwaytrack`. A road element may capture a numeric parameter such as `$1`. Calls accept numeric literals, including negative coordinates, or identifiers and may assign results to names. A semicolon after `end` is optional when reading and always emitted when writing.

Run the Rust checks with `cargo test`.
