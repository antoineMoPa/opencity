# OpenCity

OpenCity is a Rust crate for reading and writing a small city definition language. It currently parses road cross sections, blocks of calls, and top level calls into a typed AST. It does not yet generate a road network or draw one.

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

Road elements are `busstop`, `sidewalk`, `parkinglane`, `bikelane`, `buslane`, `middle`, `carlane`, and `buildingedge`. A road element may capture a numeric parameter such as `$1`. Calls accept numeric literals or identifiers and may assign results to names. A semicolon after `end` is optional when reading and always emitted when writing.

Run the Rust checks with `cargo test`.
