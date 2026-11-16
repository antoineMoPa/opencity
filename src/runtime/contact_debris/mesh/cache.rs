//! One bounded pavement bake shared by successive reload workers. Building geometry
//! is not an input to contact clipping, so editing it does not invalidate this cache.
use super::*;
use std::sync::{Arc, Mutex};

type Replacements = HashMap<usize, Vec<Vertex>>;
type Input = Vec<[[u32; 13]; 3]>;
const MAX_BYTES: usize = 64 * 1024 * 1024;
static CACHE: Mutex<Option<Entry>> = Mutex::new(None);

struct Entry {
    input: Input,
    output: Arc<Replacements>,
}
pub(super) struct Prepared {
    pub indices: Vec<usize>,
    pub output: Arc<Replacements>,
}

fn indices(mesh: &Mesh) -> Vec<usize> {
    mesh.vertices
        .chunks_exact(3)
        .enumerate()
        .filter(|(_, t)| {
            let kind = t[0].surface[0];
            kind == SurfaceKind::Sidewalk as u32 as f32
                || kind == SurfaceKind::Asphalt as u32 as f32
                || (kind == SurfaceKind::Prop as u32 as f32 && t[0].surface[3] == -2.)
        })
        .map(|(id, _)| id)
        .collect()
}
fn input(mesh: &Mesh, indices: &[usize]) -> Input {
    indices
        .iter()
        .map(|id| {
            let t = &mesh.vertices[id * 3..id * 3 + 3];
            std::array::from_fn(|i| {
                let v = t[i];
                [
                    v.position[0],
                    v.position[1],
                    v.position[2],
                    v.normal[0],
                    v.normal[1],
                    v.normal[2],
                    v.color[0],
                    v.color[1],
                    v.color[2],
                    v.surface[0],
                    v.surface[1],
                    v.surface[2],
                    v.surface[3],
                ]
                .map(f32::to_bits)
            })
        })
        .collect()
}

pub(super) fn replacements(mesh: &Mesh, build: impl FnOnce(&Mesh) -> Replacements) -> Prepared {
    prepare(mesh, build, &CACHE)
}
fn prepare(
    mesh: &Mesh,
    build: impl FnOnce(&Mesh) -> Replacements,
    cache: &Mutex<Option<Entry>>,
) -> Prepared {
    let indices = indices(mesh);
    let input = input(mesh, &indices);
    if let Some(hit) = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .filter(|entry| entry.input == input)
    {
        return Prepared {
            indices,
            output: Arc::clone(&hit.output),
        };
    }
    // Never hold the cache lock while clipping; concurrent map preparations remain independent.
    // Ordinals refer to relevant triangles, not whole-map offsets. Adding a building
    // before a retaining wall must not invalidate unchanged pavement or shift its edits.
    let pavement = Mesh {
        vertices: indices
            .iter()
            .flat_map(|id| mesh.vertices[id * 3..id * 3 + 3].iter().copied())
            .collect(),
    };
    let output = Arc::new(build(&pavement));
    let bytes = input.capacity() * std::mem::size_of::<[[u32; 13]; 3]>()
        + output.capacity() * std::mem::size_of::<(usize, Vec<Vertex>)>()
        + output
            .values()
            .map(|v| v.capacity() * std::mem::size_of::<Vertex>())
            .sum::<usize>();
    if bytes <= MAX_BYTES {
        *cache.lock().unwrap_or_else(|e| e.into_inner()) = Some(Entry {
            input,
            output: Arc::clone(&output),
        });
    }
    Prepared { indices, output }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn building_edits_reuse_contacts_but_pavement_edits_invalidate_them() {
        let cache = Mutex::new(None);
        let mut mesh = super::super::tests::specimen(0.0015, 0.);
        let first = prepare(&mesh, build_replacements, &cache);
        assert!(!first.output.is_empty());
        let mut building = mesh.vertices[..3].to_vec();
        for v in &mut building {
            v.surface[0] = SurfaceKind::Facade as u32 as f32;
            v.position[2] += 1.;
        }
        mesh.vertices.splice(0..0, building);
        let reused = prepare(&mesh, |_| panic!("building edit rebaked pavement"), &cache);
        assert!(Arc::ptr_eq(&first.output, &reused.output));
        assert_eq!(
            reused.indices,
            first.indices.iter().map(|i| i + 1).collect::<Vec<_>>()
        );

        let original = mesh.clone();
        // Each field affects either clipping or the vertices emitted by the bake.
        for field in 0..4 {
            let baseline = prepare(&original, build_replacements, &cache);
            let mut changed = original.clone();
            let v = &mut changed.vertices[3];
            match field {
                0 => v.position[2] += 0.01,
                1 => v.normal[0] += 0.1,
                2 => v.color[0] += 0.1,
                _ => v.surface[1] += 0.1,
            }
            let fresh = prepare(&changed, build_replacements, &cache);
            assert!(!Arc::ptr_eq(&baseline.output, &fresh.output));
        }
    }
}
