# OpenCity documentation

The [README](README.md) has the short project overview, road language example, and run commands. This page contains the detailed editor and language reference.

## Road editor

Clone with `git clone --recurse-submodules https://github.com/antoineMoPa/opencity.git`, or run `git submodule update --init --recursive` in an existing checkout. The editor uses the reusable [egui_frames](https://github.com/antoineMoPa/egui_frames) and [egui_command_palette](https://github.com/antoineMoPa/egui_command_palette) repositories as submodules.

Run the native egui editor with `cargo run`, or open a file immediately with `cargo run -- path/to/city.road`.

### Browser build

Install the `wasm32-unknown-unknown` Rust target and `wasm-bindgen-cli` version `0.2.128`, then run `make build` and `make serve`. Open `http://localhost:8877`; override the port with `make serve PORT=9000` if needed. Running `make` builds and serves in one step. The browser editor starts with the same editable starter city and viewport as the native app. Open uses a local `.road` file; Save, Save As, and GLB export download files through browser dialogs. The page fills the browser viewport, including when the window is resized. WebGPU is required.

The workflow in `.github/workflows/pages.yml` builds and deploys `web/dist` from `master`. In the repository's **Settings > Pages**, choose **GitHub Actions** as the build and deployment source; the first push to `master` or a manual workflow run publishes the site. The page uses relative asset paths so it also works under the repository's GitHub Pages subpath.

The workspace has draggable tabs for the road library, viewport, and inspector. Drag a tab to an edge to split the workspace, or close a side panel and reopen it from the command palette. Use the **File**, **Edit**, **View**, and **Panels** menus for document actions, undo/redo, copy/paste, view settings, and panel placement. **Help > Template Library** opens the [template QA gallery](road_library/qa_gallery.road) in the editor so you can inspect all five built-in road patterns. Open the command palette with Cmd+Shift+P or **View > Command Palette** to search those actions and tools. The viewport stays open.

Top and Isometric use the same depth-tested wgpu scene, including detailed road bands, ramps, intersections, grades, land, and water. Choose **Top** for a plan view or **Isometric** for an angled view. In Isometric, drag empty space to pan. Ctrl+left drag orbits around the middle of the viewport: drag horizontally to rotate and vertically to change the elevation angle. Ctrl+Up and Ctrl+Down change the elevation angle in 5° steps; the View menu has the same commands. The wheel zooms, and Shift+left drag or middle drag also pans. Use Control rather than Command on macOS. Shift drag box-selects in Top; Alt/Option drag or hold B while dragging box-selects in Isometric. Both views let you drag road endpoints and curve controls, select or move intersections, extend a selected endpoint with E, rotate its tangent with R, and start a branch from the selected road's marker. The **Draft pen** places a chain of independent straight segments using the selected road type; Enter or Esc finishes it. Cmd+C copies selected segments and Cmd+V pastes them with their curves and heights, offset by one map unit. **File > Export GLB** writes road bands, elevated side walls, junction surfaces, land, lakes, and rivers for use in a 3D engine. **File > Export SVG** writes a top-down map with road direction arrows, visible names, and per-feature `data-*` attributes for roads, buildings, terrain, vegetation, and water. Road paths follow placement point order; `data-direction` states permitted travel. The placed segment list was removed from the sidebar; select placements in the viewport to inspect them.

Use **Terrain & Water** in the library to draw land, lakes, or rivers: click points, then press Enter to finish or Esc to cancel. A reusable block is placed at the viewport center and can be moved from its inspector. Road and block elevations are relative to the ground plane, with negative values below it. When editing a selected road end, Shift+R rotates its tangent the other way; when extending, click another road end to join it. The branch inspector chooses a cross section element and position: a dedicated branch moves the outer elements to the exit, while **Keep lane on through road** copies them and preserves the through road's cross section.

The app opens on the editable road network traced from `target.png`: two urban grids, I-10, I-95, three freeway interchanges, and an airport. The boot scene is in `examples/target_map.road`, which includes building footprints in its buildable street-grid blocks. The earlier compact example remains in `examples/starter_city.road`. Use **File > New** (Cmd+N on macOS) for a blank map that keeps 23 cross section types in the city, countryside, interstate, transport, and airport library. Hover a type on the left to preview its cross section. Choose a type, then click two points to place a road; every new segment is a Bézier that starts straight. Free placement and point dragging follow the pointer; hold Shift to snap to the quarter-unit grid. The four nearest visible junctions and cross section element ends appear as you hover while drawing or selecting. Click one to attach the new road there. Zoom in to pick closely spaced elements. Keep clicking to extend the path; press Enter or Esc to finish. Select a segment to rename it at the top of the inspector, drag its endpoints or control handles, or press Backspace to delete it. Click a centerline end and press E (or use **Extend / join** in the inspector) to continue with the same cross section. Click another road end to fill the gap with a smoothly connected segment; existing coordinate endpoints become shared junctions automatically. Press R on a selected end to rotate its tangent by 15°, or Shift+R to rotate it the other way. Multiple named roads can share one cross section.

To make an exit or merge, select its parent road, choose an element and position in the inspector, then click **Start branch here** or the corresponding map marker and place the branch end. A dedicated exit carries the chosen element and every element outward on that side into a new reusable cross section. A sidewalk or verge in that range stays on the branch; the highway continuation keeps the other elements and gets a thin `barrier` divider. For an interior exit, the parent Bézier is split at the attachment, and the downstream segment uses the reduced cross section. For a terminal exit, the editor adds a short through segment. The exit itself uses two connected Bézier segments with the same carried cross section. Their band widths stay constant instead of collapsing at the seam. **Keep lane on through road** creates an optional branch that copies the outer elements. Select a placed segment to choose another cross section later, so a subsequent highway segment can retain the reduced lane count or gain a lane. The branch follows its source element's position and direction when the parent is reshaped. The map draws cross sections as colored bands. In Top view, drag from empty map space or use right drag to pan, and scroll to zoom. The editor starts in Dark mode; the moon/sun button on the workspace tab strip or Cmd+J switches themes.

New road placements are calls with shared `map_point` junction identifiers and two numeric Bézier controls. A `lane_point` call attaches a junction to a named parent road, cross section element, and position along the curve; new attachments also store the source direction so the branch turns with the parent. A `lane_split(parent_placement, entry_placement, dedicated|keep_lane, full_placement)` call records the two-segment exit and its lane ownership. The older three-argument form remains readable, and lane attachments without a split call are treated as dedicated exits. Existing four-value straight calls and eight-value numeric Bézier calls remain readable and editable. Road calls inside one level of numerically placed blocks are visible but read only on the map.

Click **+ 90° turn**, **+ 3-way intersection**, or **+ 4-way intersection** under **INTERSECTIONS** in the Road Library to place a standalone junction at the center of the visible map. The selected road type fixes its width and edge cross section; choose a type first if you want a different one. Each open edge has a visible port. Click a port to start a matching road, then click its other end; a road drawn toward a port snaps to that edge and finishes there. Ports accept one matching road approaching from outside the junction. You can also drag an existing matching road end onto a port. Select the junction body to see its ports in the inspector, drag it to move it with attached road ends, rotate a turn or three-way junction by 90°, or delete it. The design checker reports roads whose cross sections differ from the intersection edge. A source call can assign another profile to specific arms, such as `intersection4(crossing, arterial, north, local, south, local);` for local streets meeting an arterial. Older road files with `intersection2(point_name, road_type);`, `intersection3(point_name, road_type);`, or `intersection4(point_name, road_type);` remain readable as shared-center junctions; newly placed intersections store separate named points for their edge ports. The starter city's street intersections now connect through edge ports; its highway interchange has four loop ramps and four direct connectors with lane-specific entry and exit mouths.

Switch to **Cross section** to edit the selected road type's ordered elements, including `verge`, or create a new type. Click an element to change its kind or optional `$n` capture, move it, or remove it. The inspector can delete a type after confirmation; calls that name it remain in the document. **File** offers New, Open, Save, and Save As; on macOS these commands are also in the system menu with Cmd+N, Cmd+O, Cmd+S, and Shift+Cmd+S. The source preview shows the serialized AST. New documents include the built-in library, and saving writes the whole document.

**Undo** and **Redo** are in the toolbar. Use Cmd+Z / Shift+Cmd+Z on macOS, or Ctrl+Z / Ctrl+Shift+Z (also Ctrl+Y) elsewhere. A point drag is one undo step. History is limited to 100 edits, resets on New or Open, and tracks the saved document so undoing back to the saved state clears the unsaved marker.

The **Vehicular** camera shows a perspective view from road level. It starts at the selected road's midpoint, or at the road nearest the current orbit target when none is selected. Drag to look around, use the wheel or W/S to move, A/D to strafe, and R to return to the road camera position. Switch to Top or Isometric to edit the map.

### Live road MCP

Build with `cargo build --features editor --bin editor`, then start the desktop editor normally. It listens on a local, owner-only Unix socket while its window is open. Configure an MCP client to launch `<repository>/target/debug/editor mcp` (`--mcp` is also accepted). That command is a stdio adapter: it connects to the **already-open editor** and does not open another window. Start the editor before calling tools, and restart it once after upgrading from an older build that lacked the socket bridge. The socket defaults to `~/.opencity-agent/agent.sock`; set `OPENCITY_MCP_SOCKET` to the same path in the editor and client to override it. One editor owns the socket at a time. A stale socket is replaced at startup.

The MCP tools are `get_road_state` (serialized AST, projected segments, and intersections), `check_design_rules` (road joins and intersection edge mismatches), `get_viewport`, `set_viewport`, `get_viewport_image` (map PNG), `replace_document` (load road source into the open window as one undoable edit), `save_document` (write an absolute `.road` path), `add_road`, `set_intersection`, `move_road`, `move_control_point`, `move_endpoint`, `undo`, and `redo`. `set_intersection` takes `point_ref`, `kind` (`right_turn`, `three_way`, `four_way`, or `none`), and `road_type` when defining a junction; the point must have the matching number of attached road ends. `add_road` accepts optional `name`, `start_ref`, and `end_ref`; use the exact junction references returned by `get_road_state` to keep roads connected. Coordinates use the map's world units; `zoom` is pixels per unit, from 18 to 2400. Segment indices are top-level AST statement indices returned by `get_road_state` or `add_road`. Road edits through MCP share the editor's undo history. The map inspector also lists design issues; click one to center that join. When a straight join changes between compatible cross sections, the renderer gradually widens or narrows the extra bands at the seam.

The map uses illustrative fixed widths for cross section elements; physical lane widths are not represented by the current language.

The shared [road pattern library](road_library/README.md) can be placed directly in `.road` source with `place(four_way, cross, 0, 0, 0);` and `join(cross, port_east, tee, port_west);`. `place` accepts a built-in asset name, unique instance name, x/y translation, and 0–3 counterclockwise quarter turns. `join` aliases two matching outer endpoints after checking location, cross section, height, tangent, and one-way travel. Parsing expands the patterns into ordinary named roads and junctions; saving writes the expanded, editable map. See [composition.example.road](road_library/composition.example.road).

### Land, water, elevation, and reusable crossings

`land`, `beach`, `concrete`, and `lake` use three or more `(x, y)` pairs to form closed boundaries. `river` uses two or more pairs to form an open path. Assign a name to a feature if you want to set its height. Map units are shared with roads.

`beach(x, y, ...)` defines a closed sandy area. It draws just below adjacent `land` so a larger beach outline can form a thin shore around a land polygon.

`concrete(x, y, ...)` defines a paved area, such as a port quay or airport hardstand. It draws over land and under roads and buildings; separate outlines can leave open water between piers.

`vegetation(type, x, y, ...)` defines a closed area; `forest` is a useful type. It currently draws an area without generating trees. `terrain(height, x, y, ...)` defines an elevated map patch. Overlapping outlines at increasing heights form a terraced mountain; the viewport and GLB export draw the exposed sides between levels. `building(height, x, y, ...)` defines a footprint extruded to that height. A building may include a quoted GLB path after its height, for example `building(0.4, "assets/hall.glb", 0, 0, 1, 0, 1, 1)`. The native editor reads that GLB relative to the `.road` file; the browser editor's building inspector has **Choose GLB…** to load a local asset. The viewport fits embedded triangle meshes within the footprint and specified height, using material base colors and vertex colors. Texture images and animation are not drawn yet. GLB export currently retains the model reference as metadata and exports the building extrusion.

In the editor's **Terrain & Water** section, choose a feature, click its points on the map, then press Enter to finish or Esc to cancel. Select a feature on the map or in the sidebar to change its elevation or delete it. The **Reusable Blocks** section places a saved block at the viewport center; its inspector moves, raises, duplicates, or deletes the instance. A named road placement has elevation and start/end height controls in its inspector.

```text
shore = land(-3, -2, 3, -2, 3, 2, -3, 2);
pond = lake(-1, 0, 0, 0.5, 1, 0);
creek = river(-3, 1, -1, 0.5, 1, 0.6, 3, 1);
woods = vegetation(forest, -2, -1, -1, -1, -1, 1);
ridge = terrain(0.3, 1, 1, 3, 1, 2, 2);
hall = building(0.5, "assets/hall.glb", 0, 0, 1, 0, 1, 1, 0, 1);
elevation(pond, -0.1);
```

Named road placements use the same `elevation(name, z)` call for a constant height. Use `grade(name, start_z, end_z)` to change height linearly from the road's start to its end; for example, `grade(exit_ramp, 0, 0.25)` rises from ground to an overpass. If both calls target a road, `grade` sets its heights. Ground is `0`; positive heights are above ground and negative heights below it. The map uses a road's midpoint height for whole-road paint order, except attached ramp mouths paint at their parent road's layer so the joins stay visible. Selecting a road at a crossing uses its height at the pointer, and selecting a ramp shows its endpoint heights. A block can contain roads and land or water features. Place it with `block_name(x, y)` or `block_name(x, y, z)`; the optional third value raises the whole instance. A named block instance can also use `elevation(instance_name, z)`. Road calls inside blocks use local numeric Bézier coordinates. Local `elevation` and `grade` calls inside a block give its component placements relative heights, which add to the instance height. See [interstate_crossing.road](examples/interstate_crossing.road) and [landscape_and_mobility.road](examples/landscape_and_mobility.road).

Road cross sections also accept `traintrack`, `tramtrack`, and `subwaytrack` elements. The built-in library includes rail, tram, subway, dedicated bike path, runway, taxiway, and apron profiles. These are drawn as map corridors and can be placed like roads; their widths are illustrative.

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

Call `check_design_rules(&program)` to inspect ramp connectivity, crossings, car-lane continuity, and car paths crossing non-car bands in any parsed `.road` program. The result contains a typed rule, message, world position, and the involved road names. Each connected car lane must meet another car lane at the same position and height, with compatible travel direction; a `lane_point` must meet its selected parent car lane at matching height and direction. The surface check samples car-lane centers against nearby sidewalk, verge, median, barrier, and other non-car bands at the same height. Explicit intersection areas and car-only `ramp_mouth` merge surfaces are handled as drivable junction geometry. The checks are advisory and use the editor's fixed band widths, so final vehicle width and game collision mesh still need engine-specific verification. A road definition named `ramp` or ending in `_ramp` also gets endpoint and crossing checks. The editor's **Check design rules** command adds road-edge and intersection-profile checks.

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

Road elements are `busstop`, `sidewalk`, `parkinglane`, `bikelane`, `buslane`, `middle`, `carlane`, `buildingedge`, `verge`, `barrier`, `traintrack`, `tramtrack`, `subwaytrack`, `runway`, `taxiway`, and `apron`. A road element may capture a numeric parameter such as `$1`. Calls accept numeric literals, including negative coordinates, or identifiers and may assign results to names. A semicolon after `end` is optional when reading and always emitted when writing.

Road definitions can declare `direction forward;`, `direction reverse;`, or `direction both;` after their cross section and before `end`. The default is `both`, so existing files remain bidirectional. Forward follows each placed road's first point to its last; reverse travels the other way. `RoadDefinition::lane_direction` gives the allowed travel for a cross section lane. For a bidirectional road with a `middle` divider, lanes before the divider run in reverse and lanes after it run forward, following right-hand traffic. Projected segments expose `direction` and `permits_travel` for routing. The target map's ramps and ramp mouths are explicitly forward, and their paths run from entry to merge.

Run the Rust checks with `cargo test`.
