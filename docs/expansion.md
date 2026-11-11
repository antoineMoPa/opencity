# Road network expansion

Open the command palette and choose **Expand road network…** in the editor or native game.
Pick a gold open-end handle in the world view, or use the endpoint arrows in the bottom icon
strip. Dashed ghosts show the available extensions; the selected ghost is brighter. Choose an
icon or click a ghost, then press Enter or the checkmark to place it. Tab/Shift-Tab cycles the
icons when a field or button does not own keyboard focus. Escape or the close icon exits.

The strip reserves 42 logical pixels below the canvas. Both the native game's camera and HUD
and the editor's canvas use the remaining viewport; controls never cover the scene. Narrow
windows scroll the strip's tools horizontally while placement and close remain accessible.
Length appears with map units (u). Curves expose angle, slopes expose grade as a percent, and
cross-section changes expose an element selector. Tooltips identify icons and report save
status or errors without a permanent instruction panel.

After placing a section, its new open end is selected with Continue ready. Curves and hills
carry their heading and height forward; branches and lane splits select the new branch tip.
The editor moves to that end while keeping the current view and zoom. The native game uses
an elevated building camera facing the next section, without moving the player or car.
Closing the strip returns to the driving camera. Closing a road end or failing a save does
not move the camera.

The alternatives cover ending a road, continuing its cross section, branching to either side,
curving to either side, going up/down, starting/ending a cross-section element, and splitting a
lane into an exit.
An end at the start of a placement preserves the original cross-section order and travel
permissions. Dedicated lane exits currently require a placement's ending end and two travel
lanes. Unavailable alternatives are omitted. Existing block placements remain reusable and
are not rewritten; their geometry still participates in clash pruning. Unused placed
intersection ports can grow their own approaches.

Editor changes use normal Undo/Redo and Save. The native game saves each accepted extension
to its loaded `.road` file and applies the result through normal map reload. Existing comments
and untouched source formatting are preserved. A preview cannot replace a changed document;
a file modified externally must be reopened in the picker before saving. Browser game editing
is unavailable because bundled maps do not have a writable source file.

`expansion_stop(point)` marks an intentionally closed end. Other accepted alternatives use
ordinary road definitions, Bézier placements, endpoint references, grades, and lane splits.
The stop marker survives saving and reopening, and does not add road geometry. Remove that
call from the source to offer proposals at the end again.

## Auto-growth experiment

Open `examples/expansion-seed.road`: it contains just one intersection with four unused ports.
Choose **Auto-grow road network…** in the command palette, set a seed and step count in the same
bottom strip, and press the checkmark. The back arrow returns to extensions. Growth is
bounded and repeatable for a given source, seed, and length. Every step regenerates open ends
and prunes clashing proposals against the updated map. In the editor, one Undo reverses the
whole batch. In the game, the batch is saved to the current source file.

The generator uses finite alternatives rather than an unconstrained drawing tool. Pruning
checks road widths and vertical separation, building footprints/heights, water, terrain walls,
and explicit component surface bands. Curves are sampled along their interiors, with bounds checks
before
detailed tests. The immediate shared road join is allowed. This is conservative proposal
filtering, not a general geometry validator; it does not certify terrain support or construct
bridge structures for an elevated proposal.

## Rust API

`expansion::ends` discovers open editable road ends and intersection ports.
`expansion::possibilities` returns default proposals; `possibilities_with_shape` accepts curve
angle and grade as well as length. `Proposal::ghosts` contains the preview geometry, and
`Proposal::apply` accepts it only against its original document snapshot.
`expansion::propose` accepts typed options, including the cross-section element to change.
`expansion::grow` applies a bounded deterministic batch. `expansion::edited_source` produces
source-preserving text edits for native consumers. The optional `expansion-ui` feature exposes
the shared `expansion_ui::Picker`; callers retain ownership of saving and rendering. Call
`Picker::show` on the root UI before allocating the canvas, and use `Picker::viewport_rect`
for external renderers. Pass the matching viewport rectangle to `world_overlay` for clipping
and picking. `start` and `start_growth` open the normal and experimental strip modes.
`take_follow_target` consumes the position, heading and length of a newly accepted tip. Native
consumers should apply that camera target only after saving the edit successfully.
