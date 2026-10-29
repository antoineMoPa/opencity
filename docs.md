# OpenCity documentation

## Road editor

```sh
git submodule update --init --recursive
cargo run                       # bundled map
cargo run -- path/to/city.road
make fmt                        # format Rust
make lint                       # formatting and source limits
make hooks                      # enable pre-commit checks
cargo test
```

Source limits: 100 columns for Markdown/Rust; 1,200 lines per Rust file. Native scene preparation
runs on a worker. Profile startup without a GPU:
`OPENCITY_SCENE_PROFILE=1 cargo test --bin editor profile_default_startup -- --ignored --nocapture`.

### Browser build

Install the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` `0.2.128`, then run `make`.
Alternatively, `make build` writes `web/dist`; `make serve PORT=8877` serves it. WebGPU is required.
Open reads local `.road` files; Save downloads them. External elevation PNGs require desktop access;
the default map's elevation is embedded. GitHub Pages deploys from `main` through
`.github/workflows/pages.yml`; select **GitHub Actions** in repository Pages settings.

### Editor controls

- Cmd/Ctrl+Shift+P opens the command palette. Escape cancels tools and dialogs.
- Drag empty space or middle-drag to pan; Ctrl+drag orbits; scroll zooms.
  Ctrl+Up/Down changes elevation angle. Hover the top edge for Top/Isometric/Vehicular views.
- Cmd/Ctrl+O opens; Cmd/Ctrl+S saves; Shift+Cmd/Ctrl+S saves as. Cmd/Ctrl+N clears with
  confirmation.
  Cmd/Ctrl+Z undoes; Shift+Cmd/Ctrl+Z redoes. Open resets history; Clear is undoable.
- **Add road** takes two ground clicks. Select a road to rename it or drag circular endpoint,
  square curve and diamond lane-attachment handles. Endpoint menus extend or branch roads.
  Roads inside block instances are read-only.
- **Edit cross section** edits the shared profile; changes affect every placement using it.
  Profile renames update references. **Back to map** returns without losing undo history.
- Add landscape/building polygons by clicking vertices, then Enter (three minimum; rivers need two).
  Annotation segments take two clicks; annotation text takes a position and text.
- Standalone intersections: R rotates, click places. They require clear ground and do not split
  underlying roads automatically. Connect their named edge ports explicitly.
- Select a building in Top/Isometric to edit height, material and floor spacing. Select a zone or
  generated volume to edit zoning. Geometry and selection reuse cached scene data.

New roads sample terrain at both ends and check full-width clearance. Free roads can rise while
retaining slope; attached endpoints keep their grade, rejecting obstructed placements. Existing
map grades remain authoritative. Cancelled tools leave no source changes.

### Live road MCP

Start the editor, then configure your client to run `target/debug/editor mcp`. This stdio adapter
connects to the existing window through `~/.opencity-agent/agent.sock`; override both ends with
`OPENCITY_MCP_SOCKET`. One editor owns the socket. API edits share editor undo history.

See the [editor API](docs/editor-api.md) for tools and the Unix socket protocol, and the
[road-building guide](docs/road-building-guide-for-agents.md) for tracing and valid joins.

### Land, water, elevation, and reusable crossings

`land`, `beach`, `concrete`, `lake`, `vegetation` and `terrain` take polygon footprints;
`river` takes an open path. Heights and coordinates use map units. Named features accept
`elevation(name, z)`; roads also accept `grade(name, start_z, end_z)`.

```road
shore = land(-3, -2, 3, -2, 3, 2, -3, 2);
woods = vegetation(forest, -2, -1, -1, -1, -1, 1);
ridge = terrain(0.3, 1, 1, 3, 1, 2, 2);
office = building(0.65, { material: glass, floor_height: 0.045 },
    0, 0, 0.15, 0, 0.15, 0.15, 0, 0.15);
