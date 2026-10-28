# Building a correct OpenCity map from a reference image

Use this guide when translating an image such as [`target.png`](../target.png) into an editable
`.road` map such as [`examples/default_map.road`](../examples/default_map.road). Advance through
small, connected patches. Establish each intersection locally before extending its roads. The image
gives visual intent; the `.road` file must supply topology, permitted travel, grade, and precise
junction geometry.

## 1. Establish coordinates and road classes

1. Mark a few image landmarks, map them to world coordinates, and keep one scale and orientation for
   the whole scene. Trace road centerlines, not pavement edges.
2. Classify each route as a local street, collector, arterial, interstate, or one-way ramp. Reuse an
   existing cross section for roads that must meet. A road's `direction` and point order determine
   travel; for a bidirectional divided road, lanes before `middle` run in reverse and lanes after it
   run forward.
3. Mark intended junctions and bridge crossings before tracing continuous roads. Use named
   `map_point` references for connections. A visual overlap of two curves does not connect them.
   Share a point reference for an at-grade join; use different references at the same horizontal
   coordinate for an overpass.

## 2. Build each junction as a small local patch

For each junction, work in this order:

1. If an existing long segment reaches or crosses the patch, split it into short pieces before
   adding another road. Replace the span through the proposed junction with named endpoints at its
   edges. Keep the outer pieces' Bézier handles tangent to the original alignment. Never leave a
   full-length road underneath a new intersection surface.
2. Place an `intersection3` or `intersection4` with separate edge ports, then attach one short
   street stub to each open port. Use the same cross section at each matching edge. The library's
   [`three_way.road`](../road_library/three_way.road) and
   [`four_way.road`](../road_library/four_way.road) show the complete local patch, including its
   outward connection points.
3. Connect the stubs to the next nearby patch with short named segments. Keep Bézier handles aligned
   at ordinary seams. As soon as a new road approaches another road, stop and build that next local
   junction before extending farther.
4. Inspect the patch in Top and Isometric. Road bands must end at the intersection ports; two bands
   must not stack across the junction or overlap without an explicit merge. Fix the local geometry
   before moving to the next patch.

For a bridge, split the approaches into short grade pieces, leave the two crossing references
distinct, set `elevation` or `grade`, and check vertical clearance. A road passing over another has
no junction. A real at-grade crossing needs an intersection: crossing centerlines alone are not
navigable.

## 3. Build one interchange movement at a time

Build the central crossing and short through-highway pieces first. Add adjacent through pieces only
as each lane attachment or grade change is located; split any existing long piece before placing a
ramp beside it. A cloverleaf has eight separate turning movements: four inner loops for left turns
and four outer direct connectors for right turns. For each movement:

1. Choose an entry `lane_point(parent, lane_index, t)` on a lane whose travel direction matches the
   route. Create a one-way `ramp_mouth` leading away from that lane.
2. Draw the ramp as a chain of named Bézier pieces. Each piece starts at the previous piece's exact
   `map_point`; keep neighboring handles tangent. Give inner loops room outside both through
   carriageways. Direct connectors stay outside the loop area.
3. Finish with a one-way `ramp_mouth` ending at a merge `lane_point` on a lane whose travel
   direction matches the ramp. Approach the merge with a nearly parallel tangent. A merge is a lane
   attachment, never a four-way road junction.
4. Set continuous `grade` values along every piece. At both lane attachments, the ramp height must
   equal the parent road's height at `t`. Where a ramp crosses an unrelated road, separate their
   heights or reroute it.

The library's [`cloverleaf.road`](../road_library/cloverleaf.road) names its short through-road
pieces `i_ew_*` and `i_ns_*`, loop pieces `*_loop_*`, and direct connectors `*_connector*`. Build
each ramp port on a short `interstate_core` piece, keeping both highway shoulders continuous
through the interchange. Merge pavement opens the parent wall across the lane and shoulders.
Its east-west and north-south center points are separate; the latter deck is raised. The
[`flat`](../road_library/street_to_interstate_flat.road) and
[`elevated`](../road_library/street_to_interstate_elevated.road) street ramps demonstrate a
lane-specific entry and forward merge into an interstate. Copy the full entry/curve/mouth chain when
adapting one.

## 4. Validate each local piece, then the whole map

- After each patch, parse and project the file. Require `projection.unresolved == 0`; inspect any
  unresolved call before continuing.
