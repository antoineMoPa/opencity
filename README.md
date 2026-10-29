# OpenCity

[comment]: <> (note for agents: add stuff to docs.md instead of here. keep readme short)

Road files define reusable cross sections and place them on a map:

```text
road street:
    sidewalk carlane middle carlane sidewalk;
end
street(0, 0, 10, 0);
```

Clone with submodules: `git clone --recurse-submodules https://github.com/antoineMoPa/opencity.git`.

```sh
cargo run   # native editor
make build
make serve  # web editor
```

See [docs.md](docs.md) for the language, editor controls, API and road-building guides.
