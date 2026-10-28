# Use OpenCity in a game

Add the repository as a git submodule and select the renderer-independent runtime feature:

```sh
git submodule add git@github.com:antoineMoPa/opencity.git opencity
git submodule update --init --recursive
```

```toml
[dependencies]
opencity = { path = "opencity", default-features = false, features = ["game-runtime"] }
```

This feature adds polygon tessellation but brings in no editor, windowing, GUI or graphics runtime.
Application behavior belongs in your Rust game; map construction remains in `.road` files.

```rust,no_run
use opencity::runtime::{City, Region};
let city = City::load("opencity/examples/default_map.road")?;
let nearby = city.query(Region::radius([-24.0, 3.0], 0.75)?);
for (id, road) in nearby.roads {
    println!("{id}: {:?}", road.segment.direction);
    for lane in 0..road.definition.elements.len() {
        println!("{:?}", road.definition.lane_direction(lane));
    }
}
let all = city.query(Region::WholeMap);
println!("{} buildings, {} routes", all.buildings.len(), all.paths.len());
let svg = city.minimap_svg()?; // North-up terrain, building footprints and vector roads.
let support = city.surface_at([-24.0, 3.0], 0.3);
let mesh = city.mesh(Region::radius([-24.0, 3.0], 2.0)?, 0.04)?;
let furniture = city.detail_mesh(Region::radius([-24.0, 3.0], 0.8)?);
# Ok::<(), String>(())
```

Coordinates are **X east, Y north, Z up**, in authored map units. The library assigns no physical
length; each application chooses its own conversion from map units to meters. Radius queries are
horizontal circles in the map's XY plane, including objects at every elevation. Use surface ceilings
or inspect feature heights when selecting a particular deck.

`City::load` resolves the elevation PNG relative to the `.road` file. `City::from_program` accepts
an optional asset base directory. Compilation rejects unresolved road placements, invalid landscape
and network projections and failed geometry subdivision. A compiled City is a snapshot; reload it
after source edits. Collection indices are IDs valid for that snapshot. Treat published records as
read-only: the compiled spatial index and meshes are not rebuilt after direct collection changes.

`query` returns structured Rust references: road placements with exact cubic controls, start/end
heights, endpoint references, cross-section definitions, lane directions and bounds; component/road
route polylines; building footprints, heights, facade/material options and optional GLB paths;
landscape features; native junctions and ports. `City::components` retains explicit component ports,
paths, bands and permitted movements. Radius selection tests road/shoulder triangles, polygon
containment/edges and path segments, rather than just object centers. Junction selection uses
conservative footprint bounds. Invalid directly-constructed regions produce empty selections; prefer
the validated `Region::radius` constructor. These are typed library APIs, without a JSON or socket
requirement.

`surface_at(point, ceiling)` selects the highest physical support **at or below the ceiling** and
returns height, XY gradient, surface kind and optional road ID. Supply the current tire height plus
a small step allowance, rather than infinity, to stay under bridges. Buildings and decorative
markings are excluded from tire support. `building_collision` checks a horizontal circle against
extruded building footprints and a vertical body interval. Surface kinds expose prototype dry
friction and rolling-resistance values that games may replace. Water marks flooded terrain as low
grip; the initial prototype does not simulate buoyancy.

Physical road heights follow authored grades. Unlike the editor's rendering-only pavement offset,
runtime pavement adds no artificial elevation. Sidewalks rise 0.0015 units; short grass shoulders
connect ordinary dry road edges to nearby terrain, preserving open space below bridges. Elevated
road placements gain concrete slabs (0.0065 units thick), undersides, fascia, columns, crossheads
and footings. Slabs use the exact adaptive surface rows; piers are spaced about 0.24 units apart and
skip lower roads, sidewalks and buildings. Ground clearance above 0.02 units selects elevated spans.
These structures use `SurfaceKind::Prop`, so they render and cast shadows while leaving existing
support and building-collision queries unchanged. Native four-way/two-way junction meshes reuse the
shared profile geometry, with connected seam heights. Runtime composition clips approach strips to
the junction footprint and subtracts covered fills so roads and medians have no coplanar duplicate
surfaces; tees currently use a paved footprint and closed-side sidewalk. Landscape ground-cover
elevation follows the editor's -0.02 convention. Building geometry is procedural footprint
extrusion; attached GLB paths are metadata for consumers to load if needed.

`mesh` returns packed triangle-list vertices with material metadata. Near meshes use original
elevation-map sample boundaries and diagonals; `terrain_step` selects an integer sample stride, with
steps below a pixel keeping full resolution. Coarser strides approximate terrain for distant LOD.
Request bounded neighborhoods and cache the resulting GPU buffers; whole-map meshes may be large.
`minimap_svg` exports a north-up SVG with vector roads/component geometry, footprints and an
embedded georeferenced terrain PNG. Hermes parses it once and crops cached neighborhoods for the
HUD. SVG Y reflects world Y; the PNG explicitly stretches to its authored map rectangle.

`street_features` exposes deterministic lamp/bench/tree placements; `detail_mesh` builds simple
geometry for them. Furniture is anchored to authored sidewalk bands and avoids building footprints.
`detail_objects` returns one mesh per placement for applications that own collision and destruction.
OpenCity does not assign collision or point-light behavior to these meshes.

