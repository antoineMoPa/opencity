# Explicit intersections and interchanges

A component defines its ports, geometric paths, permitted movements, pavement, sidewalks, and paint. Geometry and connectivity are separate: crossing two paths never connects them. Only an `allow` declaration permits a traversal. The same primitives work in `intersection`, `interchange`, and `road_segment` components.

## Define and place a component

```text
intersection crossing {
    port W {
        in at (-2, 0) heading east;
    }
    port E.out at (2, 0) heading east;

    path through {
        from W.in;
        line to E.out;
    }
    allow vehicle W.in -> E.out via through;
    surface pavement along through width 0.3 material asphalt;
}

cross = crossing(10, 20);
raised = crossing(15, 20, 1);
```

Named definitions are reusable templates. Call them with `x, y` and optional elevation, and optionally assign a name to the placed instance. An inline component can instead be written `intersection (10, 20) { ... }`, or `intersection local (10, 20) { ... }`. Braces close component declarations without a semicolon. Calls and declarations inside them retain their semicolons.

Coordinates inside components follow the proposal: +X east, +Y south, +Z up. Two-coordinate points have elevation zero. A placement uses the city's existing world coordinates (+Y north); local Y is converted when the component is placed. All distances use map units. Components can be called inside typed blocks; their placement arguments can use the block's parameters. Names inside each placed block are scoped to that instance.

The complete [intersection gallery](../examples/intersections.road) implements the proposal's four-way intersection with twelve movements, an elevated highway exit, and a shared-path merge.

## Ports and external roads

Ports can group `in` and `out` lane anchors, or declare each anchor separately:

```text
port N {
    in at (-0.03, -0.2) heading south width 0.03;
    out at (0.03, -0.2) heading north width 0.03;
}
port E.out at (0.2, 0.03, 0.1) heading east;
```

`in` arrives at the component; `out` departs from it. Headings are `north`, `east`, `south`, or `west`. Optional widths must be positive. An external road can use a placed component's lane anchor as an endpoint reference:

```text
road street:
    carlane;
end

far = map_point(14, 20);
approach = street(cross.E.out, 12.5, 20, 13.5, 20, far);
```

This six-argument road form uses endpoint references and two absolute Bézier control points. Port positions and default endpoint heights come from the component. Explicit road `grade` or `elevation` calls override those defaults. For an instance `local` inside a placed block `district`, the public reference is `district.local.E.out`. Roads inside that block use `local.E.out`.

## Paths, anchors, and permitted movements

A path starts with `from` (a port, anchor, or coordinate) or `move` (a coordinate). It then contains one or more segments:

```text
path turn {
    from N.in;
    cubic (-0.03, -0.08), (-0.08, -0.03) to W.out;
}
path curb {
    move (-0.2, -0.09);
    line (-0.13, -0.09);
    arc to (-0.09, -0.13) radius 0.04 clockwise;
    line (-0.09, -0.2);
}
```

`line` accepts a coordinate or `to` followed by a coordinate/reference. Cubics have two control points and an endpoint. Arcs accept `clockwise` or `counterclockwise`; the default is clockwise in the local +Y-south plane. The radius must span the chord. Arcs use the minor circular sweep, with elevation interpolated between endpoints. Cubics interpolate all three coordinates. Curves are resolved into 64 segments for display and distance calculations.

Anchors refer to distance along a path, and routes can reuse path ranges:

```text
anchor exit at highway distance 65;
path exit_ramp {
    from exit;
    cubic (-10, 0, 1), (30, -40, 8) to N.out;
}
allow vehicle W.in -> N.out via highway[0..exit] then exit_ramp;
```

Ranges accept numeric distances, named anchors on that path, `start`, and `end`. Omit the range to use the entire path. Paths and anchors may be declared in either order. A movement starts at an inbound port, ends at an outbound port, and must be continuous in all three coordinates. Additional vehicle-class names are retained as identifiers in the resolved movement data. A merge is an explicit shared anchor plus a declared route through it; proximity and overpasses do not imply connectivity.

Duplicate names, missing references, cyclic dependencies, out-of-range anchors, impossible arcs, disconnected routes, incorrect endpoint roles, and invalid widths are rejected with a source location. Port-heading mismatches produce advisory warnings in the viewer because the proposal's Bézier controls are schematic. Engineering constraints such as vehicle swept volume, grade limits, merge priority, and collision clearance are not simulated.

## Surfaces and markings

