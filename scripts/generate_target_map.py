"""Write the editable road layout traced from target.png.

Coordinates below are image pixels. Road placements are emitted as cubic
Bezier segments in the editor's world coordinates. Run this script after
adjusting the traced paths to refresh examples/target_map.road.
"""

from pathlib import Path
import sys

# Running this file directly keeps the local package importable without an
# editable pip installation.
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "python"))
from opencity_agent import (  # noqa: E402
    IntersectionSide, LandscapeKind, Point, RoadDirection, RoadDocument, RoadElement,
)

road_statements = RoadDocument()
points = {}
connection_arms = {}
placements = {}
road_paths = {}
road_traces = {}
mouth_connections = set()
city_grids = []
serial = 0
mouth_count = 0


def world(point):
    return ((point[0] - 768) / 60, (512 - point[1]) / 60)


def world_point(point):
    x, y = world(point)
    return Point(round(x, 4), round(y, 4))


def point_ref(point):
    key = (round(point[0], 3), round(point[1], 3))
    if key not in points:
        points[key] = f"p_{len(points):04d}"
    return points[key]


def road(name, kind, path, smooth=True):
    global serial
    road_paths[name] = (kind, path)
    trace = [path[0]]
    road_traces[name] = (kind, trace)
    for index, (start, end) in enumerate(zip(path, path[1:])):
        if smooth and len(path) > 2:
            previous = path[index - 1] if index else start
            following = path[index + 2] if index + 2 < len(path) else end
            first = (start[0] + (end[0] - previous[0]) / 6,
                     start[1] + (end[1] - previous[1]) / 6)
            second = (end[0] - (following[0] - start[0]) / 6,
                      end[1] - (following[1] - start[1]) / 6)
        else:
            first = (start[0] * 2 / 3 + end[0] / 3,
                     start[1] * 2 / 3 + end[1] / 3)
            second = (start[0] / 3 + end[0] * 2 / 3,
                      start[1] / 3 + end[1] * 2 / 3)
        for step in range(1, 9):
            t = step / 8
            inverse = 1 - t
            trace.append((inverse ** 3 * start[0]
                          + 3 * inverse ** 2 * t * first[0]
                          + 3 * inverse * t ** 2 * second[0]
                          + t ** 3 * end[0],
                          inverse ** 3 * start[1]
                          + 3 * inverse ** 2 * t * first[1]
                          + 3 * inverse * t ** 2 * second[1]
                          + t ** 3 * end[1]))
        a, b = point_ref(start), point_ref(end)
        if abs(end[0] - start[0]) >= abs(end[1] - start[1]):
            a_side = "east" if end[0] > start[0] else "west"
            b_side = "west" if end[0] > start[0] else "east"
        else:
            a_side = "south" if end[1] > start[1] else "north"
            b_side = "north" if end[1] > start[1] else "south"
        connection_arms.setdefault(a, []).append((a_side, kind))
        connection_arms.setdefault(b, []).append((b_side, kind))
        serial += 1
        placement = f"{name}_{serial:04d}"
        placements.setdefault(name, []).append(placement)
        road_statements.road(kind, a, b, name=placement,
                             control1=world_point(first), control2=world_point(second))


def ramp_mouth(name, parent, lane, fraction, freeway_point, loop_point, from_freeway):
    global mouth_count
    mouth_connections.add(loop_point)
    lane_ref = f"{name}_lane"
    road_statements.lane_point(lane_ref, parent, lane, round(fraction, 4))
    start, end = (freeway_point, loop_point) if from_freeway else (loop_point, freeway_point)
    first = (start[0] * 2 / 3 + end[0] / 3,
             start[1] * 2 / 3 + end[1] / 3)
    second = (start[0] / 3 + end[0] * 2 / 3,
              start[1] / 3 + end[1] * 2 / 3)
    start_ref = lane_ref if from_freeway else point_ref(loop_point)
    end_ref = point_ref(loop_point) if from_freeway else lane_ref
    road_statements.road("ramp_mouth", start_ref, end_ref, name=name,
                         control1=world_point(first), control2=world_point(second))
    mouth_count += 1


def grid(name, kind, xs, ys, mask=None):
    city_grids.append((name, xs, ys))
    for row, y in enumerate(ys):
        for col in range(len(xs) - 1):
            if mask is None or mask(xs[col], y, xs[col + 1], y):
                road(f"{name}_h{row}_{col}", kind,
                     [(xs[col], y), (xs[col + 1], y)], False)
    for col, x in enumerate(xs):
        for row in range(len(ys) - 1):
            if mask is None or mask(x, ys[row], x, ys[row + 1]):
                road(f"{name}_v{col}_{row}", kind,
                     [(x, ys[row]), (x, ys[row + 1])], False)


