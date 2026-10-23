"""Run with: PYTHONPATH=python python3 python/examples/build_and_edit.py

Start the desktop OpenCity editor first. This adds a small neighborhood to its
current map as one undoable edit, then reports design issues. It does not save.
"""

from opencity_agent import (
    LandscapeKind,
    OpenCityEditor,
    Point,
    RoadDirection,
    RoadElement,
)


editor = OpenCityEditor()
map_file = editor.document()
map_file.define_road(
    "agent_lane",
    [RoadElement.SIDEWALK, RoadElement.CAR_LANE, RoadElement.VERGE],
    RoadDirection.FORWARD,
)
map_file.point("agent_west", Point(15, 0))
map_file.point("agent_east", Point(17, 0))
map_file.road(
    "agent_lane", "agent_west", "agent_east", name="agent_road",
    control1=Point(15.67, 0), control2=Point(16.33, 0),
)
map_file.area(
    LandscapeKind.LAND,
    [Point(14, -1), Point(18, -1), Point(18, 1), Point(14, 1)],
    name="agent_land",
)
map_file.building(
    "agent_house", 0.15,
    [Point(15.2, 0.2), Point(15.6, 0.2), Point(15.6, 0.6), Point(15.2, 0.6)],
)
editor.replace_document(map_file)
print(editor.check_design_rules())