```text
surface pavement along highway width 7 material asphalt;
surface sidewalk along curb width 3 side left material sidewalk;
marking stop_line along stop_path width 0.3 color white;
```

Bands follow the referenced path's geometry and elevation. `side` is `center` (default), `left`, or `right`, relative to the path direction. Materials/colors are `asphalt`, `sidewalk`, `concrete`, `verge`, `white`, `yellow`, `red`, and `blue`. Surfaces default to asphalt, markings to white. Markings render above pavement in the map's depth-tested scene. Arbitrary filled regions, holes, lane-change rules, and traffic simulation remain outside this grammar.

## Read-only intersection viewer

Open a `.road` file in the editor, click the intersection's body in Top or Isometric view, and choose **Open intersection viewer**. Existing profile-based intersections also offer this option.

The isolated viewer provides layer toggles, ports and headings, path lengths, anchors, and a list of permitted movements. Select a movement to highlight its complete route, including shared path sections. Drag to pan, scroll to zoom, and use **Fit intersection** to reset framing. **Back** or Escape restores the map camera. Viewing and layer controls do not change the document or create undo entries. Four-way profile intersections use the shared default component, including its twelve movements. Two-way corners use their shared curved road profiles. Three-way profile intersections retain their appearance and ports without adding explicit movements.

## SVG and PNG command-line exports

Build the headless CLI without the GUI dependencies:

```sh
cargo build --no-default-features --features intersection-export --bin opencity
./target/debug/opencity --help
```

Export a definition or uniquely named placed instance:

```sh
./target/debug/opencity intersection-svg examples/intersections.road \
  --name crossroads --output /tmp/crossroads.svg
./target/debug/opencity intersection-png examples/intersections.road \
  --name highway_junction --output /tmp/highway.png --size 1600
```

Both commands also work with `./target/debug/editor`. `editor --help` lists them without opening a window. Each command also accepts `--help`.

`--name` can be omitted when the file contains exactly one explicit component definition. `--output` (or `-o`) is required. `--size` sets square image dimensions from 64 to 8192 pixels (default 1024). `--no-paths` hides diagnostic paths and movements while retaining surfaces, markings, and port indicators. `--appearance-only` also hides port indicators for a clean pavement, sidewalk, and paint image. Exports use the component's isolated local coordinates, so selecting a placed instance does not add its world translation to the image.

SVGs retain machine-readable path, band, port, and movement attributes. PNGs rasterize that same SVG locally with resvg; no GUI, browser, or external service is needed. Invalid input or options return a nonzero exit status before writing the output. Exports support explicit definitions and placed components. Existing four-way map junctions can also be selected by their point reference, such as `--name center` in `examples/default_intersection.road`.

## Default map appearance and stop lines

Two-way `intersection2` corners carry each connected road profile around a smooth quarter turn. Sidewalks, lanes, medians, and other road bands follow the bend and meet the approach edges; differing profiles taper their unmatched bands. New corners reserve room for the inner curb radius, while explicit port positions keep their authored footprint. The map, isolated viewer, and GLB export share this curved geometry.

[default_four_way.road](../road_library/default_four_way.road) authors the default four-way appearance: four quarter-circle sidewalks with curved inner and outer edges, and one white vehicle stop bar on each inbound approach. Pavement ends at the curved curbs; the outside corners remain open to the surrounding terrain instead of receiving decorative grass fills. Connected road profiles still supply their own edge bands. The twelve permitted movements are separate from this physical geometry. `marking stop_N along stop_N width 0.025 color white;` is a rendered paint band; it does not add traffic-signal or vehicle-stop simulation.

Existing `intersection4` map junctions use this component automatically. The template includes approach stubs for isolated viewing and export; the map clips its artwork to the existing junction footprint so connected roads retain their profiles. Road pavement, outlines, center markings, and walls end at that footprint. Center-connected legacy source retains its logical junction references, while the compiled road rows stop at the physical ports; explicit port-connected roads already end there. Graded approaches keep the height evaluated at the actual port boundary. This avoids coplanar approach pavement and markings continuing across the intersection.

Open [default_intersection.road](../examples/default_intersection.road) in the editor to inspect this integration. Export the clean reference appearance with:

```sh
./target/debug/opencity intersection-svg road_library/default_four_way.road \
  --name default_four_way --appearance-only --output /tmp/default-intersection.svg
./target/debug/opencity intersection-png road_library/default_four_way.road \
  --name default_four_way --appearance-only --output /tmp/default-intersection.png
```