road_definitions = {
    "local": ([RoadElement.CAR_LANE, RoadElement.MIDDLE, RoadElement.CAR_LANE], RoadDirection.BOTH),
    "collector": ([RoadElement.SIDEWALK, RoadElement.CAR_LANE, RoadElement.MIDDLE,
                   RoadElement.CAR_LANE, RoadElement.SIDEWALK], RoadDirection.BOTH),
    "arterial": ([RoadElement.SIDEWALK, RoadElement.CAR_LANE, RoadElement.CAR_LANE,
                  RoadElement.MIDDLE, RoadElement.CAR_LANE, RoadElement.CAR_LANE,
                  RoadElement.SIDEWALK], RoadDirection.BOTH),
    "freeway": ([RoadElement.VERGE, RoadElement.CAR_LANE, RoadElement.CAR_LANE,
                 RoadElement.MIDDLE, RoadElement.CAR_LANE, RoadElement.CAR_LANE,
                 RoadElement.VERGE], RoadDirection.BOTH),
    "ramp": ([RoadElement.VERGE, RoadElement.CAR_LANE, RoadElement.VERGE], RoadDirection.FORWARD),
    "ramp_mouth": ([RoadElement.CAR_LANE, RoadElement.VERGE], RoadDirection.FORWARD),
    "airport_runway": ([RoadElement.VERGE, RoadElement.RUNWAY, RoadElement.VERGE], RoadDirection.BOTH),
    "airport_taxiway": ([RoadElement.VERGE, RoadElement.TAXIWAY, RoadElement.VERGE], RoadDirection.BOTH),
    "airport_apron": ([RoadElement.APRON], RoadDirection.BOTH),
}

# Coastline vertices follow the reference at a coarser scale than the roads.
# Separate polygons keep the outer islands independent of the mainland.
land_shapes = {
    "mainland": [
        (28, 348), (38, 312), (61, 275), (95, 241), (113, 208),
        (131, 178), (159, 164), (194, 143), (229, 132), (272, 136),
        (310, 133), (342, 109), (397, 105), (455, 88), (483, 83),
        (507, 98), (568, 104), (598, 84), (665, 88), (704, 89),
        (762, 58), (786, 40), (812, 42), (839, 56), (881, 65),
        (962, 51), (1001, 74), (1036, 107), (1080, 108),
        (1114, 83), (1130, 52), (1158, 47), (1186, 68),
        (1210, 97), (1247, 109), (1284, 104), (1320, 115),
        (1349, 137), (1407, 173), (1458, 178), (1481, 190),
        (1492, 218), (1471, 254), (1467, 289), (1494, 320),
        (1504, 353), (1482, 382), (1479, 412), (1491, 445),
        (1489, 479), (1472, 514), (1458, 540), (1463, 570),
        (1450, 606), (1425, 627), (1380, 635), (1350, 650),
        (1338, 670), (1360, 708),
        (1343, 747), (1300, 773), (1277, 784), (1253, 818),
        (1220, 805), (1192, 786), (1155, 811), (1112, 805),
        (1072, 778), (1032, 735), (1005, 692), (980, 686),
        (938, 706), (909, 694), (881, 666), (839, 665),
        (802, 678), (769, 663), (724, 661), (694, 637),
        (649, 633), (622, 650), (584, 625), (553, 620),
        (515, 595), (474, 579), (436, 558), (387, 550),
        (356, 552), (339, 589), (311, 602), (270, 585),
        (230, 590), (202, 568), (169, 554), (154, 527),
        (145, 498), (125, 470), (120, 443), (96, 420),
        (73, 403), (54, 374),
    ],
    "northwest_island": [
        (10, 46), (15, 31), (29, 18), (53, 14), (80, 22),
        (98, 38), (99, 67), (88, 88), (60, 98), (29, 91), (13, 75),
    ],
    "northwest_islet": [(122, 73), (126, 65), (136, 65), (143, 74),
                         (138, 84), (128, 84)],
    "southwest_island": [(329, 608), (337, 594), (353, 586),
                          (370, 592), (382, 608), (373, 624),
                          (354, 632), (336, 624)],
    "southwest_islet": [(379, 664), (386, 650), (400, 650),
                         (412, 664), (404, 680), (389, 680)],
    "southwest_rock": [(427, 668), (434, 663), (443, 667),
                        (447, 676), (439, 682), (430, 678)],
    "southern_island": [
        (826, 853), (851, 830), (878, 821), (906, 804),
        (931, 798), (956, 780), (989, 773), (1015, 788),
        (1020, 813), (1038, 826), (1070, 835), (1091, 855),
        (1081, 890), (1070, 930), (1045, 952), (1013, 961),
        (975, 968), (942, 958), (909, 971), (874, 959),
        (853, 938), (839, 910), (826, 882),
    ],
}


def beach_outline(outline, width=4.0):
    """Offset a traced coastline by a small, uniform distance in image pixels."""
    signed_area = sum(a[0] * b[1] - b[0] * a[1]
                      for a, b in zip(outline, outline[1:] + outline[:1]))
    side = 1 if signed_area > 0 else -1
    beach = []
    for index, point in enumerate(outline):
        previous = outline[index - 1]
        following = outline[(index + 1) % len(outline)]
        incoming = (point[0] - previous[0], point[1] - previous[1])
        outgoing = (following[0] - point[0], following[1] - point[1])
        incoming_length = (incoming[0] ** 2 + incoming[1] ** 2) ** 0.5
        outgoing_length = (outgoing[0] ** 2 + outgoing[1] ** 2) ** 0.5
        if incoming_length == 0 or outgoing_length == 0:
            raise ValueError("coastline has a repeated vertex")
        first = (side * incoming[1] / incoming_length,
                 -side * incoming[0] / incoming_length)
        second = (side * outgoing[1] / outgoing_length,
                  -side * outgoing[0] / outgoing_length)
        bisector = (first[0] + second[0], first[1] + second[1])
        denominator = bisector[0] * second[0] + bisector[1] * second[1]
        if denominator < 0.4:
            # Keep acute corners short enough to avoid coastline spikes.
            displacement = (second[0] * width, second[1] * width)
        else:
            displacement = (bisector[0] * width / denominator,
                            bisector[1] * width / denominator)
        beach.append((point[0] + displacement[0],
                      point[1] + displacement[1]))
    return beach


