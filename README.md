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
cargo run --features editor --bin editor   # native
make build
make serve
```

See [docs.md](docs.md) for the language, editor controls, and API.
