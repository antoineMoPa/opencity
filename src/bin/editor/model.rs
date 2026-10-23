//! Embedded GLB triangle meshes used by building placements.

use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub struct ModelVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct ModelMesh {
    pub triangles: Vec<[ModelVertex; 3]>,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

type Matrix = [[f32; 4]; 4];
const IDENTITY: Matrix = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn number(value: &Value) -> Option<f32> {
    value
        .as_f64()
        .map(|value| value as f32)
        .filter(|value| value.is_finite())
}

fn index(value: &Value) -> Option<usize> {
    value.as_u64().and_then(|value| usize::try_from(value).ok())
}

fn words(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn multiply(a: Matrix, b: Matrix) -> Matrix {
    let mut result = [[0.0; 4]; 4];
    for column in 0..4 {
        for row in 0..4 {
            for axis in 0..4 {
                result[column][row] += a[axis][row] * b[column][axis];
            }
        }
    }
    result
}

fn transform(matrix: Matrix, point: [f32; 3]) -> [f32; 3] {
    [
        matrix[0][0] * point[0] + matrix[1][0] * point[1] + matrix[2][0] * point[2] + matrix[3][0],
        matrix[0][1] * point[0] + matrix[1][1] * point[1] + matrix[2][1] * point[2] + matrix[3][1],
        matrix[0][2] * point[0] + matrix[1][2] * point[1] + matrix[2][2] * point[2] + matrix[3][2],
    ]
}

fn node_matrix(node: &Value) -> Result<Matrix, String> {
    if let Some(values) = node["matrix"].as_array() {
        if values.len() != 16 {
            return Err("node matrix needs 16 values".into());
        }
        let mut result = [[0.0; 4]; 4];
        for column in 0..4 {
            for row in 0..4 {
                result[column][row] =
                    number(&values[column * 4 + row]).ok_or("invalid node matrix")?;
            }
        }
        return Ok(result);
    }
    let values = |key: &str, fallback: [f32; 3]| -> Result<[f32; 3], String> {
        let Some(source) = node[key].as_array() else {
            return Ok(fallback);
        };
        if source.len() != 3 {
            return Err(format!("invalid node {key}"));
        }
        Ok([
            number(&source[0]).ok_or("invalid transform")?,
            number(&source[1]).ok_or("invalid transform")?,
            number(&source[2]).ok_or("invalid transform")?,
        ])
    };
    let translation = values("translation", [0.0; 3])?;
    let scale = values("scale", [1.0; 3])?;
    let rotation = if let Some(source) = node["rotation"].as_array() {
        if source.len() != 4 {
            return Err("invalid node rotation".into());
        }
        [
            number(&source[0]).ok_or("invalid rotation")?,
            number(&source[1]).ok_or("invalid rotation")?,
            number(&source[2]).ok_or("invalid rotation")?,
            number(&source[3]).ok_or("invalid rotation")?,
        ]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };
    let [x, y, z, w] = rotation;
    let mut result = IDENTITY;
    result[0] = [
        (1.0 - 2.0 * (y * y + z * z)) * scale[0],
        (2.0 * (x * y + z * w)) * scale[0],
        (2.0 * (x * z - y * w)) * scale[0],
        0.0,
    ];
    result[1] = [
        (2.0 * (x * y - z * w)) * scale[1],
        (1.0 - 2.0 * (x * x + z * z)) * scale[1],
        (2.0 * (y * z + x * w)) * scale[1],
        0.0,
    ];
    result[2] = [
        (2.0 * (x * z + y * w)) * scale[2],
        (2.0 * (y * z - x * w)) * scale[2],
        (1.0 - 2.0 * (x * x + y * y)) * scale[2],
        0.0,
    ];
    result[3] = [translation[0], translation[1], translation[2], 1.0];
    Ok(result)
}

struct Accessor<'a> {
    bin: &'a [u8],
    start: usize,
    stride: usize,
    count: usize,
    component: usize,
    components: usize,
    scaled: bool,
}

impl<'a> Accessor<'a> {
    fn new(root: &Value, bin: &'a [u8], id: usize) -> Result<Self, String> {
        let accessor = root["accessors"].get(id).ok_or("missing accessor")?;
        let view_id =
            index(&accessor["bufferView"]).ok_or("sparse or missing accessor buffer view")?;
        let view = root["bufferViews"]
            .get(view_id)
            .ok_or("missing buffer view")?;
        if index(&view["buffer"]) != Some(0) {
            return Err("GLB accessor uses an external buffer".into());
        }
        let component = index(&accessor["componentType"]).ok_or("missing component type")?;
        let component_size = match component {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return Err("unsupported accessor component".into()),
        };
        let components = match accessor["type"].as_str() {
            Some("SCALAR") => 1,
            Some("VEC3") => 3,
            Some("VEC4") => 4,
            _ => return Err("unsupported accessor shape".into()),
        };
        let count = index(&accessor["count"]).ok_or("missing accessor count")?;
        let start = index(&view["byteOffset"])
            .unwrap_or(0)
            .checked_add(index(&accessor["byteOffset"]).unwrap_or(0))
            .ok_or("accessor offset overflow")?;
        let stride = index(&view["byteStride"]).unwrap_or(component_size * components);
        if stride < component_size * components {
            return Err("accessor stride is too short".into());
        }
        let end = if count == 0 {
            start
        } else {
            start
                .checked_add(
                    (count - 1)
                        .checked_mul(stride)
                        .ok_or("accessor length overflow")?,
                )
                .and_then(|at| at.checked_add(component_size * components))
                .ok_or("accessor length overflow")?
        };
        if end > bin.len() {
            return Err("accessor extends outside GLB binary data".into());
        }
        Ok(Self {
            bin,
            start,
            stride,
            count,
            component,
            components,
            scaled: accessor["normalized"].as_bool().unwrap_or(false),
        })
    }

    fn scalar(&self, item: usize, axis: usize) -> Option<f32> {
        if item >= self.count || axis >= self.components {
            return None;
        }
        let width = match self.component {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            _ => 4,
        };
        let at = self
            .start
            .checked_add(item.checked_mul(self.stride)?)?
            .checked_add(axis * width)?;
        let bytes = self.bin.get(at..at + width)?;
        let value = match self.component {
            5120 => {
                let value = bytes[0] as i8 as f32;
                if self.scaled {
                    (value / 127.0).max(-1.0)
                } else {
                    value
                }
            }
            5121 => {
                let value = bytes[0] as f32;
                if self.scaled {
                    value / 255.0
                } else {
                    value
                }
            }
            5122 => {
                let value = i16::from_le_bytes(bytes.try_into().ok()?) as f32;
                if self.scaled {
                    (value / 32767.0).max(-1.0)
                } else {
                    value
                }
            }
            5123 => {
                let value = u16::from_le_bytes(bytes.try_into().ok()?) as f32;
                if self.scaled {
                    value / 65535.0
                } else {
                    value
                }
            }
            5125 => u32::from_le_bytes(bytes.try_into().ok()?) as f32,
            5126 => f32::from_le_bytes(bytes.try_into().ok()?),
            _ => return None,
        };
        value.is_finite().then_some(value)
    }

    fn position(&self, item: usize) -> Option<[f32; 3]> {
        if self.components != 3 || self.component != 5126 {
            return None;
        }
        Some([
            self.scalar(item, 0)?,
            self.scalar(item, 1)?,
            self.scalar(item, 2)?,
        ])
    }

    fn color(&self, item: usize) -> Option<[f32; 4]> {
        if !matches!(self.components, 3 | 4) {
            return None;
        }
        Some([
            self.scalar(item, 0)?,
            self.scalar(item, 1)?,
            self.scalar(item, 2)?,
            if self.components == 4 {
                self.scalar(item, 3)?
            } else {
                1.0
            },
        ])
    }

    fn indices(&self) -> Option<Vec<usize>> {
        if self.components != 1 || !matches!(self.component, 5121 | 5123 | 5125) {
            return None;
        }
        (0..self.count)
            .map(|item| self.scalar(item, 0).map(|value| value as usize))
            .collect()
    }
}

fn visit_node(
    root: &Value,
    bin: &[u8],
    node_id: usize,
    parent: Matrix,
    depth: usize,
    mesh: &mut ModelMesh,
) -> Result<(), String> {
    if depth > 64 {
        return Err("GLB node tree is too deep".into());
    }
    let node = root["nodes"].get(node_id).ok_or("missing GLB node")?;
    let matrix = multiply(parent, node_matrix(node)?);
    if let Some(mesh_id) = index(&node["mesh"]) {
        let source = root["meshes"].get(mesh_id).ok_or("missing GLB mesh")?;
        if let Some(primitives) = source["primitives"].as_array() {
            for primitive in primitives {
                if index(&primitive["mode"]).unwrap_or(4) != 4 {
                    continue;
                }
                let Some(position_id) = index(&primitive["attributes"]["POSITION"]) else {
                    continue;
                };
                let positions = Accessor::new(root, bin, position_id)?;
                let colors = index(&primitive["attributes"]["COLOR_0"])
                    .map(|id| Accessor::new(root, bin, id))
                    .transpose()?;
                let indices = if let Some(id) = index(&primitive["indices"]) {
                    Accessor::new(root, bin, id)?
                        .indices()
                        .ok_or("unsupported GLB indices")?
                } else {
                    (0..positions.count).collect()
                };
                let tint = index(&primitive["material"])
                    .and_then(|id| root["materials"].get(id))
                    .and_then(|material| {
                        material["pbrMetallicRoughness"]["baseColorFactor"].as_array()
                    })
                    .and_then(|values| {
                        if values.len() != 4 {
                            return None;
                        }
                        Some([
                            number(&values[0])?,
                            number(&values[1])?,
                            number(&values[2])?,
                            number(&values[3])?,
                        ])
                    })
                    .unwrap_or([1.0; 4]);
                for triangle in indices.chunks_exact(3) {
                    let vertices: Option<Vec<_>> = triangle
                        .iter()
                        .map(|&id| {
                            let position = transform(matrix, positions.position(id)?);
                            let color = colors
                                .as_ref()
                                .and_then(|colors| colors.color(id))
                                .unwrap_or([1.0; 4]);
                            Some(ModelVertex {
                                position,
                                color: [
                                    color[0] * tint[0],
                                    color[1] * tint[1],
                                    color[2] * tint[2],
                                    color[3] * tint[3],
                                ],
                            })
                        })
                        .collect();
                    let Some(vertices) = vertices else {
                        return Err("invalid GLB mesh vertex".into());
                    };
                    for vertex in &vertices {
                        for axis in 0..3 {
                            mesh.min[axis] = mesh.min[axis].min(vertex.position[axis]);
                            mesh.max[axis] = mesh.max[axis].max(vertex.position[axis]);
                        }
                    }
                    mesh.triangles.push([vertices[0], vertices[1], vertices[2]]);
                }
            }
        }
    }
    if let Some(children) = node["children"].as_array() {
        for child in children {
            visit_node(
                root,
                bin,
                index(child).ok_or("invalid child node")?,
                matrix,
                depth + 1,
                mesh,
            )?;
        }
    }
    Ok(())
}

pub fn parse_glb(bytes: &[u8]) -> Result<ModelMesh, String> {
    if bytes.len() < 20
        || &bytes[..4] != b"glTF"
        || words(bytes, 4) != Some(2)
        || words(bytes, 8) != Some(bytes.len() as u32)
    {
        return Err("expected a complete GLB 2.0 file".into());
    }
    let mut at = 12;
    let mut json = None;
    let mut bin = None;
    while at + 8 <= bytes.len() {
        let length = words(bytes, at).ok_or("invalid GLB chunk")? as usize;
        let kind = bytes.get(at + 4..at + 8).ok_or("invalid GLB chunk")?;
        at += 8;
        let chunk = bytes
            .get(at..at.checked_add(length).ok_or("GLB chunk overflow")?)
            .ok_or("GLB chunk exceeds file")?;
        if kind == b"JSON" {
            json = Some(chunk);
        }
        if kind == b"BIN\0" {
            bin = Some(chunk);
        }
        at += length;
    }
    let root: Value = serde_json::from_slice(json.ok_or("GLB has no JSON chunk")?)
        .map_err(|error| error.to_string())?;
    let bin = bin.ok_or("GLB has no embedded binary chunk")?;
    let scene_id = index(&root["scene"]).unwrap_or(0);
    let scene = root["scenes"].get(scene_id).ok_or("GLB has no scene")?;
    let mut mesh = ModelMesh {
        triangles: Vec::new(),
        min: [f32::INFINITY; 3],
        max: [f32::NEG_INFINITY; 3],
    };
    for node in scene["nodes"].as_array().ok_or("GLB scene has no nodes")? {
        visit_node(
            &root,
            bin,
            index(node).ok_or("invalid root node")?,
            IDENTITY,
            0,
            &mut mesh,
        )?;
    }
    if mesh.triangles.is_empty() {
        return Err("GLB scene has no triangle meshes".into());
    }
    if mesh
        .min
        .iter()
        .chain(mesh.max.iter())
        .any(|value| !value.is_finite())
    {
        return Err("GLB has non-finite mesh coordinates".into());
    }
    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_exported_scene_mesh() {
        let program = opencity::parse("building(0.5, 0, 0, 1, 0, 1, 1, 0, 1);").unwrap();
        let bytes = super::super::glb::export(&program);
        let mesh = parse_glb(&bytes).unwrap();
        assert!(!mesh.triangles.is_empty());
        assert_eq!(mesh.max[1], 0.5);
    }

    #[test]
    fn applies_gltf_node_translation_and_scale() {
        let node = serde_json::json!({"translation": [2, 3, 4], "scale": [2, 2, 2]});
        assert_eq!(
            transform(node_matrix(&node).unwrap(), [1.0, 1.0, 1.0]),
            [4.0, 5.0, 6.0]
        );
    }
}