beach_shapes = {f"{name}_beach": beach_outline(outline)
                for name, outline in land_shapes.items()}

# Paved ground traced from the reference. Quays are independent shapes so
# harbor water stays open between the finger piers. Coordinates are pixels.
concrete_shapes = {
    # West port: the north landing, industrial apron, quays, and finger piers.
    "west_port_north_landing": [(77, 325), (196, 325), (196, 351),
                                 (141, 351), (141, 344), (77, 344)],
    "west_port_north_pier": [(109, 342), (116, 342), (116, 358), (109, 358)],
    "west_port_inner_pier": [(137, 344), (150, 344), (150, 385), (137, 385)],
    "west_port_middle_yard": [(149, 385), (199, 385), (199, 445),
                               (153, 445), (153, 417), (149, 417)],
    "west_port_west_quay": [(94, 411), (195, 411), (195, 442),
                            (94, 442)],
    "west_port_south_quay": [(196, 442), (229, 442), (229, 545),
                             (154, 545), (154, 527), (195, 527)],
    "west_port_south_pier_1": [(195, 457), (211, 457), (211, 465),
                                (195, 465)],
    "west_port_south_pier_2": [(193, 481), (209, 481), (209, 489),
                                (193, 489)],
    "west_port_south_pier_3": [(191, 505), (209, 505), (209, 513),
                                (191, 513)],
    # Industrial parcels bordering the west quay; streets remain visible.
    "west_industrial_north_1": [(82, 295), (105, 295),
                                  (105, 315), (82, 315)],
    "west_industrial_north_2": [(113, 295), (163, 295),
                                  (163, 315), (113, 315)],
    "west_industrial_north_3": [(173, 295), (192, 295),
                                  (192, 315), (173, 315)],
    "west_industrial_inner_1": [(230, 326), (282, 326),
                                  (282, 352), (230, 352)],
    "west_industrial_inner_2": [(292, 326), (342, 326),
                                  (342, 352), (292, 352)],
    "west_industrial_inner_3": [(230, 362), (281, 362),
                                  (281, 378), (230, 378)],
    # Airport: runway shoulders and the connected terminal hardstands.
    "airport_primary_shoulder": [(1174, 330), (1190, 346),
                                  (1308, 227), (1293, 212)],
    "airport_cross_shoulder": [(1199, 239), (1212, 225),
                                (1275, 287), (1261, 301)],
    "airport_north_taxi_apron": [(1255, 276), (1338, 276),
                                 (1338, 300), (1263, 300)],
    "airport_terminal_hardstand": [(1278, 301), (1339, 301),
                                    (1339, 359), (1278, 359)],
    "airport_terminal_forecourt": [(1280, 359), (1340, 359),
                                    (1340, 375), (1280, 375)],
    # East port: the waterfront apron follows the irregular basin edge.
    "east_port_north_yard": [(1264, 541), (1317, 541),
                              (1317, 584), (1264, 584)],
    "east_port_main_apron": [(1243, 582), (1337, 582),
                              (1337, 643), (1378, 643),
                              (1378, 682), (1255, 682),
                              (1255, 650), (1220, 650),
                              (1220, 623), (1243, 623)],
    "east_port_west_quay": [(1186, 606), (1217, 606),
                             (1217, 724), (1186, 724)],
    "east_port_north_pier": [(1280, 593), (1350, 593),
                              (1350, 605), (1280, 605)],
    "east_port_central_pier": [(1292, 621), (1353, 621),
                                (1353, 633), (1292, 633)],
    "east_port_long_pier": [(1262, 688), (1357, 688),
                             (1357, 715), (1262, 715)],
    "east_port_south_pier": [(1184, 702), (1223, 702),
                              (1223, 719), (1184, 719)],
    "east_port_east_pier": [(1340, 548), (1355, 548),
                             (1390, 594), (1374, 606)],
    # Industrial street blocks inland of the east quay.
    "east_industrial_west_1": [(1062, 525), (1111, 525),
                                (1111, 544), (1062, 544)],
    "east_industrial_west_2": [(1119, 525), (1156, 525),
                                (1156, 544), (1119, 544)],
    "east_industrial_west_3": [(1062, 553), (1111, 553),
                                (1111, 572), (1062, 572)],
    "east_industrial_west_4": [(1119, 553), (1156, 553),
                                (1156, 572), (1119, 572)],
    "east_industrial_inner_1": [(1178, 553), (1227, 553),
                                 (1227, 572), (1178, 572)],
    "east_industrial_inner_2": [(1178, 581), (1227, 581),
                                 (1227, 600), (1178, 600)],
}

