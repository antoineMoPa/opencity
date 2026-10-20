# OpenCity

OpenCity is a Rust crate for reading and writing a small city definition language. It parses road cross sections, blocks of calls, and top level calls into a typed AST. The editor maps Bézier road placements and lets you edit reusable cross sections.

## Road editor

Run the native egui editor with `cargo run --features editor --bin editor`, or open a file immediately with `cargo run --features editor --bin editor -- path/to/city.road`.

The app starts on a pan and zoom map with 16 cross section types in the city, countryside, and interstate library. Hover a type on the left to preview its cross section. Choose a type, then click two points to place a road; every new segment is a Bézier that starts straight. Nearby junctions and the ends of individual cross section elements appear while drawing. Click one to attach the new road there. Zoom in to pick closely spaced elements. Keep clicking to extend the path; press Enter or Esc to finish. Select a segment to rename it at the top of the inspector, drag its endpoints or control handles, or press Backspace to delete it. Multiple named roads can share one cross section.

To make an exit or merge, select its parent road, choose a cross section element and position in the inspector, choose the new road's cross section (the library includes **Exit ramp**), then click **Start branch here** and place its endpoint. **Start end** and **Far end** snap the branch to an element's endpoint. A dedicated exit tapers the selected element out of the through road while the ramp grows from that lane; **Keep lane on through road** makes an optional exit instead. The choice can also be changed later from the ramp inspector. The branch follows its source element's position and direction when the parent is reshaped. Select the branch later to change its source element or position. Continued segments inherit the curve direction at their shared end, so their cross sections meet. The map draws cross sections as colored bands. Drag from empty map space or use right drag to pan, and scroll to zoom. **Theme** in the toolbar offers System, Light, and Dark; System follows your computer's appearance.

New road placements are calls with shared `map_point` junction identifiers and two numeric Bézier controls. A `lane_point` call attaches a junction to a named parent road, cross section element, and position along the curve; new attachments also store the source direction so the branch turns with the parent. A `lane_split(parent_placement, ramp_placement, dedicated|keep_lane)` call records the exit behavior for rendering. Existing lane attachments without a split call are treated as dedicated exits. Existing four-value straight calls and eight-value numeric Bézier calls remain readable and editable. Road calls inside one level of numerically placed blocks are visible but read only on the map.

Switch to **Cross section** to edit the selected road type's ordered elements, including `shoulder` and `verge`, or create a new type. Click an element to change its kind or optional `$n` capture, move it, or remove it. The inspector can delete a type after confirmation; calls that name it remain in the document. **File** offers New, Open, Save, and Save As; on macOS these commands are also in the system menu with Cmd+N, Cmd+O, Cmd+S, and Shift+Cmd+S. The source preview shows the serialized AST. New documents include the built-in library, and saving writes the whole document.

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