- Run `check_design_rules`. Resolve unconnected ramp ends, same-grade crossings, car-lane
  discontinuities, and car paths crossing sidewalks, verges, medians, barriers, or other non-car
  bands. A lane path must meet the next car lane at the same point, height, and permitted travel
  direction. Check street profiles against their intersection edges in the editor's **Check design
  rules** view.
- For every ramp, follow references from entry to merge. Verify point order, one-way direction,
  parent lane travel, exact shared seam references, and equal heights at each seam. Use a paved
  `ramp_mouth` at each intentional edge crossing so the merge has a drivable surface. Check that an
  exit never feeds into opposing traffic.
- Inspect top and isometric views after every local join and again at the final target zoom. Look
  for long segments still running through an intersection, doubled pavement, sharp tangents, road
  bands that clip into other bands, floating or buried ramp mouths, and unintended at-grade
  contacts. Export a GLB and inspect it in the game at the actual camera, shader, and scale. Recheck
  after moving any junction or changing any profile.

The supplied library tests cover parse/round-trip, projection, junction ports, directed ramp paths,
and grade continuity. Use native `place(asset, id, x, y, quarter_turns)` and `join(id, port, id,
port)` calls to assemble the [shared patterns](../road_library/README.md) in a `.road` file. The
parser checks joined coordinates, cross sections, heights, tangents, and one-way travel before
expanding the calls. The language still uses illustrative fixed band widths; final engine
dimensions, clearance, lane tapers, pavement markings, collision geometry, and pixel alignment need
project-specific specifications and visual QA.

## 5. Declare connection semantics and check compiled geometry

Road names are labels. Declare `role ramp;` on ramp profiles and `role merge;` on paved
entry/merge profiles. The default is `role ordinary;`. A name such as `ramp_mouth` no longer grants
a merge exception. Both `role` and `direction` survive saving, template expansion, translation, and
rotation.

```text
road connector:
    verge carlane verge;
    direction forward;
    role merge;
end
```

Carry `verge carlane verge` continuously through ramp mouths so shoulders do not taper away at
the highway entrance. Merge verges are paved; sidewalks and barriers are invalid merge bands.

A merge must have an explicit `lane_point` attachment to a parent **car lane**. Its other end
connects to the ramp with a shared point reference. Match elevation and travel direction. Changing a
label or declaring a merge cannot authorize a crossing of an unrelated road or an explicit barrier.

The shared Rust `road_geometry::RoadGeometry` compiler subdivides Bézier centerlines, builds
full-width lane triangles, and clips elevated sidewalls against the attached merge/parent pavement
surfaces. It opens both sides of the connection: the parent wall across the mouth and the mouth wall
inside the parent pavement. The editor and GLB exporter consume these same wall pieces. Never add
camera-dependent wall hiding or enlarge a proximity radius to repair an obstruction.

Run `check_design_rules` after every geometry or profile edit. Treat **all** issues as failures
before adding a template to the library:

- `invalid_merge_connection`: supply a paved merge profile and an explicit parent car-lane
  attachment.
- `vehicle_path_obstructed`: a remaining elevated wall enters another lane's clearance corridor. Fix
  topology or provide enough vertical separation; unrelated walls are not automatically removed.
- `vehicle_lane_crosses_non_car_band`: the lane's full width overlaps a forbidden band. A clear
  centerline is insufficient. Explicit barriers are never exempted by merge or junction
  declarations.
- `geometry_unresolved`: subdivision exhausted its budget; simplify the curve. Do not accept an
  unresolved result as collision-free.

Run `cargo test --test road_library --test road_geometry` and the editor tests after library
changes. Tests cover the gallery, all quarter-turns of the elevated ramp and cloverleaf, renamed
merge profiles, narrow off-center barriers, unrelated walls, and separated overpasses. Keep negative
examples: tests that only validate good templates cannot establish that the checker detects
obstructions.

Current tolerances use illustrative world units: centerline flatness `0.0005`, maximum subdivision
control-polygon length `0.04`, merge height agreement `0.012`, wall depth `0.08`, and a `0.04`
vertical vehicle reserve. The cloverleaf connector peaks are `0.65` to clear the loop corridors
including their sidewalls. These checks operate on a tessellated authoring model using f64
predicates; they are not an exact-arithmetic proof for continuous offset curves, a vehicle-specific
turning envelope, or arbitrary buildings/terrain. The existing junction pavement exception is
retained. Continue visual and engine collision QA, and specify real vehicle dimensions before
treating a map as physically certified.

