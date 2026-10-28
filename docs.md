# OpenCity documentation

The [README](README.md) has the short project overview, road language example, and run commands.
This page contains the detailed editor and language reference.

## Road editor

Clone with `git clone --recurse-submodules https://github.com/antoineMoPa/opencity.git`, or run `git
submodule update --init --recursive` in an existing checkout. The editor uses the reusable
[egui_command_palette](https://github.com/antoineMoPa/egui_command_palette) submodule and the
vendored MIT Moon toggle switch. The old docking dependency is no longer used by the application.

Run the native egui editor with `cargo run`, or open a file immediately with `cargo run --
path/to/city.road`.

### Browser build

Install the `wasm32-unknown-unknown` Rust target and `wasm-bindgen-cli` version `0.2.128`, then run
`make build` and `make serve`. Open `http://localhost:8877`; override the port with `make serve
PORT=9000` if needed. Running `make` builds and serves in one step. The browser editor starts with
the same editable starter city and viewport as the native app. Open uses a local `.road` file; Save
and Save As download files through browser dialogs. The page fills the browser viewport, including
when the window is resized. WebGPU is required.

The workflow in `.github/workflows/pages.yml` builds and deploys `web/dist` from `main`. In the
repository's **Settings > Pages**, choose **GitHub Actions** as the build and deployment source; the
first push to `main` or a manual workflow run publishes the site. The page uses relative asset paths
so it also works under the repository's GitHub Pages subpath.

The editor fills the window with one viewport, initially Isometric. Open the command palette and
choose **Add road: <profile>**, then click two ground positions to place a road. Escape cancels.
Click a road to select it: circular handles move endpoints, squares shape the curve, and diamonds
slide attached ends along compatible parent car lanes. A selected editable road has a compact name
field; Enter or blur commits a valid unique name, and Escape cancels. This renames the placement,
not its road type. Roads inside block instances are read-only. Click a road endpoint for anchored
options to extend, rotate its tangent or edit its cross section. Click a car-lane end to branch from
that lane; other band ends offer cross-section editing. Dragging still edits geometry directly, and
Escape or an outside click dismisses the options. Extension and branch drafts commit with placement
and support undo; cancellation leaves no extra points in source.

Drag empty space to pan, Ctrl+left drag to orbit, middle drag to pan, and scroll to zoom.
Ctrl+Up/Down changes the elevation angle. Keyboard map navigation pauses during text entry.
Save/Open use Cmd/Ctrl+S and Cmd/Ctrl+O; Shift+Cmd/Ctrl+S saves as, Cmd/Ctrl+N invokes Clear and
always asks for confirmation. Cmd/Ctrl+Z undoes and Shift+Cmd/Ctrl+Z redoes (Ctrl+Y also works
outside macOS). A point drag or committed rename is one undo step; Open resets history; Clear
remains one undoable edit and preserves the file path and saved baseline.

Press Command-Shift-P (Ctrl-Shift-P elsewhere) for the Moon command palette, using its default
styling. Search, use arrows/Enter, or click a command; Escape or an outside click closes it. The
native macOS OpenCity menu provides About and Quit; File provides Clear, Open, Save and Save As with
standard shortcuts. The surface has no dock panels, tabs, in-window menu bar, persistent status
labels. Choose Add terrain, concrete, land, beach, lake, river, vegetation or building in the
palette; click polygon points, then Enter to finish (three minimum; rivers need two). Escape cancels
without adding a feature. Annotation segment places a red line with two clicks. Annotation text
anchors a red label: click, type, Enter to save. Escape or clicking outside the text editor cancels;
blank labels are discarded. Annotations follow the map camera and persist in `.road` source with
undo/redo. Placement hints appear only while a tool is active. Define road profiles, blocks and
complex connections in `.road` files or through the Rust editor API. The same depth-tested wgpu
scene displays road bands, ramps, junctions, grades, land and water. Advanced geometry APIs remain
available without reopening removed UI.

Choose **Edit cross section** in the palette to edit the selected placement’s road profile in a
dedicated full-window mode. A profile picker also reaches existing types. Add, select, change,
reorder or remove bands; edit direction, capture parameters and the profile name; create or delete
profiles and save through the same document actions. Profile changes affect every placement using
that type. Renaming a profile updates road calls and intersection profile references. Lane
reordering/removal updates attachments or preserves detached points. **Back to map** returns to the
viewport; undo/redo works across both modes. The selected-road name field on the map edits only the
placement name. The responsive section body has 24px margins and grouped controls. Clear uses a
compact padded modal with right-aligned actions and Cancel as its default.

Hover the top 100 logical pixels to reveal the Top/Isometric/Vehicular switch at the top right. It
uses the Moon widget’s normal colors and lettering with 10px horizontal and 5px vertical label
padding. View changes animate over 320ms while preserving geographic focus and scale. Entering
Vehicular uses the current view location instead of a distant selected road. Navigation interrupts
the transition; geometry editing resumes once the destination projection is ready.

The palette also offers standalone three-way, four-way and right-turn intersections. In Top or
Isometric, move the preview, press R to rotate and click to place; Escape cancels. A successful
placement commits the profile and native named edge ports together as one undoable edit. These
standalone intersections are ground-level: placement rejects raised terrain, existing road surfaces
and junction-body overlap. It does not split or connect an underlying road automatically; use the
new edge ports for connected road construction.

Click a building in Top or Isometric to select its visible roof or wall and open **Building
properties** beside it. Edit height, material or floor spacing; the approximate floor count is shown
below the fields. Changes preserve its footprint, elevation and optional model path, save to
`.road`, and support undo. Escape, Close or clicking outside dismisses the controls. Selection and
highlighting reuse cached scene geometry and landscape data, so clicking a building does not rebuild
the terrain or upload the map again.

### Live road MCP

Build with `cargo build --features editor --bin editor`, then start the desktop editor normally. It
listens on a local, owner-only Unix socket while its window is open. Configure an MCP client to
launch `<repository>/target/debug/editor mcp` (`--mcp` is also accepted). That command is a stdio
adapter: it connects to the **already-open editor** and does not open another window. Start the
editor before calling tools, and restart it once after upgrading from an older build that lacked the
socket bridge. The socket defaults to `~/.opencity-agent/agent.sock`; set `OPENCITY_MCP_SOCKET` to
the same path in the editor and client to override it. One editor owns the socket at a time. A stale
socket is replaced at startup.

The MCP tools are `get_road_state` (serialized AST, projected segments, and intersections),
`check_design_rules` (road joins and intersection edge mismatches), `get_viewport`, `set_viewport`,
`get_viewport_image` (map PNG), `replace_document` (load road source into the open window as one
undoable edit), `save_document` (write an absolute `.road` path), `add_road`, `set_intersection`,
`move_road`, `move_control_point`, `move_endpoint`, `undo`, and `redo`. `set_intersection` takes
`point_ref`, `kind` (`right_turn`, `three_way`, `four_way`, or `none`), and `road_type` when
defining a junction; the point must have the matching number of attached road ends. `add_road`
accepts optional `name`, `start_ref`, and `end_ref`; use the exact junction references returned by
`get_road_state` to keep roads connected. Coordinates use the map's world units; `zoom` is pixels
per unit, from 18 to 2400. Segment indices are top-level AST statement indices returned by
`get_road_state` or `add_road`. Road edits through MCP share the editor's undo history. Use
`check_design_rules` to inspect design issues. When a straight join changes between compatible cross
sections, the renderer gradually widens or narrows the extra bands at the seam.

The map uses illustrative fixed widths for cross section elements; physical lane widths are not
represented by the current language.

The shared [road pattern library](road_library/README.md) can be placed directly in `.road` source
with `place(four_way, cross, 0, 0, 0);` and `join(cross, port_east, tee, port_west);`. `place`
accepts a built-in asset name, unique instance name, x/y translation, and 0–3 counterclockwise
quarter turns. `join` aliases two matching outer endpoints after checking location, cross section,
height, tangent, and one-way travel. Parsing expands the patterns into ordinary named roads and
junctions; saving writes the expanded, editable map. See
[composition.example.road](road_library/composition.example.road).

### Land, water, elevation, and reusable crossings

`land`, `beach`, `concrete`, and `lake` use three or more `(x, y)` pairs to form closed boundaries.
`river` uses two or more pairs to form an open path. Assign a name to a feature if you want to set
its height. Map units are shared with roads.

`beach(x, y, ...)` defines a closed sandy area. It draws just below adjacent `land` so a larger
beach outline can form a thin shore around a land polygon.

`concrete(x, y, ...)` defines a paved area, such as a port quay or airport hardstand. It draws over
land and under roads and buildings; separate outlines can leave open water between piers.

`vegetation(type, x, y, ...)` defines a closed area; `forest` is a useful type. It currently draws
an area without generating trees. `terrain(height, x, y, ...)` defines an elevated map patch.
Overlapping outlines at increasing heights form a terraced mountain; the viewport and GLB export
draw the exposed sides between levels. `building(height, x, y, ...)` defines a footprint extruded to
that height. A building may include a quoted GLB path after its height, for example `building(0.4,
"assets/hall.glb", 0, 0, 1, 0, 1, 1)`. The native editor reads that GLB relative to the `.road`
file; building assets must already be available to the runtime. The viewport fits embedded triangle
meshes within the footprint and specified height, using material base colors and vertex colors.
Texture images and animation are not drawn yet. GLB export currently retains the model reference as
metadata and exports the building extrusion.

Buildings also accept a facade options object after their height (or after the optional model path):

```text
office = building(0.65, { material: glass, floor_height: 0.045 },
    0, 0, 0.15, 0, 0.15, 0.15, 0, 0.15);
elevation(office, 0.102);
```

`material` is exactly `glass`, `concrete` or `brick`; `floor_height` is a positive finite spacing in
world units. Both options are optional: legacy buildings use concrete and a floor spacing of `0.04`.
Extruded facades use procedural, antialiased light gray floor bands, with glass panel reflections
and a subtle brick pattern. Roofs meet the wall tops and use the material's roof color. Referenced
model meshes retain their own appearance; the style applies to their extrusion fallback. GLB exports
preserve facade colors and static floor bands without requiring the editor shader. Options also work
inside parameterized blocks, and unknown or duplicate properties are rejected.

For continuous terrain, declare one top-level **1024×1024 elevation map**:

```text
elevation_map("assets/elevation.png", -10, -10, 20, 20, 0, 2);
```

The arguments are PNG path, minimum X/Y, world width/depth, and black/white heights in map units.
Width and depth must be positive; white height must be at least black height. Use an 8-bit or 16-bit
grayscale PNG without alpha; RGB, palette images, and other dimensions are rejected. Heights use the
PNG's numeric samples directly, without stretching to the image's observed minimum/maximum. Negative
heights are supported. Pixel `(0, 0)` maps to the minimum X/Y corner; columns increase X and rows
increase Y. Paths resolve relative to the `.road` file. Keep the PNG beside the map when sharing it.

On desktop, **Import elevation map…** in the command palette validates and adds or replaces the
declaration. Its initial bounds are `(-10, -10)` through `(10, 10)`, with heights `0–2`; edit the
declaration for other bounds. Import is undoable. Opening a map reports missing/invalid elevation
assets. The browser editor cannot load external elevation PNG assets yet.

Add `water_level(0.35);` to flood all terrain below that world elevation. The water surface is
horizontal and bounded by the elevation map; depth testing reveals the coastline where it intersects
the terrain, retaining the original underwater heights. Higher terrain remains dry. Picking,
new-road heights, and full-width clearance use the higher of terrain and water, so roads clear the
sea as well as the island. The water plane also exports to GLB. Omit the declaration for dry
terrain; one finite water level is allowed, and it requires an elevation map.

Water uses the elevation map to lighten shallow coastal areas. A small cosine ripple combines water
depth with a four-second looping clock; deep water retains its base color. Animation updates the
shader uniform, reusing the scene mesh and cached elevation texture. Maps with water repaint at
roughly 30 frames per second; dry maps keep their normal event-driven repaint behavior.

The bundled example is an island with six scales of deterministic noise, a sea level of `0.35`, and
black lowlands on all four PNG edges. Rebuild its 16-bit image with `cargo run --example
generate_island`; its Rust generator only produces the raster asset, while the map and water
declaration remain in `.road` source.

The new [default scene](examples/default_map.road) follows the broad archipelago layout in
`target.png`. Its unplaced `terrain_source` block contains editable coastlines (`land`), smooth hill
footprints (`terrain`) and lake basins (`lake`). Run `cargo run --no-default-features --example
bake_elevation` to rebuild its adjacent 16-bit PNG. The baker rounds contour corners, slopes the
shore down into the sea, and leaves inland plains at 0.10 map units for the towns. Sea level is
zero. This bundled PNG is also embedded for unsaved startup documents in desktop and browser builds;
other external PNGs still require desktop file access.

The terrain spans 76.8 × 51.2 map units, three times the reference layout's initial width and depth.
Road widths and terrain heights stay fixed, leaving room for denser streets and a smaller
interchange relative to the islands. Shoreline and hill softness scale with the geography when
baking. Terrain documents also allow the wider overview zoom range needed to see the complete map.

The default road network connects both towns and the southern island through graded strait bridges.
Its central cloverleaf has eight directed turning movements; all four direct connectors follow the
outside of the loops at lowland height, with only the north/south mainline raised to 0.26. Each town
has a four-ramp diamond access, local junction ports and a separate street overpass. The
northeastern highway follows the lowland corridor around the ridge, and highway ends meet local
village or harbor streets. All roads, lane attachments and grades remain editable `.road` calls.
`cargo test --no-default-features --test default_map` checks every design rule, resolved geometry,
whole-network connectivity, shared endpoint heights and full-width terrain clearance, including
terrain pixels between road samples. It also limits road height to 0.35 and longitudinal grade to
20% to catch peaked or excessively steep ramps. Manual Metal previews are available with
`OPENCITY_QA_TARGET=1 cargo test --bin editor render_starter_scene_pngs -- --ignored --nocapture`;
this also writes a GLB to `/private/tmp/opencity-default-map.glb`.

Town T-junctions use the full width of their approaching streets rather than enlarged junction
squares. Four-way pavement follows its curved curbs, leaving the outside corners open without
decorative grass fills.

Each of the 207 town blocks has a 3×3 grid of buildings, with clear setbacks from streets and
junctions. About 86% have 1–4 complete stories. A fixed random seed and a compact half-cosine
city-center envelope create a gradual rise through mid-rise blocks, with a few 9–10 story peaks
immediately downtown. Saved heights are whole multiples of each building's floor spacing. Facades
mix glass, concrete and brick, with more glass downtown. Two concrete waterfront aprons and four
separated piers form port areas, while ten irregular forest polygons occupy open lowland sites.
`cargo test --test default_population` checks all nine lots per block, the low-rise majority and
central peaks, foundations, setbacks and material variety.

Top, Isometric, and Vehicular render the same triangulated elevation surface, with slope shading.
Coplanar regions merge while individual pixel peaks remain. GLB export includes the surface. See
[elevation_map.road](examples/elevation_map.road) and its bundled 16-bit
[elevation.png](examples/elevation.png).

New roads placed in Top/Isometric sample this surface at both endpoints and receive a saved `grade`.
Clearance checks cell maxima across their complete compiled width, including peaks between
endpoints, and adds `0.002` map units above PNG terrain. Free roads retain their initial slope and
may rise above lower terrain; connected endpoints retain their existing grade and obstructed
placements reject atomically. Existing road grades remain authoritative when opening a map or
replacing its elevation image. Standalone ground-level intersections still reject raised terrain.
SVG export does not include elevation-map imagery.

Construct terrain, water, buildings and reusable blocks in `.road` source or through the Rust API.
The minimal editor displays them alongside editable roads.

```text
shore = land(-3, -2, 3, -2, 3, 2, -3, 2);
pond = lake(-1, 0, 0, 0.5, 1, 0);
creek = river(-3, 1, -1, 0.5, 1, 0.6, 3, 1);
woods = vegetation(forest, -2, -1, -1, -1, -1, 1);
ridge = terrain(0.3, 1, 1, 3, 1, 2, 2);
hall = building(0.5, "assets/hall.glb", 0, 0, 1, 0, 1, 1, 0, 1);
elevation(pond, -0.1);
```

Named road placements use the same `elevation(name, z)` call for a constant height. Use `grade(name,
start_z, end_z)` to change height linearly from the road's start to its end; for example,
`grade(exit_ramp, 0, 0.25)` rises from ground to an overpass. If both calls target a road, `grade`
sets its heights. Ground is `0`; positive heights are above ground and negative heights below it.
The map uses a road's midpoint height for whole-road paint order, except attached ramp mouths paint
at their parent road's layer so the joins stay visible. Selecting a road at a crossing uses its
height at the pointer, and selecting a ramp shows its endpoint heights. A block can contain roads
and land or water features. Place it with `block_name(x, y)` or `block_name(x, y, z)`; the optional
third value raises the whole instance. A named block instance can also use `elevation(instance_name,
z)`. Road calls inside blocks use local numeric Bézier coordinates. Local `elevation` and `grade`
calls inside a block give its component placements relative heights, which add to the instance
height. See [interstate_crossing.road](examples/interstate_crossing.road) and
[landscape_and_mobility.road](examples/landscape_and_mobility.road).

Road cross sections also accept `traintrack`, `tramtrack`, and `subwaytrack` elements. The built-in
library includes rail, tram, subway, dedicated bike path, runway, taxiway, and apron profiles. These
are drawn as map corridors and can be placed like roads; their widths are illustrative.

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

`parse(&str)` and `tokenize(&str)` are also public. `Program::to_road_source()` formats the AST
without writing a file. `write_road_file` creates missing parent directories. The writer keeps
statement order and number spelling, and its output can be parsed back into the same AST.

Call `check_design_rules(&program)` to inspect ramp connectivity, crossings, car-lane continuity,
and car paths crossing non-car bands in any parsed `.road` program. The result contains a typed
rule, message, world position, and the involved road names. Each connected car lane must meet
another car lane at the same position and height, with compatible travel direction; a `lane_point`
must meet its selected parent car lane at matching height and direction. The surface check samples
car-lane centers against nearby sidewalk, verge, median, barrier, and other non-car bands at the
same height. Explicit intersection areas and car-only `ramp_mouth` merge surfaces are handled as
drivable junction geometry. The checks are advisory and use the editor's fixed band widths, so final
vehicle width and game collision mesh still need engine-specific verification. A road definition
named `ramp` or ending in `_ramp` also gets endpoint and crossing checks. The editor's
`check_design_rules` API adds road-edge and intersection-profile checks.

The crate is organized under `src/` by responsibility: `lexer.rs`, `parser.rs`, `ast.rs`,
`writer.rs`, and `error.rs`. A complete sample input lives at `examples/city.road`. Output locations
are explicit: passing `output/definitions/city.road` creates that folder and writes one
self-contained road file there.

## Definition language

Separate road elements with spaces. Tabs and line breaks also work. Existing hyphen-separated files
remain readable; saving emits spaces.

```text
road normal_road:
    buildingedge($1) sidewalk parkinglane bikelane buslane carlane middle carlane buslane
    buildingedge($2);
end

block normal_block:
    a, b = normal_road(0.0, 0.0, 1.0, 0.0);
    buildings(a, b);
end

normal_block(0.0, 1.0);
```

Road elements are `busstop`, `sidewalk`, `parkinglane`, `bikelane`, `buslane`, `middle`, `carlane`,
`buildingedge`, `verge`, `barrier`, `traintrack`, `tramtrack`, `subwaytrack`, `runway`, `taxiway`,
and `apron`. A road element may capture a numeric parameter such as `$1`. Calls accept numeric
literals, including negative coordinates, or identifiers and may assign results to names. Roads and
blocks close with `end`, without a semicolon. Existing `end;` syntax remains readable.

Road definitions can declare `direction forward;`, `direction reverse;`, or `direction both;` after
their cross section and before `end`. The default is `both`, so existing files remain bidirectional.
Forward follows each placed road's first point to its last; reverse travels the other way.
`RoadDefinition::lane_direction` gives the allowed travel for a cross section lane. For a
bidirectional road with a `middle` divider, lanes before the divider run in reverse and lanes after
it run forward, following right-hand traffic. Projected segments expose `direction` and
`permits_travel` for routing. The target map's ramps and ramp mouths are explicitly forward, and
their paths run from entry to merge.

Run the Rust checks with `cargo test`.

### Reusable blocks with typed parameters

Blocks can destructure an object using TypeScript-style `{ variables }: { types }` syntax:

```text
block span({ length, height }: { length: number, height: number }):
    lane = normal_road(0, 0, length, 0);
    grade(lane, 0, height);
end

short = span(10, 20, { length: 5, height: 0.5 });
long = span(20, 20, 1, { height: 1, length: 8 });
```

Placement arguments are `x, y, { values }`, `x, y, elevation, { values }`, or `x, y, elevation,
rotation_degrees, { values }`. Coordinates inside the block are local to each placement, and heights
add to the placement's elevation. Object property order does not matter. Every declared parameter is
required; unknown properties and values of the wrong type invalidate the placement. Parameter names
cannot be reused for call results inside the block.

Supported types are `number` (a finite number), `string` (a quoted literal), and `identifier` (a
symbolic name, such as a road profile). An `identifier` parameter may also be called: `profile(0, 0,
length, 0);`. Parameters work in road calls, landscape calls, elevation and grade calls, and nested
block placements. Forward a parameter with `{ length: length }` or the shorthand `{ length }`. Each
instance binds its own values without changing the definition or other instances. Blocks without a
parameter declaration also accept `name(x, y, elevation, rotation_degrees)`. Rotation is
counterclockwise about the placement origin in world coordinates (+X east, +Y north). Nested block
origins rotate with their parent; angles add. Roads, component ports and headings, buildings,
terrain, and other landscape footprints rotate together. Elevations and grades still add
independently of rotation.

See [parameterized_blocks.road](examples/parameterized_blocks.road) for a complete example.

### Parameterized and rotated intersections

Named `intersection`, `interchange`, and `road_segment` definitions accept the same typed parameter
object syntax as blocks:

```road
intersection link({ length, width }: { length: number, width: number }) {
    path lane { move (0, 0); line to (length, 0); }
    surface pavement along lane width width material asphalt;
}
a = link(10, 20, { length: 2, width: 0.2 });
b = link(10, 24, 1, 90, { length: 4, width: 0.3 });
```

Placement arguments are `x, y`, optional elevation, optional rotation in degrees, and finally the
parameter object when declared. Supply elevation `0` to rotate a ground-level instance. Definitions
without parameters can also be rotated, for example `node(10, 20, 0, -45)`.

Numeric parameters work in coordinates, elevations, widths, radii, anchor distances, and numeric
traversal bounds. Identifier parameters work in port headings and band materials/colors. Binding
changes values only; it preserves port, path, anchor, and band names. Forward block parameters to
component instances with `{ length, width }`. Each placement validates its own bindings and resolved
geometry; a bad placement reports an error through `network::project` without hiding valid
instances. Template bodies are parsed and resolved when instantiated. Templates survive `.road`
save/load without acquiring any instance's values.

Component-local +Y points south, as before; rotation is applied after converting to the world's
+Y-north coordinates. Rotated ports remain usable as `instance.port.in` and `instance.port.out` road
endpoints. See [parameterized_intersections.road](examples/parameterized_intersections.road) for
repeated components and a rotated containing block.

Road-profile junction templates can instead contain local `map_point` declarations and one
`intersection2`, `intersection3`, or `intersection4` call. These preserve the editor's profile-aware
pavement, sidewalks, mixed approaches, clipping, and game surfaces:

```road
intersection tee({ profile, min, max }: { profile: identifier, min: number, max: number }) {
    center = map_point(0, 0);
    north = map_point(0, max);
    east = map_point(max, 0);
    west = map_point(min, 0);
    intersection3(center, profile, north, north, east, east, west, west);
}
junction = tee(10, 20, 0, 90, { profile: town_street, min: -0.0895, max: 0.0895 });
```

Road-profile layouts use the ordinary map's +Y-north coordinates and accept multiples of 90 degrees.
Their named points stay local to the instance (`junction.center`, `junction.north`, etc.); rotating
changes each port's world position and direction while preserving its name. The canonical T opens
north/east/west; rotations 90/180/270 move its closed side to east/north/west. Placement elevation
and containing block transforms apply to every port. Template declarations validate their layout
structure; placements validate bindings and coordinates. `map::project` reports invalid placements
as unresolved, and `junction_layouts::expand` supplies detailed errors. Top-level instances remain
movable and rotatable in the editor.

The default map now uses three shared junction templates for all 268 junctions: nine corners, 93
three-way junctions, and 166 four-way junctions. Each placement supplies its road profile and signed
footprint offsets; the three-way and corner placements select their orientation by rotation.
Mixed-profile freeway terminals reuse the same T with larger offsets. Road endpoints refer to the
placed template's local point names. Junctions and derived ports are projected without expanding the
saved map into repeated definitions.



The palette starts with Add road, Add highway, Add interstate, Add terrain and Add concrete. These
road commands map explicitly to neighborhood_street, divided_highway and interstate_three_lane.
Existing definitions with those names take precedence; missing bundled profiles are added only when
placement commits. Cancel leaves no extra definitions or geometry. Custom map profiles remain
searchable. Clear appears after the construction commands, removes all placements while retaining
road and block definitions, and defaults to Cancel. One undo restores the map.

Hover the top 100 logical pixels of the viewport to reveal Moon's Top/Isometric/Vehicular
ToggleSwitch at the top right, inset 12px. It hides when the pointer leaves that strip, keeping a
switch gesture visible through release. Only the switch consumes input; the rest of the strip
remains usable canvas. Clicking it preserves the palette query; Tab still cycles views while the
palette is open. View changes preserve geographic focus and scale through a 320ms projection
transition; Vehicular entry uses nearby traffic lanes, and returning to an overview follows the
latest vehicle location. Vehicular Left/Right steering follows the visible turn direction at 1.5
radians/second. W/S and A/D translate at 0.3 world units/second and scrolling at 0.0003 world
units/pixel, both 10 times slower than before. Choosing a placement tool or Box select from
Vehicular returns to the last editing view. Box select uses a one-shot primary-button marquee in
Top/Isometric; release selects intersecting visible road paths and returns to Select, Escape
cancels. Selection adds no undo entry and does not dirty the file. Existing red annotations remain
world-anchored in all three views; annotation placement uses Top/Isometric.

Road placement in Top/Isometric uses visible terrain surfaces and checks the complete compiled road
surface for clearance. Free roads may be lifted uniformly while preserving their slope, so they can
float over lower terrain patches. This provides conservative clearance rather than terrain-following
road engineering. A route tied to existing anchors rejects without changing the map if terrain
blocks it; choose another endpoint or press Escape. First-click geometry and missing profile
definitions remain drafts until a valid placement commits; profile, road and grade changes share one
undo step.

### Explicit intersections, interchanges, and inspection

The [intersection language guide](docs/intersections.md) covers reusable `intersection`,
`interchange`, and `road_segment` definitions with ports, line/arc/cubic paths, named anchors,
explicit permitted movements, surfaces, and markings. See
[intersections.road](examples/intersections.road) for the proposal's three complete examples. Click
an intersection in the map and choose **Open intersection viewer** to inspect it independently
without editing the document.

Use `opencity intersection-svg INPUT.road --name NAME --output OUTPUT.svg` or `opencity
intersection-png INPUT.road --name NAME --output OUTPUT.png` for headless exports. Both commands
also work through `editor`, and are listed in each executable's `--help`. The guide documents all
export options and the lightweight CLI build.

## Game runtime integration

Enable `game-runtime` with default features disabled to query structured roads, directions, paths,
buildings and landscape, sample physical driving surfaces, and generate procedural meshes and nearby
street details. Optional editor/game WGSL exports let consumers reuse the visual style. See [the
game integration guide](docs/game-runtime.md).

The editor keeps intersection geometry, endpoint candidates and cross-section profiles with the
document projection. Hovering, selecting, copying and opening menus reuse that snapshot; edits and
undo refresh it. This avoids rebuilding the city while handling pointer events. The full default-map
interaction regression can be run with `cargo test --bin editor
default_map_first_click_menus_reuse_document_geometry -- --nocapture`. It checks the first
road/building/context menus in Top and Isometric views against the actual 592-road, 268-junction,
1,863-building map.

### External launch action

In Top or Isometric view, right-click any map position to **Copy coordinates** as `x,y` in authored
map units. The native editor can also show one external launch action when explicitly configured.
Set `OPENCITY_LAUNCH_ACTION` to a JSON file before starting the editor; no configuration is searched
for automatically. Configuration is loaded once at startup. Missing or invalid files show an error
in the context menu, while coordinate copying remains available.

```json
{
  "label": "Inspect here",
  "program": "external-map-viewer",
  "args": ["--map", "{map}", "--position", "{x},{y}", "--assets", "{config_dir}/assets"]
}
```

`label`, `program` and the string array `args` are required. Optional `ready_message` is a nonempty
stderr marker: the centered launch spinner disappears when that text is emitted. Without a marker it
remains until the process exits. Failures appear in a centered error dialog; successful exits close
launch feedback. The four supported tokens are `{map}` (absolute saved map path), `{x}`, `{y}` and
`{config_dir}` (absolute directory containing the configuration). A relative configuration path
resolves against the editor's startup working directory. Tokens may appear inside an argument;
expansion happens once and inserted values remain literal. The executable and arguments are passed
directly to the operating system without a shell, preserving spaces and shell metacharacters.
Relative executable paths resolve against the editor's working directory; use `{config_dir}` for
configuration-relative executables. Launching includes current unsaved edits. When needed, `{map}`
points to a temporary snapshot beside the original map, or in the startup directory for a new
document; relative assets keep the same base directory. For the unsaved default city, its embedded
elevation PNG is exported alongside the snapshot. The original file and editor save state remain
untouched, and the snapshot is removed after the external process exits. The launch window reports
process creation errors and unsuccessful exits with recent stderr output. The browser editor
provides coordinate copying only.

### Road shoulders and safety walls

Use `role highway;` on interstate profiles, including ground-level roads. The bundled interstate
profiles declare this role. Highway, ramp and merge profiles have concrete edge barriers; highways
with a `middle` band also have a median barrier. Explicit `barrier` bands produce solid walls.

Other road types gain edge barriers when their pavement clears the local terrain by more than
0.02 map units. Grass verge and median bands become paved surfaces on protected roads, retaining
their authored widths and lane indices. Merge openings use the shared attachment geometry.
The editor scene, GLB export and game runtime share the barrier prisms.
Protected intersections continue these barriers around their curved exterior and closed sides,
leaving approach mouths open. Their grass bands use pavement, matching the connected road decks.

Runtime consumers can inspect `City::road_barriers` and use `City::road_barrier_contact` for circle
contacts with a vertical interval. Barriers render as props and do not support tires; their height
is 0.011 map units and their width is 0.004 map units.