# Broad wooded regions, raised ground, and city building lots.
# Vegetation polygons mark cover only; they do not generate individual trees.
vegetation_shapes = {
    "central_forest": [(601, 135), (699, 112), (778, 150), (791, 264),
                       (737, 317), (637, 304), (588, 240)],
    "east_forest": [(852, 129), (952, 113), (997, 168), (968, 271),
                    (909, 310), (848, 262)],
    "north_forest": [(1116, 118), (1148, 105), (1192, 113),
                     (1213, 138), (1190, 167), (1135, 157),
                     (1104, 135)],
}
terrain_shapes = {
    "central_hill": (0.15, [(612, 185), (667, 165), (715, 192),
                             (730, 251), (682, 280), (626, 259)]),
    "central_ridge": (0.27, [(643, 184), (685, 175), (720, 208),
                               (714, 246), (673, 260), (635, 228)]),
    "central_peak": (0.41, [(664, 196), (689, 193), (706, 213),
                              (692, 239), (666, 231)]),
    "southern_hill": (0.12, [(685, 405), (739, 390), (773, 437),
                              (753, 483), (699, 489)]),
    "southern_summit": (0.24, [(716, 423), (745, 412), (759, 439),
                                 (741, 466), (714, 458)]),
}
building_shapes = {}


def building_lots(label, bounds, columns, rows):
    """Place separate footprints inside one street block, clear of the roads."""
    left, top, right, bottom = bounds
    inset = 5.5
    gap = 2
    usable_width = right - left - inset * 2 - gap * (columns - 1)
    usable_height = bottom - top - inset * 2 - gap * (rows - 1)
    width = usable_width / columns
    depth = usable_height / rows
    if width < 3 or depth < 3:
        return 0
    placed = 0
    for row in range(rows):
        for col in range(columns):
            x = left + inset + col * (width + gap)
            y = top + inset + row * (depth + gap)
            name = f"{label}_{row + 1}_{col + 1}"
            height = 0.08 + 0.025 * ((row + col) % 4)
            outline = [(x, y), (x + width, y),
                       (x + width, y + depth), (x, y + depth)]
            if footprint_clear(outline):
                building_shapes[name] = (height, outline)
                placed += 1
    return placed


# East-west I-10: the coastline interchange, central cloverleaf, and southeast
# trumpet are distinct from the surrounding street network.
road("i10", "freeway", [(178, 193), (235, 212), (343, 233), (445, 301),
                         (566, 339), (700, 349), (817, 351), (961, 356),
                         (1090, 365), (1190, 395), (1303, 451), (1442, 489)])
road("i95", "freeway", [(816, 76), (816, 155), (817, 254), (817, 351),
                         (817, 441), (817, 528), (817, 642)])

# Central cloverleaf: four circular loops and the long diagonal direct ramps.
for label, path in [
    ("nw_loop", [(783, 351), (777, 338), (784, 321), (800, 317), (817, 323)]),
    ("ne_loop", [(817, 323), (833, 316), (851, 320), (858, 338), (850, 352)]),
    ("se_loop", [(850, 352), (858, 368), (850, 385), (833, 388), (817, 379)]),
    ("sw_loop", [(817, 379), (800, 388), (783, 384), (776, 367), (783, 351)]),
    ("nw_direct", [(807, 270), (792, 303), (759, 330), (717, 338)]),
    ("ne_direct", [(932, 343), (878, 337), (842, 300), (826, 270)]),
    ("sw_direct", [(717, 363), (759, 375), (797, 422), (807, 510)]),
    ("se_direct", [(827, 510), (842, 422), (879, 379), (932, 368)]),
]:
    road(label, "ramp", path)

# Every direct turning ramp begins and ends at an outside freeway lane. The
# short mouths provide a visible taper instead of ending on a centerline.
for name, parent, lane, fraction, freeway, turn, from_freeway in [
    ("nw_direct_from_95", placements["i95"][2], 5, 0.16, (817, 270), (807, 270), True),
    ("nw_direct_to_10", placements["i10"][5], 1, 0.13, (715, 349), (717, 338), False),
    ("ne_direct_from_10", placements["i10"][6], 1, 0.80, (932, 355), (932, 343), True),
    ("ne_direct_to_95", placements["i95"][2], 1, 0.16, (817, 270), (826, 270), False),
    ("sw_direct_from_10", placements["i10"][5], 5, 0.13, (715, 349), (717, 363), True),
    ("sw_direct_to_95", placements["i95"][4], 5, 0.82, (817, 512), (807, 510), False),
    ("se_direct_from_95", placements["i95"][4], 1, 0.82, (817, 512), (827, 510), True),
    ("se_direct_to_10", placements["i10"][6], 5, 0.80, (932, 355), (932, 368), False),
]:
    ramp_mouth(name, parent, lane, fraction, freeway, turn, from_freeway)

# Each mouth follows the travel direction of its freeway lane before the loop
# bends away, or after it merges back. Opposing arms use different lane points.
ramp_mouth("nw_loop_entry", placements["i10"][5], 1, 0.85,
           (800, 351), (783, 351), True)
ramp_mouth("nw_loop_merge", placements["i95"][2], 5, 0.85,
           (817, 337), (817, 323), False)
ramp_mouth("ne_loop_entry", placements["i95"][2], 1, 0.85,
           (817, 337), (817, 323), True)
ramp_mouth("ne_loop_merge", placements["i10"][6], 1, 0.10,
           (831, 351), (850, 352), False)
ramp_mouth("se_loop_entry", placements["i10"][6], 5, 0.10,
           (831, 351), (850, 352), True)
ramp_mouth("se_loop_merge", placements["i95"][3], 1, 0.15,
           (817, 365), (817, 379), False)
ramp_mouth("sw_loop_entry", placements["i95"][3], 5, 0.15,
           (817, 365), (817, 379), True)