The older `examples/starter_city.road`, `examples/cloverleaf_town.road`, and target-map fixtures
predate these contracts and can report existing invalid merge profiles and obstructions. Their
editor tests retain their original topology scope; they are not substitutes for the strict library
acceptance tests. Do not copy their merge profiles into new library assets.

## 6. Verify rendered merge surfaces and vehicular controls

Geometry checks alone do not establish that the rendered road is usable. Capture the actual GPU
output in Top, Isometric, and Vehicular views. In particular, verify that an intentional merge's
pavement renders over its parent's verge and outline; removing a wall does not remove an incorrectly
ordered pavement overlay. Preserve perspective depth ordering for deliberate paint layers. The
renderer uses a floating-point reversed-Z depth buffer for stable perspective precision, and merge
paint has explicit depth priority without raising its physical surface.

GPU regressions in scene3d.rs check merge pixels and coplanar layer ordering, including genuinely
nearer occluders. Run `OPENCITY_QA_LIBRARY=1 cargo test --bin editor render_starter_scene_pngs --
--ignored --nocapture` with graphics access to generate template QA PNGs under `/private/tmp`.
Inspect the images; do not infer visual correctness from geometry tests alone.

In vehicular input handling, read key state inside `ctx.input`/`ui.input`, then request repaint
after the closure returns. Requesting repaint while holding the input lock can deadlock. Movement
should update only camera state and reuse the scene; the simulated W-key regression checks this.

Ordinary road pavement and sidewalls now use shared `road_profiles` alignment/taper rules and the
same sampled `RoadGeometry::cross_sections` vertices, including grade. GLB bands consume those rows
too. Do not independently reconstruct full-width walls beside a tapered road, or infer surface
heights by projecting a mesh vertex onto a coarse centerline: both produced detached fins and jagged
wall/pavement seams. Regression tests cover all four cloverleaf connector-to-merge seams and require
rendered wall tops to be actual pavement vertices. Specialized rail/airfield decoration painters and
junction surfaces still need the broader unification tracked on the follow-up card.

## 7. Edit roads in the minimal 3D editor

The editor is one full-window viewport, initially Isometric. Open the palette and choose **Add road:
<profile>**; click twice on the ground to place one road. The tool returns to selection after
placement. Escape cancels an unfinished road. New road types and map construction belong in `.road`
source.

Click a placed road to select it. Circular handles move free endpoints; square handles shape its
Bézier curve. Diamond handles mark lane-attached ends: drag along the parent road to choose a
compatible car lane and location. Hover cues identify each handle. The preview follows actual lane
geometry and grade; shared references and matching attachment heights remain shared. Opposite
endpoint heights and unrelated decks stay unchanged. A drag is one undo operation. Intersection
ports remain protected.

The selected editable placement's name appears in a compact field. Enter or leaving the field
commits one rename; Escape restores the original text. Names use letters, digits and underscores and
start with a letter or underscore. Invalid or occupied names leave the document unchanged. This
renames the placed road, including its dependent references, rather than the global road type.
Block-instance roads are read-only. External source edits refresh the field while it is idle.

Drag empty space to pan; Ctrl+left drag orbits, middle drag pans, and the wheel zooms. Ctrl+Up/Down
adjusts elevation. Keyboard map gestures stop while editing text. Cmd/Ctrl+O opens, Cmd/Ctrl+S
saves, Shift+Cmd/Ctrl+S saves as, Cmd/Ctrl+Z undoes and Shift+Cmd/Ctrl+Z redoes. Open resets
history. Clear (Cmd/Ctrl+N) always asks for confirmation, removes placements while retaining
definitions and the current file path, and supports one-step undo. Cancel leaves placement drafts
untouched.

Command-Shift-P (Ctrl-Shift-P elsewhere) opens the Moon palette for file actions, GLB/SVG exports,
undo/redo, copy/paste, each existing road profile, all landscape kinds, Annotation segment,
Annotation text, Select, Rename selected road and theme switching. Landscapes use click points and
Enter (three points minimum, two for rivers); Escape cancels. Red annotations are nonphysical
overlays: two clicks for a segment; click an anchor and type for text, Enter commits, Escape or an
outside click cancels. Empty text is discarded. All committed features support undo/redo and
save/reopen. Annotation editing uses source/API or undo; there are no annotation drag handles yet.
Palette input cannot edit the canvas underneath. Native macOS OpenCity (About/Quit) and File
(Clear/Open/Save/Save As) menus are restored. The cross-section editor is retained as a dedicated
full-window mode: choose Edit cross section, edit the selected placement’s profile or choose another
type, then Back to map. Profile names differ from placement names; edits apply to all placements of
that profile and share undo/save history. Dock panels, tabs, branch popups, cut tools and the
connection workspace have been removed. The viewport renders the complete map; it does not isolate
selected connections. Numeric construction, profiles, branches, heights and design checks remain
available through `.road` and the Rust API.

