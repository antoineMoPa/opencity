# Road network expansion

Open the command palette and choose **Expand road network…** in the editor or native game.
Pick a gold open-end handle in the map picker or world view. Dashed ghosts show the available
extensions; the selected ghost is brighter. Click a ghost or its label to select it, press
Tab/Shift-Tab to cycle, then Enter or **Apply selected extension** to accept. Close the window
to leave the network unchanged. Length, curve angle, grade, and element type control the next
proposals.

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
Expand **Auto-growth experiment**, choose a seed and step count, and press **Grow**. Growth is
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
the shared `expansion_ui::Picker`; callers retain ownership of saving and rendering.
