# Editor architecture

The application has two full-window modes: the map viewport and the cross-section editor. Both mutate the same Rust `Program`, share file actions and undo history, and save ordinary `.road` source. Dock layouts, library/inspector panes, old in-window menus, template galleries and focused connection workspaces have been removed.

| Responsibility | Source |
| --- | --- |
| Shared state, native/web entry and module wiring | `src/bin/editor.rs` |
| Initialization and frame/input dispatch | `initialize.rs`, `frame.rs` |
| Palette commands, document lifecycle, history and dialogs | `commands.rs`, `document.rs`, `history.rs`, `dialogs.rs` |
| Map shell, selected placement rename and hover view switch | `minimal.rs` |
| Full-window cross-section controls and profile AST edits | `section_ui.rs`, `section.rs` |
| Top, Isometric and Vehicular input/render callbacks | `viewport_top.rs`, `viewport_iso.rs`, `viewport_vehicle.rs` |
| Camera focus, projection transitions and transforms | `camera.rs`, `camera_transition.rs`, `coordinates.rs` |
| Picking, selection and anchored road-point options | `picking.rs`, `picking_iso.rs`, `selection.rs`, `point_options.rs` |
| Standalone intersection preview, placement and validation | `intersection_placement.rs` |
| Road/intersection painting and markings | `road_paint.rs`, `intersection_paint.rs`, `markings.rs`, `appearance.rs` |
| Terrain-aware road placement, landscape and annotations | `terrain_placement.rs`, `landscape.rs`, `annotations.rs` |
| Geometry caching and asynchronous API design analysis | `scene_cache.rs` |
| Socket/MCP tools and stateless connection precision edits | `mcp.rs`, `connection.rs` |
| AST projection, references, attachment edits and topology | `map.rs` facade and `map/` modules |
| Scene builders, geometry, landscape/model cache and GPU rendering | `scene3d.rs` facade and `scene3d/` modules |
| GLB/SVG export and GLB parsing | `glb.rs`, `svg.rs`, `model.rs` |

Paths after the root entry refer to `src/bin/editor/`. The map facade is also the public library map module. Native source-referenced GLB models retain automatic loading; the browser renders building extrusion when a model is unavailable. The depth-tested renderer uses shared road geometry and no longer builds filtered connection scenes. Native connection services retain stable IDs and validated point/control/attachment/grade/intersection edits over the complete document; they retain no visual workspace or cut-session state.

## Camera and overlay behavior

The Moon view switch appears at the top right while hovering the top 100 logical pixels. Its labels use 10px horizontal and 5px vertical padding without changing vendor defaults. A switch gesture retains ownership through release; the remainder of the reveal strip remains canvas.

View changes animate the actual GPU projection over 320ms, preserving the current geographic focus and scale. Vehicular entry chooses a nearby traffic lane around the current view rather than a distant selection. Returning to an overview uses the latest vehicle location. Navigation cancels an in-flight transition; geometry editing pauses during intermediate projections so picking cannot use the wrong camera.

Spatial handles and the selected-road rename UI use the Middle layer, below the Foreground command palette. Anchored point options preserve click-versus-drag ownership: road endpoints offer extension, tangent rotation and cross-section editing; car-lane ends offer branching, while other band ends offer cross-section editing. Screen-distance picking retains exact lane identity at low zoom. Extension and branch starts use held placement drafts; Escape, outside clicks, dragging and palette/modal entry dismiss options without introducing source/history mutations.

## Placement and cross sections

The palette offers standalone three-way, four-way and right-turn intersections in Top and Isometric. `intersection_placement.rs` builds previews through native map port/rotation APIs; R rotates the draft. A valid click commits the lazy profile, named center, edge ports and intersection call as one undoable transaction. Cancellation adds nothing. Native standalone ports are ground-level: placement rejects positive-terrain, existing road-surface and junction-body overlap. It does not automatically split or connect an underlying road; connections use the new named edge ports.

Cross-section mode starts from the selected placement's profile and includes an existing-profile picker, profile name/direction, band add/change/reorder/remove, capture parameters, new/delete profile, source display and save. Profile edits affect all placements of that type; reorder/removal updates attachments and preserves detached geometry. Its responsive body uses 24px margins and grouped controls with one heading. Back to map returns to the viewport. The map's name field edits a placement name, independently of the profile name.

Clear uses a modal with a maximum width of 440px, 24px padding, right-aligned actions and Cancel as the default. Confirmation preserves definitions, file path and saved baseline and records one undoable map edit; cancellation preserves the current draft.

## Source boundaries and verification

Tests live beside their responsibilities: document/geometry/camera regressions in `editor/tests/`, full-frame UI regressions in `minimal/tests/`, and focused map, renderer, connection and terrain modules. Child test paths are explicit because the facade is loaded through `#[path]` from both the native binary and browser library. Point options have three full-frame pointer regressions covering targeting, completion/undo, cancellation and drag separation. Section and modal layout checks inspect actual egui painted bounds. Camera projection interpolation has actual Metal GPU readback coverage; the junction seam gallery was visually inspected. These checks are distinct from a native application screenshot review.

`tests/source_size.rs` recursively checks handwritten `.rs` files under `src/` and `tests/`, plus root `build.rs` if present. The hard cap is 850 physical lines, including comments, blanks and tests. Substantial modules target 450–850 lines; small entrypoints/helpers are not padded. Vendored crates, data and configuration are outside this source check.

Explicit intersection components live in `src/network/`: typed geometry and movement data, dependency resolution, reusable placement, default components, and SVG generation. `src/parser/network.rs` handles their brace grammar. `src/cli.rs` supplies headless SVG/PNG export commands to both binaries; the `intersection-export` feature uses the existing resvg dependency for rasterization.

`editor/network_ui.rs` owns read-only inspection state and the map-click option. Opening the viewer suspends the normal map command path; its layer and movement selections never mutate `Program`. `road_library/default_four_way.road` is the source for default four-way curves and stop paint, shared by the map, viewer, and exports. Ordinary approaches use `RoadProfile` boundary parameters from `road_profiles/intersections.rs`, so their shared `RoadGeometry` pavement, markings, sidewalls, and selection line stop outside the junction rather than overlap its surface.
