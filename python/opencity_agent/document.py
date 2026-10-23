"""A small, explicit writer for the OpenCity .road language.

This appends statements without reparsing existing source. The editor validates the
complete program when ``replace_document`` is called.
"""

from __future__ import annotations

from dataclasses import dataclass
from decimal import Decimal
from enum import Enum
import json
import math
import re
from typing import Iterable


_IDENTIFIER = re.compile(r"[A-Za-z_][A-Za-z_0-9]*\Z")


class RoadDirection(str, Enum):
    BOTH = "both"
    FORWARD = "forward"
    REVERSE = "reverse"


class RoadElement(str, Enum):
    BUS_STOP = "busstop"
    SIDEWALK = "sidewalk"
    PARKING_LANE = "parkinglane"
    BIKE_LANE = "bikelane"
    BUS_LANE = "buslane"
    MIDDLE = "middle"
    CAR_LANE = "carlane"
    BUILDING_EDGE = "buildingedge"
    VERGE = "verge"
    BARRIER = "barrier"
    TRAIN_TRACK = "traintrack"
    TRAM_TRACK = "tramtrack"
    SUBWAY_TRACK = "subwaytrack"
    RUNWAY = "runway"
    TAXIWAY = "taxiway"
    APRON = "apron"


class LandscapeKind(str, Enum):
    LAND = "land"
    BEACH = "beach"
    LAKE = "lake"
    RIVER = "river"


class LaneSplitMode(str, Enum):
    DEDICATED = "dedicated"
    KEEP_LANE = "keep_lane"


@dataclass(frozen=True)
class Point:
    x: float
    y: float


def _identifier(value: str) -> str:
    if not isinstance(value, str) or not _IDENTIFIER.fullmatch(value):
        raise ValueError(f"invalid OpenCity identifier: {value!r}")
    return value


def _number(value: float) -> str:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ValueError(f"expected a finite number, got {value!r}")
    # The .road lexer accepts decimal literals but not exponent notation.
    return format(Decimal(str(value)), "f")


def _position(value: Point | str) -> str:
    if isinstance(value, Point):
        return f"{_number(value.x)}, {_number(value.y)}"
    return _identifier(value)


class RoadDocument:
    """Append map statements, preserving any existing .road source verbatim."""

    def __init__(self, source: str = "") -> None:
        self._source = source
        self._statements: list[str] = []

    @property
    def source(self) -> str:
        pieces = [self._source.rstrip(), *self._statements]
        return "\n".join(piece for piece in pieces if piece) + "\n"

    def define_road(
        self,
        name: str,
        elements: Iterable[RoadElement],
        direction: RoadDirection = RoadDirection.BOTH,
    ) -> None:
        parts = list(elements)
        if not parts or any(not isinstance(part, RoadElement) for part in parts):
            raise ValueError("elements must contain RoadElement values")
        if not isinstance(direction, RoadDirection):
            raise ValueError("direction must be a RoadDirection")
        body = "-".join(part.value for part in parts)
        travel = "" if direction is RoadDirection.BOTH else f" direction {direction.value};"
        self._statements.append(f"road {_identifier(name)}: {body};{travel} end;")

    def point(self, name: str, at: Point) -> None:
        self._statements.append(f"{_identifier(name)} = map_point({_position(at)});")

    def road(
        self,
        road_type: str,
        start: Point | str,
        end: Point | str,
        *,
        name: str | None = None,
        control1: Point | None = None,
        control2: Point | None = None,
    ) -> None:
        if (control1 is None) != (control2 is None):
            raise ValueError("provide both control points or neither")
        if control1 is None:
            if not isinstance(start, Point) or not isinstance(end, Point):
                raise ValueError("named endpoints require explicit control points")
            args = f"{_position(start)}, {_position(end)}"
        else:
            args = f"{_position(start)}, {_position(control1)}, {_position(control2)}, {_position(end)}"
        assignment = f"{_identifier(name)} = " if name is not None else ""
        self._statements.append(f"{assignment}{_identifier(road_type)}({args});")

    def lane_point(self, name: str, parent_road: str, element_index: int, fraction: float) -> None:
        """Attach a named point to a lane of an existing named placement."""
        if isinstance(element_index, bool) or not isinstance(element_index, int) or element_index < 0:
            raise ValueError("element_index must be a nonnegative integer")
        if not isinstance(fraction, (int, float)) or not 0 <= fraction <= 1:
            raise ValueError("fraction must be between 0 and 1")
        self._statements.append(
            f"{_identifier(name)} = lane_point({_identifier(parent_road)}, {element_index}, {_number(fraction)});"
        )

    def lane_split(
        self, parent_road: str, entry_road: str, mode: LaneSplitMode,
        *, full_road: str | None = None,
    ) -> None:
        if not isinstance(mode, LaneSplitMode):
            raise ValueError("mode must be a LaneSplitMode")
        final = f", {_identifier(full_road)}" if full_road is not None else ""
        self._statements.append(
            f"lane_split({_identifier(parent_road)}, {_identifier(entry_road)}, {mode.value}{final});"
        )

    def area(self, kind: LandscapeKind, points: Iterable[Point], *, name: str | None = None) -> None:
        if not isinstance(kind, LandscapeKind):
            raise ValueError("kind must be a LandscapeKind")
        vertices = list(points)
        minimum = 2 if kind is LandscapeKind.RIVER else 3
        if len(vertices) < minimum:
            raise ValueError(f"{kind.value} needs at least {minimum} points")
        assignment = f"{_identifier(name)} = " if name is not None else ""
        coordinates = ", ".join(_position(vertex) for vertex in vertices)
        self._statements.append(f"{assignment}{kind.value}({coordinates});")

    def vegetation(self, name: str, vegetation_type: str, points: Iterable[Point]) -> None:
        vertices = list(points)
        if len(vertices) < 3:
            raise ValueError("vegetation needs at least 3 points")
        coordinates = ", ".join(_position(vertex) for vertex in vertices)
        self._statements.append(
            f"{_identifier(name)} = vegetation({_identifier(vegetation_type)}, {coordinates});"
        )

    def terrain(self, name: str, height: float, points: Iterable[Point]) -> None:
        self._height_area("terrain", name, height, points)

    def building(
        self, name: str, height: float, points: Iterable[Point], *, model: str | None = None
    ) -> None:
        if model is not None and (not model or "\n" in model or "\r" in model):
            raise ValueError("model must be a nonempty single-line path")
        prefix = f"{_number(height)}, {json.dumps(model)}, " if model is not None else f"{_number(height)}, "
        self._height_area("building", name, height, points, prefix=prefix)

    def _height_area(
        self, kind: str, name: str, height: float, points: Iterable[Point], *, prefix: str | None = None
    ) -> None:
        vertices = list(points)
        if len(vertices) < 3:
            raise ValueError(f"{kind} needs at least 3 points")
        coordinates = ", ".join(_position(vertex) for vertex in vertices)
        self._statements.append(
            f"{_identifier(name)} = {kind}({prefix if prefix is not None else f'{_number(height)}, '}{coordinates});"
        )

    def elevation(self, name: str, height: float) -> None:
        self._statements.append(f"elevation({_identifier(name)}, {_number(height)});")

    def grade(self, name: str, start_height: float, end_height: float) -> None:
        self._statements.append(
            f"grade({_identifier(name)}, {_number(start_height)}, {_number(end_height)});"
        )
