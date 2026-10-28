# OpenCity

[comment]: <> (note for agents: add stuff to docs.md instead of here. keep readme short)

Road files define reusable cross sections and place them on a map:

```text
road street:
    sidewalk carlane middle carlane sidewalk;
end
street(0, 0, 10, 0);
```

Reusable blocks accept typed object parameters:
`block span({ length }: { length: number }): street(0, 0, length, 0); end`. Place one with
`span(10, 20, { length: 5 });`. See
[the language guide](docs.md#reusable-blocks-with-typed-parameters).

Clone with submodules: `git clone --recurse-submodules https://github.com/antoineMoPa/opencity.git`.

```sh
cargo run   # native editor
make build
make serve  # web editor
```

See [docs.md](docs.md) for the language, editor controls, and API. The editor exposes its live
[MCP and Unix socket API](docs/editor-api.md), including a wire protocol usable by external Python
clients. Reusable junction, ramp, and cloverleaf assets live in the
[road pattern library](road_library/README.md); the
[agent road-building guide](docs/road-building-guide-for-agents.md) explains how to trace a
reference map without impossible joins.

See [FORMATTING.md](FORMATTING.md) for formatting, lint checks and Git hook setup.