elevation(office, 0.102);
```

Building materials are `glass`, `concrete`, `brick`; default floor spacing is `0.04`.
An optional GLB path follows height, before options/coordinates. Native preview fits model meshes
to the footprint and height; textures/animation are unsupported. GLB export uses the extrusion.
Vegetation currently marks an area without generating trees.

```road
elevation_map("assets/elevation.png", -10, -10, 20, 20, 0, 2);
water_level(0.35);
```

Declare at most one elevation map: path, minimum X/Y, positive width/depth, black/white heights.
PNG must be 1024², 8/16-bit grayscale without alpha. Numeric samples map directly to the supplied
heights; pixel `(0,0)` is minimum X/Y, columns increase X, rows increase Y. Paths are map-relative.
**Import elevation map…** adds/replaces the declaration with undo support. Optional `water_level`
floods the map below one finite elevation and requires an elevation map. Road placement clears
both terrain and water. GLB includes terrain/water; SVG omits elevation imagery.

The [default map](examples/default_map.road) uses editable contours in its `terrain_source` block.
Rebuild its elevation PNG with `cargo run --no-default-features --example bake_elevation`.
Check connectivity, grades and clearance with
`cargo test --no-default-features --test default_map`; check building placement with
`cargo test --test default_population`. Native preview/export:
`OPENCITY_QA_TARGET=1 cargo test --bin editor render_starter_scene_pngs -- --ignored --nocapture`.

The [road pattern library](road_library/README.md) supports
`place(four_way, cross, 0, 0, 0);` and `join(cross, port_east, tee, port_west);`.
Placement takes asset, name, X/Y and 0–3 counterclockwise quarter turns. Joins validate position,
height, cross-section, tangent and direction. Saving writes expanded editable roads. See
[composition.example.road](road_library/composition.example.road).

## Use the crate

```toml
[dependencies]
opencity = { path = "../opencity" }
```

```rust
let program = opencity::parse_road_file("examples/city.road")?;
program.write_road_file("output/city.road")?;
# Ok::<(), opencity::Error>(())
```

`parse(&str)`, `tokenize(&str)` and `Program::to_road_source()` support in-memory use.
Writing creates parent directories and preserves statement order and numeric spelling.
`check_design_rules(&program)` reports typed diagnostics for connectivity, heights, directions and
car-lane continuity; editor checks additionally cover road edges and intersection profiles.
Checks use illustrative band widths; consumers still validate vehicle collision geometry.

## Definition language

```road
road street:
    sidewalk carlane middle carlane sidewalk;
end
main = street(0, 0, 1, 0);
grade(main, 0, 0.1);
```

Elements are space-separated; legacy hyphen separators and `end;` remain readable. Definitions
end with `end`. Calls accept numeric literals/identifiers and may assign named results; elements
can capture numeric parameters such as `$1`.

Bands include `sidewalk`, `parkinglane`, `bikelane`, `buslane`, `carlane`, `middle`, `busstop`,
`buildingedge`, `verge`, `barrier`, `traintrack`, `tramtrack`, `subwaytrack`, `runway`, `taxiway`,
`apron`. Widths are illustrative. Profiles may declare `direction forward|reverse|both;`;
default `both` uses reverse lanes before `middle`, forward lanes after it. Forward follows the
placement's authored start-to-end direction. Projected segments expose `direction`/`permits_travel`.

### Reusable blocks with typed parameters

```road
block span({ length, height }: { length: number, height: number }):
    lane = street(0, 0, length, 0);
    grade(lane, 0, height);
