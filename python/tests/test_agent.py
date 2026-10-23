import base64
import json
import socket
import threading
import unittest
from unittest.mock import patch

from opencity_agent import (
    EditorError, IntersectionSide, LaneSplitMode, LandscapeKind, OpenCityEditor, Point, RoadDirection,
    RoadDocument, RoadElement,
)


class DocumentTests(unittest.TestCase):
    def test_builds_connected_map_with_direction_and_terrain(self):
        doc = RoadDocument("road existing: carlane; end;")
        doc.define_road("path", [RoadElement.VERGE, RoadElement.CAR_LANE], RoadDirection.FORWARD)
        doc.point("a", Point(0, 0))
        doc.point("b", Point(2, 0))
        doc.road("path", "a", "b", name="first", control1=Point(0.5, 0), control2=Point(1.5, 0))
        doc.area(LandscapeKind.BEACH, [Point(0, 0), Point(2, 0), Point(0, 2)], name="shore")
        doc.area(LandscapeKind.CONCRETE, [Point(0, 0), Point(2, 0), Point(0, 2)], name="quay")
        doc.vegetation("trees", "forest", [Point(0, 0), Point(1, 0), Point(0, 1)])
        doc.terrain("hill", 0.2, [Point(0, 0), Point(1, 0), Point(0, 1)])
        doc.building("hall", 0.5, [Point(0, 0), Point(1, 0), Point(0, 1)], model="assets/hall.glb")
        doc.grade("first", 0, 0.2)
        doc.lane_point("entry", "first", 1, 0.5)
        doc.lane_split("first", "first", LaneSplitMode.KEEP_LANE)
        doc.intersection("a", "path", 3, overrides={IntersectionSide.EAST: "existing"})
        self.assertIn("road path: verge-carlane; direction forward; end;", doc.source)
        self.assertIn("first = path(a, 0.5, 0, 1.5, 0, b);", doc.source)
        self.assertIn('hall = building(0.5, "assets/hall.glb", 0, 0, 1, 0, 0, 1);', doc.source)
        self.assertIn("shore = beach(0, 0, 2, 0, 0, 2);", doc.source)
        self.assertIn("quay = concrete(0, 0, 2, 0, 0, 2);", doc.source)
        self.assertIn("entry = lane_point(first, 1, 0.5);", doc.source)
        self.assertIn("intersection3(a, path, east, existing);", doc.source)

    def test_extends_document_statements_in_order(self):
        roads = RoadDocument()
        roads.road("local", "a", "b", name="link", control1=Point(1, 0), control2=Point(2, 0))
        doc = RoadDocument()
        doc.point("a", Point(0, 0))
        doc.point("b", Point(3, 0))
        doc.extend(roads)
        self.assertLess(doc.source.index("b = map_point"), doc.source.index("link = local"))
        with self.assertRaises(ValueError):
            doc.extend(RoadDocument("road existing: carlane; end;"))

    def test_small_numbers_use_decimal_syntax(self):
        doc = RoadDocument()
        doc.point("near_zero", Point(1e-8, -1e-8))
        self.assertEqual(doc.source, "near_zero = map_point(0.00000001, -0.00000001);\n")

    def test_rejects_invalid_values(self):
        doc = RoadDocument()
        with self.assertRaises(ValueError):
            doc.point("bad-name", Point(0, 0))
        with self.assertRaises(ValueError):
            doc.area(LandscapeKind.LAKE, [Point(0, 0), Point(1, 1)])
        with self.assertRaises(ValueError):
            doc.road("path", "a", "b")
        with self.assertRaises(ValueError):
            doc.terrain("hill", float("nan"), [Point(0, 0), Point(1, 0), Point(0, 1)])
        with self.assertRaises(ValueError):
            doc.intersection("a", "local", 5)
        with self.assertRaises(ValueError):
            doc.intersection("a", "local", 3, overrides={"east": "local"})


def serve_once(connection, result, requests):
    with connection:
        with connection.makefile("rb") as stream:
            requests.append(json.loads(stream.readline()))
        connection.sendall(json.dumps(result).encode() + b"\n")


class ConnectedSocket:
    """A client-side socketpair end with the connect call supplied by the editor protocol."""

    def __init__(self, connection):
        self.connection = connection
        self.path = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.connection.close()

    def settimeout(self, timeout):
        self.connection.settimeout(timeout)

    def connect(self, path):
        self.path = path

    def sendall(self, data):
        self.connection.sendall(data)

    def makefile(self, mode):
        return self.connection.makefile(mode)


class ClientTests(unittest.TestCase):
    def request(self, reply, action):
        client_end, server_end = socket.socketpair()
        client_end.settimeout(2)
        server_end.settimeout(2)
        mock_client = ConnectedSocket(client_end)
        requests = []
        thread = threading.Thread(target=serve_once, args=(server_end, reply, requests))
        thread.start()
        try:
            with patch("opencity_agent.client.socket.socket", return_value=mock_client):
                value = action(OpenCityEditor("/mock/editor.sock"))
        finally:
            thread.join(timeout=2)
        self.assertFalse(thread.is_alive())
        self.assertEqual(mock_client.path, "/mock/editor.sock")
        return value, requests[0]

    def test_add_road_sends_exact_coordinates_and_junction_refs(self):
        value, request = self.request(
            {"content": [{"type": "text", "text": '{"segment_index": 42}'}], "isError": False},
            lambda editor: editor.add_road("local", Point(1, 2), Point(3, 4), name="link", start_ref="p_001"),
        )
        self.assertEqual(value, 42)
        self.assertEqual(request["name"], "add_road")
        self.assertEqual(request["arguments"], {
            "road_type": "local", "name": "link", "start_ref": "p_001",
            "start_x": 1, "start_y": 2, "end_x": 3, "end_y": 4,
        })

    def test_editor_error_is_raised(self):
        with self.assertRaisesRegex(EditorError, "Unknown road type"):
            self.request(
                {"content": [{"type": "text", "text": "Unknown road type: x"}], "isError": True},
                lambda editor: editor.add_road("x", Point(0, 0), Point(1, 1)),
            )

    def test_source_replacement_uses_single_request(self):
        doc = RoadDocument()
        doc.define_road("roadway", [RoadElement.CAR_LANE])
        value, request = self.request(
            {"content": [{"type": "text", "text": '{"segments":0}'}], "isError": False},
            lambda editor: editor.replace_document(doc),
        )
        self.assertEqual(value["segments"], 0)
        self.assertEqual(request, {"name": "replace_document", "arguments": {"source": doc.source}})

    def test_viewport_image_decodes_png(self):
        png = b"\x89PNG\r\n\x1a\nexample"
        value, request = self.request(
            {"content": [{"type": "image", "mimeType": "image/png", "data": base64.b64encode(png).decode()}]},
            lambda editor: editor.viewport_png(),
        )
        self.assertEqual(value, png)
        self.assertEqual(request["name"], "get_viewport_image")


if __name__ == "__main__":
    unittest.main()