`building_detail_mesh(region, config)` adds optional low-rise facade and frontage details to the
existing footprint extrusions. `BuildingDetailConfig` receives the application seed, map-unit scale,
height and frontage limits, road-search distance, garden chance and weighted patterns. A
`BuildingPattern` explicitly chooses floor count, facade, roof, stair shape and colors. Independent
weighted `GardenEdge` and `GardenProp` choices control optional visual walls, fences, hedges and
objects. Applications supply the mix: the library does not assign geographic or cultural styles.
Patterns support one through four storeys. Where the stair footprint reaches pavement, the runtime
uses an internal-entry facade and omits the exterior stair.

In addition to the original materials, building calls accept `red_brick`, `brown_brick`,
`buff_brick`, `pale_brick` and `limestone`. The game shader renders their masonry as a surface
texture and leaves openings to explicit detail geometry; no individual bricks or stones are modeled.
These are generic material choices and applications decide where to use them. Masonry uses separate
mineral-colored mortar and deterministic per-unit clay variation. The shader filters joint coverage
over the pixel footprint, preserving mortar color at a distance without adding geometry. Limestone
uses larger courses and a cooler, darker joint palette.

The seed derives from footprint coordinates, so a given feature is consistent between whole-map and
radius queries. The street-facing edge is selected from nearby road geometry; without a nearby road,
the longest edge is used. Windows, trim, balconies, straight/curved/quarter-turn stairs, coping,
cornices, parapets and eligible pitched/mansard roofs use coarse geometry. Masonry and shingle
detail is a shader texture, not separate bricks or rocks. Pitched roofs fall back to coping on
irregular footprints. GLB-authored buildings are skipped. Garden features are omitted when the
sampled frontage is obstructed or paved. Details do not modify footprint collisions, support queries
or authored map records; users can implement those behaviors separately. `building_detail_parts`
separates facade geometry from independently removable balconies, gallery structures, stairs, garden
boundary sections and garden objects. Its two outputs contain the same geometry as
`building_detail_mesh` and carry no mutable physics state. Iron fences, chain-link fences and
balcony guards use two-triangle planes. Each vertex carries local panel coordinates, so the
procedural line pattern follows translated or rotated props. `FencePattern::IronPickets` and
`FencePattern::ChainLink` select the mask. Openings are cut out in both color and shadow passes;
custom renderers must implement the same material contract. Garden fences include side returns where
terrain and building clearance allow them; attached buildings share one divider. Each section
remains an independently removable object.

## Optional shaders

Exported shader strings are optional; the library creates no GPU resources. `runtime::EDITOR_SHADER`
is the original editor shader, including its camera, coast elevation texture and facade metadata
contract. See `src/bin/editor/shaders/scene.wgsl` and `scene3d/renderer.rs` for its bindings. It
uses editor-specific depth bias and camera conventions; it is not interchangeable with the game
shader.

`runtime::GAME_SHADER` is a standalone Z-up shader for consuming applications. Its vertex stride is
52 bytes, with the following float attributes:

| Location | Field                                                  | Format    |
| -------- | ------------------------------------------------------ | --------- |
| 0        | world position                                         | Float32x3 |
| 1        | world normal                                           | Float32x3 |
| 2        | linear RGB                                             | Float32x3 |
| 3        | surface kind, base elevation, floor spacing, facade ID | Float32x4 |

For `SurfaceKind::Fence`, location 3 instead stores
`[11, horizontal pattern distance, vertical fraction 0..1, FencePattern]`. Horizontal distance is
expressed in the consumer's `BuildingDetailConfig::unit`; pattern IDs are 0 for iron and 1 for
chain-link. The packed vertex stride is unchanged. Render planes double-sided, with 4x MSAA and
alpha-to-coverage enabled in the scene pipeline to soften line edges without sorting transparent
objects.

Group 0 binding 0 is a 176-byte uniform: column-major view-projection matrix, column-major light
view-projection matrix, eye XYZ + time seconds, sun XYZ + exposure, daylight (0–1) + sky RGB.
Matrices use wgpu's 0..1 clip depth. Group 1 binding 0 is a 2048² depth texture, binding 1 a
comparison sampler. `vs_shadow`/`fs_shadow` write sunlight depth and discard fence gaps; `vs`/`fs`
render the scene to an sRGB target. Create a depth-only shadow pipeline with a fragment stage, no
color targets and just group 0, and the scene pipeline with both groups. The shader provides
procedural surface noise, facade details, animated water, PCF sunlight shadows, fog and exposure
compression. `SurfaceKind::Window` supports five-face parallax interiors with seeded lighting and
edge AO. Detailed panes store `[Window, U, V, 16 * (1 + seed % 256) + aspect]`, with aspect clamped
to 0.125..15. UVs interpolate across the pane, U along `(normal.y, -normal.x, 0)` and V upward; the
packed seed/aspect stays flat. The vertex layout is unchanged. Pane red values below 0.05 select an
unoccupied room. Zero metadata keeps legacy glass glow. Procedural facade windows use the same
five-face rooms. `SurfaceKind::Light` glows at night; `SurfaceKind::Roof` stores its texture unit in
`surface.y`. Games can replace the shader or bind an all-lit depth texture to omit shadowing.

Validate the library with `cargo test --no-default-features --features game-runtime`. Runtime
integration tests cover structured selection, one-way lanes, polygon containment, stacked
roads/grades, support/mesh consistency, footprint collision, deterministic details and
loading/streaming the complete default map. The consumer adds vehicle behavior, WGSL validation and
actual GPU smoke rendering.