end
short = span(10, 20, { length: 5, height: 0.5 });
long = span(20, 20, 1, 90, { length: 8, height: 1 });
```

Arguments are X/Y, optional elevation, optional rotation in degrees, then the parameter object.
All declared parameters are required, with types `number` (finite), `string` or `identifier`.
Unknown/wrong-type properties are errors. Identifier parameters can be called as road profiles;
forward parameters with `{ length }`. Names cannot also name call results inside the block.

Block coordinates are local, rotation is counterclockwise (+X east, +Y north), and nested angles
and elevations add. Each instance binds independently. See
[parameterized_blocks.road](examples/parameterized_blocks.road).

### Parameterized and rotated intersections

`intersection`, `interchange` and `road_segment` accept the same parameter objects and placement
arguments. Components use local +Y south, converted before world rotation; ports remain usable as
`instance.port.in`/`.out`. Parameters bind values without renaming ports or paths. Invalid instances
produce `network::project` diagnostics without hiding valid instances. See
[parameterized_intersections.road](examples/parameterized_intersections.road).

Profile-aware junction templates can declare local `map_point`s and call `intersection2/3/4`.
These use world +Y north and rotations in multiples of 90 degrees; named points stay local to the
instance. `junction_layouts::expand` validates geometry; saved maps retain templates. The default
map uses shared corner, T and four-way templates.

### Frontage zoning and building scaffolds

```road
market = frontage_zone(main, {
    use: commercial, side: both, start: 0.1, end: 0.9, depth: 0.16, seed: 2147
});
```

All six fields are required. Uses: `residential|commercial|mixed_use`; sides: `left|right|both`
relative to authored direction. Cubic parameters satisfy `0 <= start < end <= 1`; positive depth
starts outside the full cross-section. Seed is unsigned 64-bit. Zone and referenced top-level road
names must be unique; block-local/generated roads are unsupported. Editor renames update references.

Optional controls: `form: street|city_centre`, `density` (0..1), `height_min`, `height_max`,
`frontage_min`, `frontage_max`, `setback` (distances in map units). Density retains stable slots.
`runtime::scaffold::expand(source, City::from_program)` supplies the shared editor/game layout;
`scaffold::buildings` exposes stable IDs and geometry. Expansion validates occupancy and protects
models, typed buildings and shells taller than 0.16. Consumers own regional dressing; generated
buildings are not saved. Legacy `residential_area(...)` uses the same pipeline.

`project_frontage_zones` returns zones and diagnostics; City rejects invalid zones. Frames/strips
are approximate reservations. Building options accept `use` and zero-based `front_edge`;
`City::building_frontage` supplies a frame. Residential details exclude commercial/mixed-use.
Edit zone properties in Top/Isometric; author references/sides/intervals in source. See
[frontage_zones.road](examples/frontage_zones.road).

### Explicit intersections, interchanges, and inspection

The [intersection guide](docs/intersections.md) covers ports, paths, anchors, permitted movements,
surfaces and markings. Select an intersection and choose **Open intersection viewer** to inspect it.
Headless exports (also accepted by `editor`):

```sh
opencity intersection-svg INPUT.road --name NAME --output OUTPUT.svg
opencity intersection-png INPUT.road --name NAME --output OUTPUT.png
```

## Game runtime integration

Enable `game-runtime` with default features disabled for renderer-independent City queries,
physical surfaces, procedural meshes and details. See the [game guide](docs/game-runtime.md).

### Runtime shader contract

`GAME_SHADER` keeps the 52-byte vertex layout documented in the game guide. Group 0/binding 0 is
288 bytes, ordered: view-projection and light view-projection matrices; eye XYZ/time; sun
XYZ/exposure; daylight/three reserved floats; inverse view-projection; sun RGB/intensity; star
density/size/brightness/angular footprint; star RGB/solar visibility. Matrices are column-major,
clip depth 0..1; angular footprint is vertical FOV / target height.

Group 1: 2048² depth texture + comparison sampler. Group 2: linear HDR sky cube + filtering sampler.
Scene pipelines use all groups and sRGB output; shadows use group 0. Fence shadows use
`vs_shadow`/`fs_shadow` masking; opaque casters may omit the fragment stage. `fs_opaque` has no
discard.
Background/glass add `SKY_STARS_SHADER` radiance before exposure; fog samples only the atmosphere.

Sidewalk metadata `[Sidewalk, distance, 0, 2]` carries distance through junction clipping for
transverse gaps (spacing 0.015, width 0.00015 map units). Paths use local depth (`surface.w = 1`);
curbs/plain concrete have no joints.

### External launch action

Right-click Top/Isometric to copy `x,y`. Native builds also support a launch action configured once
at startup through `OPENCITY_LAUNCH_ACTION=/path/to/action.json`:

```json
{
  "label": "Inspect here",
  "program": "external-map-viewer",
  "args": ["--map", "{map}", "--position", "{x},{y}"]
}
```

Required: `label`, `program`, string-array `args`. Tokens: `{map}`, `{x}`, `{y}`, `{config_dir}`.
Arguments pass directly without a shell; relative executable paths use the editor's working
directory. `{map}` includes unsaved edits through a temporary snapshot beside the original file
(or startup directory for new maps), preserving asset paths; default elevation is exported too.
Snapshots disappear on process exit. Optional `ready_message` hides the spinner on a matching
stderr marker; otherwise it stays until exit. Failures show recent stderr. Browsers only copy
coordinates.

### Road shoulders and safety walls

`role highway;`, ramp/merge profiles and explicit `barrier` bands produce concrete barriers;
other roads gain edge barriers above 0.02 map units of terrain clearance. Protected roads/junctions
pave verge/median bands without changing widths or lane indices, leaving merge mouths open.
Editor, GLB and runtime share barrier geometry. `City::road_barriers` exposes it;
`City::road_barrier_contact` handles circle contacts with a vertical interval. Barriers are props,
not tire support: height 0.011, width 0.004 map units.