## 8. Inspect and edit connections through the Rust API

`get_connection_workspace {}` returns named merges and intersections, member roads, ports,
attachments and asynchronous design-check status. `open_connection_editor
{"connection_id":"merge:ramp"}` is a compatibility command that selects and focuses that connection
in the full isometric viewport. It does not open a panel or enter a filtered workspace; its legacy
`camera` argument is accepted, and the compatibility command focuses Isometric. The unused
`close_connection_editor` and `cut_connection_segment` tools have been removed.

`edit_connection` requires `connection_id` and `edit`. For `start`, `handle1`, `handle2`, or `end`,
supply `segment`, `x`, `y`. For `grade`, supply `segment`, `start_height`, `end_height`. For
`attachment`, supply `reference`, `lane`, `t`. For `intersection`, supply `x`, `y`, `kind`
(`right_turn`, `three_way`, `four_way`), and `profile`. These are validated, undoable AST edits;
they do not repair pavement, grade or clearance defects automatically. Run the complete design
checker after editing. Block instances and protected endpoints reject unsupported mutations.

Use `get_viewport_image {"include_ui":true}` to inspect the entire editor window, including the
selected road name field. The default `false` captures the map rectangle. Check `design_checks`
before interpreting cached issues; `pending` does not mean geometry is clear. Connection IDs use
assigned names; refresh after document replacement or renaming. Connection queries and edits operate
on the full document; the compatibility `workspace` field is null.

The palette prioritizes Add road (neighborhood_street), Add highway (divided_highway), Add
interstate (interstate_three_lane), Add terrain and Add concrete. Missing canonical road definitions
are materialized only on commit; existing same-named definitions are respected. Hover the top 100
logical pixels to reveal the Moon Top/Isometric/Vehicular switch at the top right. It hides when
leaving that strip, retaining an active switch gesture through release. The rest of the strip
remains usable canvas. Its labels have 10px horizontal and 5px vertical padding. It preserves the
palette query; palette Tab also cycles views. View changes animate over 320ms around the current
geographic focus and scale; Vehicular entry stays near that view location instead of jumping to a
remote selection. Navigation interrupts the transition, and geometry editing pauses during
intermediate projections. Vehicular Left/Right turn at 1.5 radians/second; W/S and A/D translate at
0.3 world units/second and scrolling at 0.0003 units/pixel. Box select performs one primary-button
marquee, then returns to Select; Escape cancels without a map edit. Placement/selection tools opened
from Vehicular return to the last editing view. Red annotation placement works in Top/Isometric;
existing markup is projected correctly in Vehicular.

Terrain-aware placement chooses visible terrain surfaces in Top/Isometric and checks the complete
shared compiled road surface, including generated branch geometry. A free route can receive a
uniform upward grade shift preserving its slope; it can therefore float over lower terrain patches.
It is conservative clearance, not a terrain-following engineering workflow. Existing anchored routes
reject atomically when terrain blocks them. Failure preserves the first-point preview and explains
the rejection; Escape cancels all draft references/profile changes, while a valid commit records one
undoable transaction.

The palette also provides **Add three-way intersection**, **Add four-way intersection** and **Add
right-turn intersection** immediately after the five frequent placement actions. Search
`intersection`, `junction`, `tee` or `cross`. In Top or Isometric, move the pointer to preview the
footprint and open ports, press R to rotate 90°, click once to place, or Escape to cancel. The
selected road profile supplies the cross section; if none is selected, the canonical neighborhood
street profile is inserted only when placement commits. Each placement is one undoable `.road` edit
with native named center/edge references. Extend roads outward from the matching ports. Standalone
intersections currently require ground level: a red preview rejects any footprint touching raised
terrain. Starting from Vehicular restores the previous editing view through the normal
focus-preserving camera transition.
A standalone intersection also requires a clear site: overlap with existing road surfaces or
junction bodies is rejected atomically. Use explicit source/API splits when converting an existing
road into a junction; the palette action does not infer that topology.
