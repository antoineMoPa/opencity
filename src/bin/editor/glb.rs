//! Binary glTF export of the road surface and elevated side walls.
use super::map::{self, Segment, WorldPoint};
use super::{egui, landscape_triangles};
use opencity::{
    project_landscape_features, LandscapeKind, Program, RoadDefinition, RoadElementKind, Statement,
};
use serde_json::json;

struct Geometry {
    positions: Vec<[f32; 3]>,
    colors: Vec<[u8; 4]>,
    indices: Vec<u32>,
    min: [f32; 3],
    max: [f32; 3],
}

impl Geometry {
    fn new() -> Self {
        Self {
            positions: Vec::new(),
            colors: Vec::new(),
            indices: Vec::new(),
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        }
    }

    fn vertex(&mut self, point: WorldPoint, height: f32, color: [u8; 4]) -> u32 {
        let position = [point.x, height, -point.y];
        for axis in 0..3 {
            self.min[axis] = self.min[axis].min(position[axis]);
            self.max[axis] = self.max[axis].max(position[axis]);
        }
        let index = self.positions.len() as u32;
        self.positions.push(position);
        self.colors.push(color);
        index
    }

    fn quad(&mut self, points: [(WorldPoint, f32); 4], color: [u8; 4]) {
        let first = self.positions.len() as u32;
        for (point, height) in points {
            self.vertex(point, height, color);
        }
        self.indices
            .extend_from_slice(&[first, first + 2, first + 1, first, first + 3, first + 2]);
    }
}

fn band_color(kind: RoadElementKind) -> [u8; 4] {
    let rgb = match kind {
        RoadElementKind::BuildingEdge => [120, 130, 135],
        RoadElementKind::Sidewalk => [194, 172, 145],
        RoadElementKind::BikeLane => [64, 145, 100],
        RoadElementKind::ParkingLane => [95, 112, 125],
        RoadElementKind::BusLane => [172, 86, 76],
        RoadElementKind::CarLane | RoadElementKind::Middle => [72, 83, 90],
        RoadElementKind::BusStop => [181, 143, 72],
        RoadElementKind::Verge => [86, 131, 76],
        RoadElementKind::Barrier => [164, 170, 166],
        RoadElementKind::TrainTrack => [109, 95, 80],
        RoadElementKind::TramTrack => [110, 122, 126],
        RoadElementKind::SubwayTrack => [80, 91, 123],
    };
    [rgb[0], rgb[1], rgb[2], 255]
}

fn offset(segment: &Segment, t: f32, width: f32) -> WorldPoint {
    let center = segment.point_at(t);
    let tangent = segment.tangent_at(t);
    let length = (tangent.x * tangent.x + tangent.y * tangent.y)
        .sqrt()
        .max(0.0001);
    WorldPoint {
        x: center.x - tangent.y / length * width,
        y: center.y + tangent.x / length * width,
    }
}

fn add_segment(mesh: &mut Geometry, segment: &Segment, road: Option<&RoadDefinition>) {
    let bands: Vec<_> = road
        .map(|road| {
            road.elements
                .iter()
                .map(|element| {
                    (
                        map::element_world_width(element.kind),
                        band_color(element.kind),
                    )
                })
                .collect()
        })
        .unwrap_or_else(|| vec![(0.16, [100, 110, 115, 255])]);
    let total: f32 = bands.iter().map(|(width, _)| width).sum();
    let mut left = -total / 2.0;
    for (width, color) in bands {
        let right = left + width;
        for step in 0..24 {
            let t0 = step as f32 / 24.0;
            let t1 = (step + 1) as f32 / 24.0;
            let z0 = segment.height_at(t0);
            let z1 = segment.height_at(t1);
            mesh.quad(
                [
                    (offset(segment, t0, left), z0),
                    (offset(segment, t0, right), z0),
                    (offset(segment, t1, right), z1),
                    (offset(segment, t1, left), z1),
                ],
                color,
            );
            if z0 > 0.001 || z1 > 0.001 {
                for edge in [left, right] {
                    let side = [color[0] / 2, color[1] / 2, color[2] / 2, 255];
                    mesh.quad(
                        [
                            (offset(segment, t0, edge), 0.0),
                            (offset(segment, t1, edge), 0.0),
                            (offset(segment, t1, edge), z1),
                            (offset(segment, t0, edge), z0),
                        ],
                        side,
                    );
                }
            }
        }
        left = right;
    }
}

fn pad(bytes: &mut Vec<u8>, multiple: usize, fill: u8) {
    while bytes.len() % multiple != 0 {
        bytes.push(fill);
    }
}