ramp_mouth("sw_loop_merge", placements["i10"][5], 5, 0.85,
           (800, 351), (783, 351), False)

# The northwest and southeast terminals have large free-flow turning ramps.
for label, path in [
    ("west_ramp_n", [(164, 201), (180, 190), (199, 184), (229, 185)]),
    ("west_ramp_s", [(190, 207), (211, 215), (223, 236), (226, 278)]),
    ("west_ramp_e", [(229, 185), (250, 191), (331, 206), (368, 228)]),
    ("east_ramp_n", [(1320, 436), (1354, 445), (1398, 460), (1442, 463)]),
    ("east_ramp_s", [(1319, 470), (1354, 485), (1396, 506), (1417, 525)]),
    ("west_ramp_to_grid", [(223, 236), (250, 244), (286, 259), (346, 278)]),
    ("west_grid_to_freeway", [(346, 305), (375, 296), (393, 300), (418, 303)]),
    ("east_ramp_loop", [(1442, 463), (1459, 454), (1474, 469),
                          (1470, 490), (1448, 495), (1428, 469)]),
]:
    road(label, "ramp", path)

for name, parent, lane, fraction, freeway, turn, from_freeway in [
    ("west_depart_10", placements["i10"][0], 5, 0.22, (190, 197), (190, 207), True),
    ("west_merge_10", placements["i10"][2], 5, 0.25, (368, 248), (368, 228), False),
    ("west_grid_merge_10", placements["i10"][2], 5, 0.95, (440, 298), (418, 303), False),
    ("east_depart_10_n", placements["i10"][10], 5, 0.01, (1304, 451), (1320, 436), True),
    ("east_depart_10_s", placements["i10"][10], 5, 0.12, (1319, 457), (1319, 470), True),
]:
    ramp_mouth(name, parent, lane, fraction, freeway, turn, from_freeway)

# Small trumpet turns join I-95 to the road running along each shore.
for label, path in [
    ("i95_north_west", [(807, 85), (799, 72), (784, 83), (775, 89)]),
    ("i95_north_east", [(826, 85), (833, 87), (851, 90), (881, 88)]),
    ("i95_north_return", [(784, 83), (807, 85), (829, 83), (851, 90)]),
    ("i95_south_west", [(807, 613), (792, 635), (766, 635), (745, 650)]),
    ("i95_south_east", [(827, 613), (842, 637), (877, 629), (901, 623)]),
    ("i95_south_turn", [(766, 635), (787, 647), (818, 646), (842, 637)]),
]:
    road(label, "ramp", path)

for name, parent, lane, fraction, freeway, turn in [
    ("north_depart_95_w", placements["i95"][0], 1, 0.30, (816, 100), (807, 85)),
    ("north_depart_95_e", placements["i95"][0], 5, 0.12, (816, 85), (826, 85)),
    ("south_depart_95_w", placements["i95"][5], 5, 0.65, (817, 602), (807, 613)),
    ("south_depart_95_e", placements["i95"][5], 5, 0.65, (817, 602), (827, 613)),
]:
    ramp_mouth(name, parent, lane, fraction, freeway, turn, True)

# North shore and west city approaches.
for label, kind, path in [
    ("north_coast", "collector", [(146, 246), (164, 201), (180, 180), (250, 143), (322, 153),
                                     (377, 126), (430, 137), (453, 162), (451, 187),
                                     (408, 211), (368, 228)]),
    ("north_arterial", "arterial", [(76, 321), (104, 276), (146, 246)]),
    ("west_bypass", "collector", [(109, 321), (149, 322), (177, 360), (222, 401),
                                    (242, 440), (266, 481), (248, 545)]),
    ("west_south_coast", "collector", [(248, 545), (309, 503), (364, 509),
                                         (453, 508), (505, 517), (563, 555),
                                         (633, 577), (701, 597), (759, 628)]),
    ("west_to_mid", "collector", [(442, 298), (466, 360), (506, 408),
                                    (553, 412), (622, 428), (660, 415)]),
    ("north_mid", "collector", [(453, 162), (526, 143), (580, 143), (606, 173),
                                  (602, 233), (572, 283), (505, 304), (442, 298)]),
    ("north_to_95", "collector", [(606, 173), (658, 122), (727, 119),
                                    (775, 89)]),
    ("northeast_coast", "collector", [(816, 76), (881, 88), (913, 88), (969, 96),
                                        (1020, 128), (1067, 150), (1123, 151),
                                        (1194, 140), (1270, 148), (1332, 174)]),
    ("east_north_loop", "collector", [(1067, 150), (1069, 228), (1090, 287),
                                        (1128, 323), (1135, 363), (1129, 420), (1144, 465)]),
    ("east_airport_access", "collector", [(1135, 363), (1160, 373), (1191, 373),
                                            (1268, 372), (1337, 374), (1361, 325),
                                            (1359, 264), (1332, 174)]),
    ("airport_ring", "collector", [(1128, 323), (1138, 267), (1177, 216),
                                     (1223, 178), (1270, 148), (1317, 190),
                                     (1352, 224), (1359, 264)]),
    ("east_coast_north", "collector", [(1332, 174), (1405, 228), (1439, 293),
                                         (1442, 323)]),
    ("east_coast_south", "collector", [(1442, 349), (1435, 401), (1428, 469)]),
    ("east_south", "collector", [(1442, 489), (1417, 525), (1375, 544),
                                   (1298, 539), (1247, 556), (1237, 601),
                                   (1170, 628), (1090, 633), (1046, 668),
                                   (996, 626), (966, 571), (902, 527), (817, 528)]),
    ("south_95_access", "ramp", [(901, 623), (937, 621),
                                        (991, 623), (1046, 668)]),
]:
    road(label, kind, path)

