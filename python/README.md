# OpenCity Python interface

Use `opencity_agent` to inspect and edit the **running desktop editor**, or to
generate `.road` source in Python. It uses only the Python standard library.

```sh
python3 -m pip install -e ./python
cargo run --features editor --bin editor
python3 python/examples/build_and_edit.py
```

The example appends a road, land, and a building to the open map as **one undoable
edit**. It checks design rules but does not save the file. To save, call
`editor.save("/absolute/path/to/map.road")`.

## Live editing

```python
from opencity_agent import OpenCityEditor, Point

editor = OpenCityEditor()  # or OpenCityEditor("/path/to/agent.sock")
state = editor.state()
print(state["segments"][0])
print(state["intersections"][0]["ports"])

# Coordinates use map world units. A ref must come from current state.
index = editor.add_road("local", Point(0, 0), Point(2, 0), name="new_link")
editor.move_control_point(index, 1, Point(0.5, 0.25))
print(editor.check_design_rules()["issues"])
png_bytes = editor.viewport_png()
```

`OpenCityEditor` wraps the editor's existing Unix socket protocol. It exposes
`state`, `document`, `replace_document`, `save`, `add_road`, `move_road`,
`move_control_point`, `move_endpoint`, `set_intersection`, `check_design_rules`,
`viewport`, `set_viewport`, `viewport_png`, `undo`, and `redo`. `call(tool_name,
**arguments)` reaches any newer editor tool. Editor errors raise `EditorError`.
The editor must be open; the client does not launch it. The default socket is
`~/.opencity-agent/agent.sock`; `OPENCITY_MCP_SOCKET` overrides it in both
processes. The editor processes requests on its UI loop, with a 30 second client
timeout. The socket bridge currently requires Unix.

`state()` returns source, projected segments, intersections, landscape features,
landscape errors, and a dirty flag. Each segment reports `direction` (`forward`,
`reverse`, or `both`) plus `permits_forward` and `permits_reverse`; forward
means travel from the first placement point to the last. Each editable segment has a `segment_index`
that indexes a top-level source statement. Indices may change after source
replacement, so get fresh state before later index-based edits. Exact junction
`point_ref` strings from state can be passed as `start_ref` or `end_ref` to
`add_road`; the coordinates must match that junction. Nested block segments have
no editable index. `set_intersection` requires a junction with the matching
number of attached road ends.

## Batch map construction

```python
from opencity_agent import LandscapeKind, Point, RoadDirection, RoadDocument, RoadElement

doc = RoadDocument()
doc.define_road("lane", [RoadElement.VERGE, RoadElement.CAR_LANE], RoadDirection.FORWARD)
doc.point("a", Point(0, 0))
doc.point("b", Point(2, 0))
doc.road("lane", "a", "b", name="link", control1=Point(0.6, 0), control2=Point(1.4, 0))
doc.area(LandscapeKind.BEACH, [Point(-1, -1), Point(3, -1), Point(1, 1)], name="shore")
print(doc.source)
```

`RoadDocument` also writes `vegetation`, `terrain`, `building` (optional GLB
path), `elevation`, `grade`, `lane_point`, and `lane_split`. A straight road may
use two coordinate `Point`s; named endpoints require explicit Bézier controls.
To append to the running map, call `doc = editor.document()`, add statements,
then `editor.replace_document(doc)`. The editor validates the complete source
and applies it as one undoable change. `RoadDocument` preserves existing source
text and appends statements; it does not parse or edit existing declarations.

The API shape follows the current `.road` DSL, target map generator, and MCP
bridge. No saved conversation transcript from the original default-map work was
available in the task records.

Run tests with:

```sh
PYTHONPATH=python python3 -m unittest discover -s python/tests -v
cargo test --test python_interface
```
