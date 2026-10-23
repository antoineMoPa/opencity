"""Synchronous Python client for the running OpenCity desktop editor."""

from __future__ import annotations

import base64
import json
import os
from pathlib import Path
import socket
from typing import Any

from .document import Point, RoadDocument


class EditorError(RuntimeError):
    """The editor rejected a request or could not be reached."""


class OpenCityEditor:
    """Connect to the editor's local Unix socket, one request per connection.

    The desktop editor must already be open. Coordinates are OpenCity world units.
    ``segment_index`` values are statement indices and may change after source edits.
    """

    def __init__(self, socket_path: str | os.PathLike[str] | None = None, *, timeout: float = 30.0):
        if socket_path is None:
            socket_path = os.environ.get("OPENCITY_MCP_SOCKET") or Path.home() / ".opencity-agent" / "agent.sock"
        self.socket_path = Path(socket_path)
        self.timeout = timeout

    def call(self, tool_name: str, **arguments: Any) -> dict[str, Any]:
        """Call an editor tool and return its JSON result. Use for newer tools too."""
        request = json.dumps({"name": tool_name, "arguments": arguments}, allow_nan=False).encode() + b"\n"
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
                connection.settimeout(self.timeout)
                connection.connect(str(self.socket_path))
                connection.sendall(request)
                with connection.makefile("rb") as response:
                    line = response.readline()
        except OSError as error:
            raise EditorError(f"Cannot reach OpenCity editor at {self.socket_path}: {error}") from error
        if not line:
            raise EditorError("OpenCity editor closed the connection without a response")
        try:
            message = json.loads(line)
        except json.JSONDecodeError as error:
            raise EditorError("OpenCity editor sent invalid JSON") from error
        if message.get("isError"):
            content = message.get("content", [])
            detail = content[0].get("text", "Unknown editor error") if content else "Unknown editor error"
            raise EditorError(detail)
        content = message.get("content", [])
        if not content:
            raise EditorError("OpenCity editor sent an empty response")
        if content[0].get("type") == "image":
            return message
        if content[0].get("type") != "text":
            raise EditorError("OpenCity editor sent an unsupported response")
        try:
            value = json.loads(content[0]["text"])
        except (KeyError, json.JSONDecodeError) as error:
            raise EditorError("OpenCity editor sent invalid tool data") from error
        if not isinstance(value, dict):
            raise EditorError("OpenCity editor sent a non-object tool result")
        return value

    def state(self) -> dict[str, Any]:
        """Source, projected segments, junction refs, landscape, and dirty flag."""
        return self.call("get_road_state")

    def document(self) -> RoadDocument:
        """Start a batch edit from the editor's current source."""
        return RoadDocument(self.state()["source"])

    def replace_document(self, document: RoadDocument | str) -> dict[str, Any]:
        """Validate and apply a complete map as one undoable edit."""
        source = document.source if isinstance(document, RoadDocument) else document
        return self.call("replace_document", source=source)

    def save(self, path: str | os.PathLike[str]) -> dict[str, Any]:
        target = Path(path)
        if not target.is_absolute():
            raise ValueError("save path must be absolute")
        return self.call("save_document", path=str(target))

    def check_design_rules(self) -> dict[str, Any]:
        return self.call("check_design_rules")

    def add_road(
        self,
        road_type: str,
        start: Point,
        end: Point,
        *,
        name: str | None = None,
        start_ref: str | None = None,
        end_ref: str | None = None,
        control1: Point | None = None,
        control2: Point | None = None,
    ) -> int:
        """Add a road and return its statement index. Existing refs keep joins connected."""
        if (control1 is None) != (control2 is None):
            raise ValueError("provide both control points or neither")
        args: dict[str, Any] = {
            "road_type": road_type,
            "start_x": start.x, "start_y": start.y,
            "end_x": end.x, "end_y": end.y,
        }
        if name is not None:
            args["name"] = name
        if start_ref is not None:
            args["start_ref"] = start_ref
        if end_ref is not None:
            args["end_ref"] = end_ref
        if control1 is not None and control2 is not None:
            args.update(control1_x=control1.x, control1_y=control1.y,
                        control2_x=control2.x, control2_y=control2.y)
        return self.call("add_road", **args)["segment_index"]

    def move_road(self, segment_index: int, dx: float, dy: float) -> None:
        self.call("move_road", segment_index=segment_index, dx=dx, dy=dy)

    def move_control_point(self, segment_index: int, control: int, at: Point) -> None:
        if control not in (1, 2):
            raise ValueError("control must be 1 or 2")
        self.call("move_control_point", segment_index=segment_index, control=control, x=at.x, y=at.y)

    def move_endpoint(self, segment_index: int, end: bool, at: Point) -> None:
        self.call("move_endpoint", segment_index=segment_index, end=end, x=at.x, y=at.y)

    def set_intersection(self, point_ref: str, kind: str, road_type: str | None = None) -> None:
        if kind not in ("right_turn", "three_way", "four_way", "none"):
            raise ValueError("invalid intersection kind")
        args = {"point_ref": point_ref, "kind": kind}
        if road_type is not None:
            args["road_type"] = road_type
        self.call("set_intersection", **args)

    def viewport(self) -> dict[str, float]:
        return self.call("get_viewport")

    def set_viewport(self, center: Point, zoom: float) -> dict[str, float]:
        return self.call("set_viewport", center_x=center.x, center_y=center.y, zoom=zoom)

    def viewport_png(self) -> bytes:
        message = self.call("get_viewport_image")
        content = message["content"][0]
        if content.get("mimeType") != "image/png":
            raise EditorError("OpenCity editor did not return a PNG")
        return base64.b64decode(content["data"], validate=True)

    def undo(self) -> bool:
        return self.call("undo")["undone"]

    def redo(self) -> bool:
        return self.call("redo")["redone"]