# A connected west-side lattice, with smaller blocks near the waterfront.
grid("west", "local", [196, 226, 257, 287, 317, 346, 376, 405, 434],
     [254, 278, 305, 321, 358, 384, 410, 438, 467, 494],
     lambda x1, y1, x2, y2: (x1 >= 226 or y1 >= 305)
     and (x1 <= 376 or y1 >= 321)
     and not (x1 >= 376 and y1 <= 278)
     and not (y1 == y2 == 321))
grid("west_north", "local", [290, 315, 339, 365, 390],
     [162, 185, 209, 232],
     lambda x1, y1, x2, y2: (x1 >= 339 or y1 >= 185)
     and not (x1 == x2 == 365 and y1 == 209))
grid("west_waterfront", "local", [81, 109, 138, 168, 196],
     [264, 290, 321, 341],
     lambda x1, y1, x2, y2: (x1 <= 168 or y1 >= 290)
     and not (x1 <= 109 and y1 <= 290)
     and not (x1 == x2 == 196)
     and not (y1 == y2 == 321))
grid("west_harbor", "local", [155, 178, 201, 224],
     [423, 450, 476, 503, 530, 550],
     lambda x1, y1, x2, y2: (x1 >= 178 or y1 >= 476)
     and not (x1 <= 178 and y1 >= 503))
grid("west_south", "local", [248, 276, 305, 335],
     [495, 520, 545, 570],
     lambda x1, y1, x2, y2: x1 <= 305 or y1 <= 520)
road("west_main", "arterial", [(76, 321), (81, 321), (109, 321),
                                  (138, 321), (168, 321), (196, 321),
                                  (226, 321), (257, 321), (287, 321),
                                  (317, 321), (346, 321), (376, 321),
                                  (405, 321), (434, 321), (442, 298)])
road("west_diagonal", "collector", [(168, 321), (207, 365), (249, 398),
                                       (271, 429), (287, 467)])

# East city: blocks cluster southeast of the interchange and run to the quay.
grid("east", "local", [1058, 1085, 1115, 1144, 1160, 1173, 1202, 1231, 1260, 1289, 1318, 1347],
     [465, 492, 520, 548, 576, 604, 632, 660, 689, 718, 747],
     lambda x1, y1, x2, y2: (x1 >= 1115 or y1 <= 604)
     and (x1 <= 1289 or y1 <= 548)
     and (y1 <= 660 or x1 >= 1144)
     and not (x1 >= 1289 and y1 >= 604)
     and not (x1 == x2 == 1160)
     and not (y1 == y2 == 520))
grid("east_north", "local", [1346, 1370, 1395, 1420],
     [242, 268, 294, 320],
     lambda x1, y1, x2, y2: x1 <= 1395 or y1 >= 268)
grid("east_edge", "local", [1361, 1385, 1410, 1434],
     [540, 566, 592],
     lambda x1, y1, x2, y2: x1 >= 1385 or y1 <= 540)
road("east_main", "arterial", [(1058, 520), (1085, 520), (1115, 520),
                                  (1144, 520), (1160, 520), (1173, 520),
                                  (1202, 520), (1231, 520), (1260, 520),
                                  (1289, 520), (1318, 520), (1347, 520),
                                  (1375, 544)])
road("east_spine", "arterial", [(1160, 422), (1160, 465), (1160, 492),
                                   (1160, 520), (1160, 548), (1160, 576),
                                   (1160, 604), (1160, 632), (1160, 660),
                                   (1160, 689), (1160, 718), (1160, 747), (1160, 749)])
road("east_diag", "collector", [(1303, 451), (1272, 480), (1250, 520),
                                   (1240, 559), (1237, 601)])
road("east_port_approach", "collector", [(1202, 660), (1199, 694),
                                            (1234, 718), (1289, 718)])

# Small southern island neighborhood and its loop approach.
grid("island", "local", [930, 952, 975, 998], [874, 897, 921, 943])
road("island_loop", "collector", [(930, 874), (912, 856), (911, 835),
                                    (925, 816), (955, 807), (985, 810),
                                    (1006, 825), (1014, 846), (998, 874)])

# Local connectors that provide the shape of the waterfront street ends.
for label, path in [
    ("west_quay", [(196, 341), (196, 390), (198, 447), (220, 477), (248, 495)]),
    ("west_shore", [(346, 467), (360, 500), (421, 508), (453, 508)]),
    ("mid_woods", [(817, 528), (785, 590), (759, 628), (745, 650)]),
    ("east_quay", [(1160, 749), (1185, 767), (1232, 771)]),
    ("airport_feeder", [(1361, 325), (1401, 325), (1429, 336)]),
    ("airport_to_east_city", [(1160, 373), (1160, 422)]),
]:
    road(label, "arterial" if label == "airport_to_east_city" else "collector", path)

# The circular turnaround at the northeastern cape joins the shore road and
# airport approach without forcing a sharp T turn.
road("east_roundabout", "collector", [(1442, 323), (1451, 327), (1455, 336),
                                        (1451, 345), (1442, 349), (1433, 345),
                                        (1429, 336), (1433, 327), (1442, 323)])

