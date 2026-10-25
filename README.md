# OpenCity

OpenCity is a Rust parser and visual editor for `.road` city maps. It opens on the editable [target map](examples/target_map.road).

Road files define reusable cross sections and place them on a map:

```text
road street:
    sidewalk carlane middle carlane sidewalk;
end
street(0, 0, 10, 0);
```

Reusable blocks accept typed object parameters: `block span({ length }: { length: number }): street(0, 0, length, 0); end`. Place one with `span(10, 20, { length: 5 });`. See [the language guide](docs.md#reusable-blocks-with-typed-parameters).

Clone with submodules: `git clone --recurse-submodules https://github.com/antoineMoPa/opencity.git`.

```sh
cargo run   # native editor
make build
make serve  # web editor
```

See [docs.md](docs.md) for the language, editor controls, and API.
The editor exposes its live [MCP and Unix socket API](docs/editor-api.md), including a wire protocol usable by external Python clients.
Reusable junction, ramp, and cloverleaf assets live in the [road pattern library](road_library/README.md); the [agent road-building guide](docs/road-building-guide-for-agents.md) explains how to trace a reference map without impossible joins.

Explicit intersection definitions describe ports, geometric paths, allowed movements, surfaces, and markings. Click an intersection on the map and choose **Open intersection viewer** for read-only inspection. See [the intersection guide](docs/intersections.md) and [the example gallery](examples/intersections.road).

Export a component without opening the editor:

```sh
cargo run --bin opencity -- intersection-svg examples/intersections.road --name crossroads --output /tmp/crossroads.svg
cargo run --bin opencity -- intersection-png examples/intersections.road --name crossroads --output /tmp/crossroads.png
cargo run --bin opencity -- --help
```