pub fn export(program: &Program) -> Vec<u8> {
    let projection = map::project(program);
    let mut mesh = Geometry::new();
    for segment in &projection.segments {
        let road = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == segment.road_name => Some(road),
                _ => None,
            });
        add_segment(&mut mesh, segment, road);
    }
    for intersection in map::intersections(program, &projection) {
        let width = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Road(road) if road.name == intersection.road_name => Some(
                    road.elements
                        .iter()
                        .map(|element| map::element_world_width(element.kind))
                        .sum::<f32>(),
                ),
                _ => None,
            })
            .unwrap_or(0.2);
        let half = width / 2.0;
        let p = intersection.position;
        mesh.quad(
            [
                (
                    WorldPoint {
                        x: p.x - half,
                        y: p.y - half,
                    },
                    0.0,
                ),
                (
                    WorldPoint {
                        x: p.x + half,
                        y: p.y - half,
                    },
                    0.0,
                ),
                (
                    WorldPoint {
                        x: p.x + half,
                        y: p.y + half,
                    },
                    0.0,
                ),
                (
                    WorldPoint {
                        x: p.x - half,
                        y: p.y + half,
                    },
                    0.0,
                ),
            ],
            [72, 83, 90, 255],
        );
    }
    for feature in project_landscape_features(program).features {
        let color = match feature.kind {
            LandscapeKind::Land => [77, 128, 79, 255],
            LandscapeKind::Lake => [48, 116, 158, 255],
            LandscapeKind::River => [52, 130, 167, 255],
        };
        if feature.kind == LandscapeKind::River {
            for pair in feature.points.windows(2) {
                let dx = pair[1][0] - pair[0][0];
                let dy = pair[1][1] - pair[0][1];
                let length = (dx * dx + dy * dy).sqrt().max(0.0001);
                let nx = -dy / length * 0.06;
                let ny = dx / length * 0.06;
                mesh.quad(
                    [
                        (
                            WorldPoint {
                                x: pair[0][0] + nx,
                                y: pair[0][1] + ny,
                            },
                            feature.elevation,
                        ),
                        (
                            WorldPoint {
                                x: pair[0][0] - nx,
                                y: pair[0][1] - ny,
                            },
                            feature.elevation,
                        ),
                        (
                            WorldPoint {
                                x: pair[1][0] - nx,
                                y: pair[1][1] - ny,
                            },
                            feature.elevation,
                        ),
                        (
                            WorldPoint {
                                x: pair[1][0] + nx,
                                y: pair[1][1] + ny,
                            },
                            feature.elevation,
                        ),
                    ],
                    color,
                );
            }
        } else {
            let polygon: Vec<_> = feature
                .points
                .iter()
                .map(|point| egui::pos2(point[0], point[1]))
                .collect();
            for triangle in landscape_triangles(&polygon) {
                let start = mesh.positions.len() as u32;
                for point in triangle {
                    mesh.vertex(
                        WorldPoint {
                            x: point.x,
                            y: point.y,
                        },
                        feature.elevation,
                        color,
                    );
                }
                mesh.indices
                    .extend_from_slice(&[start, start + 2, start + 1]);
            }
        }
    }
    if mesh.positions.is_empty() {
        return Vec::new();
    }
    let mut bin = Vec::new();
    for position in &mesh.positions {
        for value in position {
            bin.extend_from_slice(&value.to_le_bytes());
        }
    }
    let position_length = bin.len();
    for color in &mesh.colors {
        bin.extend_from_slice(color);
    }
    let color_length = bin.len() - position_length;
    pad(&mut bin, 4, 0);
    let index_offset = bin.len();
    for index in &mesh.indices {
        bin.extend_from_slice(&index.to_le_bytes());
    }
    let index_length = bin.len() - index_offset;
    let document = json!({
        "asset": {"version": "2.0", "generator": "OpenCity"},
        "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0, "name": "OpenCity roads"}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "COLOR_0": 1},
            "indices": 2, "material": 0, "mode": 4}]}],
        "materials": [{"name": "Road surfaces", "doubleSided": true,
            "pbrMetallicRoughness": {"baseColorFactor": [1, 1, 1, 1], "metallicFactor": 0,
                "roughnessFactor": 1}}],
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": position_length, "target": 34962},
            {"buffer": 0, "byteOffset": position_length, "byteLength": color_length, "target": 34962},
            {"buffer": 0, "byteOffset": index_offset, "byteLength": index_length, "target": 34963}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": mesh.positions.len(), "type": "VEC3",
                "min": mesh.min, "max": mesh.max},
            {"bufferView": 1, "componentType": 5121, "normalized": true,
                "count": mesh.colors.len(), "type": "VEC4"},
            {"bufferView": 2, "componentType": 5125, "count": mesh.indices.len(), "type": "SCALAR"}
        ]
    });
    let mut json_bytes = serde_json::to_vec(&document).expect("gltf json serializes");
    pad(&mut json_bytes, 4, b' ');
    let total_length = 12 + 8 + json_bytes.len() + 8 + bin.len();
    let mut glb = Vec::with_capacity(total_length);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total_length as u32).to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json_bytes);
    glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(&bin);
    glb
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glb_contains_mesh_and_valid_chunks() {
        let program =
            opencity::parse("road street:\n    carlane;\nend;\nstreet(0,0,0.33,0,0.66,0,1,0);")
                .unwrap();
        let bytes = export(&program);
        assert_eq!(&bytes[..4], b"glTF");
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize,
            bytes.len()
        );
        let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_length]).unwrap();
        assert!(json["accessors"][0]["count"].as_u64().unwrap() > 0);
        assert!(json["accessors"][2]["count"].as_u64().unwrap() > 0);
    }
}
