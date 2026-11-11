# Procedural building families

Rectangular, unmodeled buildings with `material: glass` now use the shared procedural kit.
Their lot, elevation and height remain authored in `.road`. `family: legacy` retains the earlier
shell. Model references and nonrectangular polygons retain their existing rendering path.

The default map replaces all 786 stock glass lots. Both towns contain curtain-wall, mixed-wing,
podium-tower, heritage, civic and concrete-frame buildings, using nine structure palettes and four
glass tints. Other existing buildings and all roads, lot coordinates and elevations are retained.

```text
hall = building(0.4, {
    material: glass, family: podium, floor_height: 0.0125,
    palette: limestone, glazing: teal, weathering: 0.25,
    podium_floors: 2, tower_ratio: 0.48,
    section: bands, ground: retail, corners: glass,
    setback: on, roof_units: 4, seed: 42, downtown_index: 0.85
}, 0,0, 0.3,0, 0.3,0.2, 0,0.2);
```

| Option | Values |
| --- | --- |
| `family` | `auto`, `legacy`, `curtain_wall`, `mixed`, `podium` |
| Family, continued | `heritage`, `civic`, `concrete_frame` |
| `palette` | `seeded`, `limestone`, `sandstone`, `granite`, `basalt`, `chalk` |
| Palette, continued | `buff`, `brownstone`, `concrete`, `terracotta` |
| `glazing` | `seeded`, `blue`, `teal`, `silver`, `smoke` |
| `section` | `seeded`, `piers`, `bands`, `frame` |
| `ground` | `auto`, `lobby`, `retail`, `arcade` |
| `corners` | `stone`, `glass`; omitted chooses the family's edge rule |
| `weathering`, `downtown_index` | Finite numbers from 0 to 1 |
| `podium_floors` | 1 or 2 |
| `tower_ratio` | Width fraction from 0.35 to 0.70; depth adds 0.10, bounded at 0.78 |
| `setback` | `on` or `off` |
| `roof_units` | Integer from 0 to 8; omitted uses a seeded count |
| `seed` | Unsigned 32-bit salt combined with footprint coordinates |

Building properties expose family, palette, glazing and weathering, and display the resolved auto
family and downtown index. Editing height or material preserves the other authored recipe options.
Advanced options can also be set directly in the source.

## District distribution

`DowntownField` supplies a reusable, deterministic index:

```text
index = clamp(cos(min(distance / reach, 1) * PI / 2) + district_noise * amplitude, 0, 1)
```

Noise interpolates a seeded spatial field at two scales: `reach * 0.31` and `reach * 0.125`,
weighted 0.78 and 0.22. The single quarter cosine avoids repeating rings. Map indices and heights
are baked into its `.road` calls; the renderer does not move lots or override authored heights.
`BuildingFamily::for_index` interpolates these weights and draws without changing collection order:

| Family | Outer weight | Core weight |
| --- | ---: | ---: |
| Curtain wall | 2 | 36 |
| Mixed wings | 9 | 20 |
| Podium tower | 5 | 30 |
| Heritage | 42 | 3 |
| Civic | 14 | 3 |
| Concrete frame | 28 | 8 |

`floors_at` concentrates taller glass and concrete in the core while retaining low masonry
buildings. Heritage caps at eight stories, civic at nine, mixed/concrete at 24, and glass/podium at
the caller's ceiling, up to 40. Footprint hashing makes appearance independent of declaration order.

## Rendering and bounds

`runtime::architecture::BuildingRecipe` emits the same mesh for the editor and game. The generator
includes distinct ground floors, projecting cornices, narrow towers, setbacks, balcony guards,
parapets, coping, access cores, ducts, louvers and fan grilles. Roof equipment avoids optional
heritage and concrete penthouses. A small facade reveal separates wall skins from applied trim.
Collision uses the generated volumes, including the empty space above a podium outside its tower.

The kit derives its detail unit from `floor_height / 3.25`, reducing it for unusually small lots.
Four units of authored height are reserved for roof details. Cornices and a canopy envelope stay
inside the authored footprint. Consequently an authored height describes the complete building,
whereas the prototype's height label describes its wall height before rooftop equipment.

`runtime/architecture/facade.wgsl` is shared by editor and game shaders. It implements the
prototype's window proportions, arches, mullions, coursing, weathering, Fresnel reflection and
open-front room-box intersections. Rooms include side walls, floor, ceiling, lamp lighting,
wall panels, low cabinets and ceiling fixtures. No room meshes are created. The game samples its
existing atmosphere cube; the editor uses an analytic environment with matched daylight colors.
The editor retains its orthographic camera and has no building shadow-map pass.

Surface kinds 12 (`Facade`), 13 (`Masonry`) and 14 (`Glazing`) carry interpolated bay/floor
coordinates in `surface.yz`. `surface.w` stores an exact 24-bit integer:
family 0–2, palette 3–6, tint 7–8,
weathering 9–12, room seed 13–19, street flag 20, section 21–22, and solid-corner flag 23.
The existing runtime vertex layout is unchanged. Thin glazing and ventilation are opaque shader
surfaces, following the prototype's approach.

## Visual and functional verification

`examples/building_catalog.road` contains matched specimens of all six families. Its shell
dimensions, upper-floor counts, tints, palettes, weathering and service counts match the prototype.
The native catalog reserves extra footprint/height space for projections instead of clipping them.
The native seed stream intentionally uses Rust coordinate hashing, so individual
room finishes and seeded massing offsets need not match the browser bit for bit.

Run focused geometry, distribution and default-map regressions:

```sh
cargo test -p opencity --no-default-features --features game-runtime \
  --test buildings --test procedural_buildings --test default_population --test default_map
```

Capture six isolated 1024 × 768 specimens through the production native GPU pipeline:

```sh
cargo test -p opencity --no-default-features --features editor --bin editor \
  render_architecture_comparison -- --ignored --nocapture
```

Captures go to `/private/tmp/opencity-building-comparison`, or `OPENCITY_BUILDING_CAPTURE_DIR`.
For whole-map and family context captures, set `OPENCITY_QA_TARGET=1` and `OPENCITY_QA_BUILDINGS=1`
when running the ignored `render_starter_scene_pngs` test.
