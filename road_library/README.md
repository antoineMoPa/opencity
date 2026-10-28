# Shared road patterns

Each `.road` asset opens by itself in OpenCity. Every outer road end is a named `map_point`, so it
can connect to another road or pattern without a coordinate guess. The samples use the editor's
fixed cross-section widths and world units.

Open [`qa_gallery.road`](qa_gallery.road) to inspect all five patterns in one scene. The junctions
are at the upper left, the elevated and flat ramps are below them, and the cloverleaf is on the
right.

<table>
  <thead>
    <tr>
      <th>
        Asset
      </th>
      <th>
        Outer connection points
      </th>
      <th>
        Network behavior
      </th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td>
        <code>four_way.road</code>
      </td>
      <td>
        <code>port_north</code>, <code>port_east</code>, <code>port_south</code>,
        <code>port_west</code>
      </td>
      <td>
        Four street stubs meet an explicit four-way intersection.
      </td>
    </tr>
    <tr>
      <td>
        <code>three_way.road</code>
      </td>
      <td>
        <code>port_north</code>, <code>port_east</code>, <code>port_west</code>
      </td>
      <td>
        Three street stubs meet an explicit T intersection.
      </td>
    </tr>
    <tr>
      <td>
        <code>street_to_interstate_flat.road</code>
      </td>
      <td>
        <code>street_w</code>, <code>street_e</code>, <code>highway_w</code>, <code>highway_e</code>
      </td>
      <td>
        The ramp leaves the eastbound side of the street and merges into an eastbound interstate
        lane at grade.
      </td>
    </tr>
    <tr>
      <td>
        <code>street_to_interstate_elevated.road</code>
      </td>
      <td>
        same as flat
      </td>
      <td>
        The ramp climbs from ground to a <code>0.3</code>-unit interstate deck with no grade step at
        the merge.
      </td>
    </tr>
    <tr>
      <td>
        <code>cloverleaf.road</code>
      </td>
      <td>
        <code>ew_w9</code>, <code>ew_e9</code>, <code>ns_s6</code>, <code>ns_n6</code>
      </td>
      <td>
        Four loop ramps, four direct connectors, and separate east-west and north-south decks.
      </td>
    </tr>
  </tbody>
</table>

## Plug patterns together in `.road`

The language has native `place(asset, id, x, y, quarter_turns)` and `join(a_id, a_port, b_id,
b_port)` calls. `asset` is one of the five names in the table above without `.road`. `quarter_turns`
is 0–3 counterclockwise; x and y translate in world units. The parser expands the asset into
ordinary named road calls and turns a join into one shared `map_point` reference. It rejects
mismatched coordinates, cross sections, heights, tangents, or one-way travel.

```text
place(four_way, cross, 0, 0, 0);
place(three_way, tee, 2.4, 0, 0);
join(cross, port_east, tee, port_west);
```

Open [`composition.example.road`](composition.example.road) in the editor to see the connected
result. Saving currently writes the expanded road calls, which remain fully editable. A join names
an outer point on each asset; for example, `cross.port_east` and `tee.port_west` become one point.
Keep permitted travel direction continuous when extending an unjoined highway port.

The cloverleaf is assembled as 25 short `interstate_core` pieces around sixteen ramp attachments,
with outer highway stubs added after those local patches. The core profile exposes its outer car
lanes for entries and merges; the outer stubs taper their verges before reaching it. Four five-piece
loops and four four-piece direct connectors link the ports. Keep the `ew_center` and `ns_center`
references separate: they occupy the same horizontal position but are on different decks. See [the
agent guide](../docs/road-building-guide-for-agents.md) for the local construction order.

Run Rust `check_design_rules` after placing patterns. It checks car-lane continuity at shared points
and lane attachments, including height and travel direction, and checks full-width lane surfaces
against non-car bands and resolved elevated walls. `role merge;` explicitly declares a car-only
merge surface that overlays the adjoining parent edge at its declared `lane_point`; use it for an
intentional entry or merge, and keep its lane center and height aligned with the parent car lane.
The street ramps use one at the street entry as well as the interstate merge. The checker reports
the offending segment and band for local repair.

The fixed illustrative widths and tessellated authoring geometry are suitable for authoring and
integration checks. A game's physical lane dimensions, shoulder geometry, clearance, markings,
collision mesh, and target pixel scale still need an engine-specific pass before shipping.

Ramp profiles declare `role ramp;`; merge profiles declare `role merge;`. Names have no semantic
effect. Elevated wall openings are clipped geometrically and shared by editor rendering, GLB export,
and design validation. Connector peaks in the cloverleaf rise to `0.65` to keep loop corridors clear
of their sidewalls. Run `cargo test --test road_library --test road_geometry` before accepting
changes; see the agent guide for tolerances and remaining physical-model limits.

The [default four-way component](default_four_way.road) uses the explicit network language rather
than the `place` pattern interface. It authors the map's curved sidewalks and four vehicle stop
lines, leaving the outside corners open without painted grass patches. Existing `intersection4`
junctions render it automatically. See the [intersection guide](../docs/intersections.md) for
inspection and SVG/PNG exports, and
[default_intersection.road](../examples/default_intersection.road) for a connected map example.
