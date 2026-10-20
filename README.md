# OpenCity

OpenCity is a Rust crate for reading and writing a small city definition language. It parses road cross sections, blocks of calls, and top level calls into a typed AST. The editor maps straight road placements with numeric endpoints and lets you edit their cross sections.

## Road editor

Run the native egui editor with `cargo run --features editor --bin editor`, or open a file immediately with `cargo run --features editor --bin editor -- path/to/city.road`.

The app starts on a pan and zoom map with 15 road types in the city, countryside, and interstate library. A neighborhood street is ready to draw; choose another type on the left or click two points on the map to place a straight segment. Keep clicking to extend from the last endpoint; press Esc to stop drawing. Select a segment to drag its endpoints or enter coordinates in the inspector. Drag from empty map space or use right drag to pan, and scroll to zoom. The map stores new segments as top level road calls with four numeric endpoint arguments. It also draws road calls inside one level of numerically placed blocks; those block instances are read only in the map.

Switch to **Cross section** to edit the selected road type's ordered elements, including `shoulder` and `verge`, or create a new type. Click an element to change its kind or optional `$n` capture, move it, or remove it. The inspector can delete a type after confirmation; calls that name it remain in the document. **File** offers New, Open, Save, and Save As; on macOS these commands are also in the system menu with ⌘N, ⌘O, ⌘S, and ⇧⌘S. The source preview shows the serialized AST. New documents include the built-in library, and saving writes the whole document.

The map convention covers straight segments with numeric coordinates. Curves, lane widths, and deeper block evaluation are not represented by the current language.

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