# The airport has two crossing runways, a parallel taxi route, and terminal
# access. The runway and taxiway element implementations are owned separately.
road("airport_runway_primary", "airport_runway", [(1185, 332), (1298, 220)], False)
road("airport_runway_cross", "airport_runway", [(1208, 245), (1262, 296)], False)
road("airport_taxi", "airport_taxiway", [(1241, 276), (1260, 286), (1311, 286), (1334, 322)], False)
road("airport_taxi_apron", "airport_taxiway", [(1280, 322), (1334, 322)], False)
road("airport_apron_link", "airport_taxiway", [(1334, 322), (1335, 338)], False)
road("airport_apron", "airport_apron", [(1302, 338), (1335, 338)], False)
road("airport_terminal", "collector", [(1334, 322), (1337, 374)], False)


def point_in_polygon(point, outline):
    x, y = point
    inside = False
    for a, b in zip(outline, outline[1:] + outline[:1]):
        if (a[1] > y) != (b[1] > y):
            crossing_x = a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1])
            if x < crossing_x:
                inside = not inside
    return inside


def segment_hits_rectangle(a, b, rectangle):
    """Liang-Barsky clip against the footprint plus the road's half-width."""
    left, top, right, bottom = rectangle
    dx, dy = b[0] - a[0], b[1] - a[1]
    low, high = 0.0, 1.0
    for p, q in ((-dx, a[0] - left), (dx, right - a[0]),
                 (-dy, a[1] - top), (dy, bottom - a[1])):
        if p == 0:
            if q < 0:
                return False
        else:
            fraction = q / p
            if p < 0:
                low = max(low, fraction)
            else:
                high = min(high, fraction)
            if low > high:
                return False
    return True


road_half_width = {
    "local": 3.5, "collector": 5.4, "arterial": 8.1,
    "freeway": 8.3, "ramp": 3.5, "airport_runway": 7.5,
    "airport_taxiway": 5, "airport_apron": 7.2,
}


