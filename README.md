# OpenCity

OpenCity is a Rust parser and visual editor for `.road` city maps. It opens on the editable [target map](examples/target_map.road).

Road files define reusable cross sections and place them on a map:

```text
road street:
    sidewalk-carlane-middle-carlane-sidewalk;
end;
street(0, 0, 10, 0);
```

Clone with submodules: `git clone --recurse-submodules https://github.com/antoineMoPa/opencity.git`.

```sh
cargo run   # native editor
make build
make serve  # web editor
```

See [docs.md](docs.md) for the language, editor controls, and API.
The editor exposes its live [MCP and Unix socket API](docs/editor-api.md), including a wire protocol usable by external Python clients.
Reusable junction, ramp, and cloverleaf assets live in the [road pattern library](road_library/README.md); the [agent road-building guide](docs/road-building-guide-for-agents.md) explains how to trace a reference map without impossible joins.