def footprint_clear(outline):
    if not all(any(point_in_polygon(corner, land) for land in land_shapes.values())
               for corner in outline):
        return False
    xs = [point[0] for point in outline]
    ys = [point[1] for point in outline]
    left, top, right, bottom = min(xs), min(ys), max(xs), max(ys)
    if any(any(point_in_polygon(corner, paved) for corner in outline)
           or any(left <= x <= right and top <= y <= bottom
                  for x, y in paved)
           for name, paved in concrete_shapes.items()
           if name.startswith(("west_port_", "east_port_", "airport_"))):
        return False
    candidates = set()
    for tile_x in range(int(left // 64), int(right // 64) + 1):
        for tile_y in range(int(top // 64), int(bottom // 64) + 1):
            candidates.update(road_segments_by_tile.get((tile_x, tile_y), ()))
    for index in candidates:
        a, b, clearance, extent = road_segments[index]
        if right < extent[0] or left > extent[2] or bottom < extent[1] or top > extent[3]:
            continue
        rectangle = (left - clearance, top - clearance,
                     right + clearance, bottom + clearance)
        if segment_hits_rectangle(a, b, rectangle):
            return False
    return True


def road_covers_edge(start, end):
    """Check that local streets or an arterial enclose the whole block side."""
    horizontal = start[1] == end[1]
    fixed = start[1] if horizontal else start[0]
    first = min(start[0], end[0]) if horizontal else min(start[1], end[1])
    last = max(start[0], end[0]) if horizontal else max(start[1], end[1])
    spans = []
    for kind, path in road_paths.values():
        if kind not in ("local", "arterial"):
            continue
        for a, b in zip(path, path[1:]):
            if horizontal and a[1] == b[1] == fixed:
                spans.append((min(a[0], b[0]), max(a[0], b[0])))
            elif not horizontal and a[0] == b[0] == fixed:
                spans.append((min(a[1], b[1]), max(a[1], b[1])))
    covered = first
    for low, high in sorted(spans):
        if low <= covered:
            covered = max(covered, high)
        if covered >= last:
            return True
    return False


def compact_lot(label, bounds):
    """Find a small safe footprint in blocks cut by a diagonal or shore road."""
    left, top, right, bottom = bounds
    for size in (6, 4, 3):
        positions = [(x, y)
                     for x in range(left + 4, right - size - 3)
                     for y in range(top + 4, bottom - size - 3)]
        positions.sort(key=lambda point: ((point[0] + size / 2 - (left + right) / 2) ** 2
                                          + (point[1] + size / 2 - (top + bottom) / 2) ** 2))
        for x, y in positions:
            outline = [(x, y), (x + size, y),
                       (x + size, y + size), (x, y + size)]
            if footprint_clear(outline):
                building_shapes[f"{label}_compact"] = (0.08, outline)
                return True
    return False


road_segments = []
road_segments_by_tile = {}
for kind, trace in road_traces.values():
    clearance = road_half_width[kind] + 1
    for a, b in zip(trace, trace[1:]):
        extent = (min(a[0], b[0]) - clearance, min(a[1], b[1]) - clearance,
                  max(a[0], b[0]) + clearance, max(a[1], b[1]) + clearance)
        index = len(road_segments)
        road_segments.append((a, b, clearance, extent))
        for tile_x in range(int(extent[0] // 64), int(extent[2] // 64) + 1):
            for tile_y in range(int(extent[1] // 64), int(extent[3] // 64) + 1):
                road_segments_by_tile.setdefault((tile_x, tile_y), []).append(index)

city_blocks = 0
unbuildable_blocks = []
for grid_name, xs, ys in city_grids:
    for row, (top, bottom) in enumerate(zip(ys, ys[1:])):
        for col, (left, right) in enumerate(zip(xs, xs[1:])):
            corners = [(left, top), (right, top), (right, bottom), (left, bottom)]
            bordered_sides = sum(road_covers_edge(a, b)
                                 for a, b in zip(corners, corners[1:] + corners[:1]))
            # Edge parcels can have only two street sides when a grid ends at
            # the shore or an older road. Fewer sides are outside the city.
            if bordered_sides < 2:
                continue
            # The grid is only a candidate: coast and highway approaches can
            # cross its outer cells. The footprint check leaves those clear.
            label = f"{grid_name}_block_{row + 1}_{col + 1}"
            columns = 2 if right - left >= 22 else 1
            rows = 2 if bottom - top >= 22 else 1
            placed = building_lots(label, (left, top, right, bottom), columns, rows)
            if placed or compact_lot(label, (left, top, right, bottom)):
                city_blocks += 1
            else:
                unbuildable_blocks.append(label)

print(f"filled {city_blocks} street-grid blocks with {len(building_shapes)} buildings")
if unbuildable_blocks:
    print(f"left {len(unbuildable_blocks)} blocks occupied by roads/coast clear")

# A ramp endpoint must have an explicit shared road junction or a lane mouth.
# This catches near misses that look connected in a zoomed-out screenshot.
for name, (kind, path) in road_paths.items():
    if kind != "ramp":
        continue
    for endpoint in (path[0], path[-1]):
        neighbors = [other for other, (_, other_path) in road_paths.items()
                     if other != name and endpoint in other_path]
        if endpoint not in mouth_connections and not neighbors:
            raise ValueError(f"{name} has an unconnected ramp endpoint at {endpoint}")
for endpoint in mouth_connections:
    if not any(kind == "ramp" and endpoint in path
               for kind, path in road_paths.values()):
        raise ValueError(f"lane mouth has no ramp at {endpoint}")

document = RoadDocument()
for name, (elements, direction) in road_definitions.items():
    document.define_road(name, elements, direction)
for position, name in points.items():
    document.point(name, world_point(position))
for name, outline in beach_shapes.items():
    document.area(LandscapeKind.BEACH, map(world_point, outline), name=name)
for name, outline in land_shapes.items():
    document.area(LandscapeKind.LAND, map(world_point, outline), name=name)
for name, outline in concrete_shapes.items():
    document.area(LandscapeKind.CONCRETE, map(world_point, outline), name=name)
for name, outline in vegetation_shapes.items():
    document.vegetation(name, "forest", map(world_point, outline))
for name, (height, outline) in terrain_shapes.items():
    document.terrain(name, round(height, 4), map(world_point, outline))
for name, (height, outline) in building_shapes.items():
    document.building(name, round(height, 4), map(world_point, outline))
document.extend(road_statements)

for reference, arms in connection_arms.items():
    sides = {side for side, _ in arms}
    if len(arms) not in (2, 3, 4) or len(sides) != len(arms):
        continue
    if len(arms) == 2 and sides in ({"north", "south"}, {"east", "west"}):
        continue
    kinds = {kind for _, kind in arms}
    if len(arms) == 2 and kinds == {"arterial"}:
        continue
    if kinds == {"local"} or kinds == {"arterial"}:
        document.intersection(reference, next(iter(kinds)), len(arms))
    elif kinds == {"local", "arterial"}:
        overrides = {IntersectionSide(side): "local" for side, kind in arms if kind == "local"}
        document.intersection(reference, "arterial", len(arms), overrides=overrides)

for index, placement in enumerate(placements["i95"]):
    if index == 1:
        document.grade(placement, 0, 0.24)
    elif index in (2, 3):
        document.elevation(placement, 0.24)
    elif index == 4:
        document.grade(placement, 0.24, 0)
for index, placement in enumerate(placements["i10"]):
    if index == 7:
        document.grade(placement, 0, 0.18)
    elif index == 8:
        document.elevation(placement, 0.18)
    elif index == 9:
        document.grade(placement, 0.18, 0)
flyovers = {
    "i95_north_return": [(0, 0.16), (0.16, 0.16), (0.16, 0)],
    "west_ramp_e": [(0, 0.18), (0.18, 0.18), (0.18, 0)],
    "west_ramp_to_grid": [(0, 0.16), (0.16, 0.16), (0.16, 0)],
    "east_ramp_n": [(0, 0.18), (0.18, 0.18), (0.18, 0.18)],
    "east_ramp_s": [(0, 0.18), (0.18, 0.18), (0.18, 0)],
    "east_ramp_loop": [(0.18, 0.18)] * 4 + [(0.18, 0)],
}
for name, grades in flyovers.items():
    if len(placements[name]) != len(grades):
        raise ValueError(f"{name} has {len(placements[name])} road pieces but {len(grades)} grades")
    for placement, (start, end) in zip(placements[name], grades):
        document.grade(placement, start, end)

Path("examples/target_map.road").write_text(document.source)
print(f"wrote {serial + mouth_count} road segments and {len(points)} junction points")
