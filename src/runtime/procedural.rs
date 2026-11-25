//! Render recipes stay compact; only a bounded set of expanded building meshes is cached.
use super::{architecture::BuildingRecipe, geometry, City, Mesh, Region, SurfaceKind, Vertex};
use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
};
pub(super) const MESH_BUDGET: usize = 4 * 1024 * 1024;
pub(super) struct Building {
    pub recipe: BuildingRecipe,
    pub bounds: [f32; 4],
}
#[derive(Default)]
pub(super) struct MeshCache {
    entries: VecDeque<(usize, Arc<Mesh>)>,
    bytes: usize,
}
impl MeshCache {
    fn get(&mut self, id: usize) -> Option<Arc<Mesh>> {
        let slot = self.entries.iter().position(|(key, _)| *key == id)?;
        let entry = self.entries.remove(slot).unwrap();
        let mesh = entry.1.clone();
        self.entries.push_back(entry);
        Some(mesh)
    }
    fn insert(&mut self, id: usize, mesh: Arc<Mesh>) -> Arc<Mesh> {
        if let Some(existing) = self.get(id) {
            return existing;
        }
        let cost = mesh.vertices.capacity() * std::mem::size_of::<Vertex>();
        if cost > MESH_BUDGET {
            return mesh;
        }
        while self.bytes + cost > MESH_BUDGET {
            let (_, old) = self.entries.pop_front().unwrap();
            self.bytes -= old.vertices.capacity() * std::mem::size_of::<Vertex>();
        }
        self.bytes += cost;
        self.entries.push_back((id, mesh.clone()));
        mesh
    }
    #[cfg(feature = "memory-profile")]
    pub(super) fn bytes(&self) -> usize {
        self.bytes
    }
}
impl City {
    fn procedural_mesh(&self, id: usize) -> Arc<Mesh> {
        if let Some(mesh) = self
            .building_meshes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
        {
            return mesh;
        }
        // Build outside the lock: callers may generate independent regions concurrently.
        let mut mesh = self.procedural_buildings[&id].recipe.mesh();
        mesh.vertices.shrink_to_fit();
        self.building_meshes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, Arc::new(mesh))
    }
    pub(super) fn append_procedural(&self, region: Region, mesh: &mut Mesh) {
        let mut ids = HashSet::new();
        match region {
            Region::WholeMap => ids.extend(self.procedural_buildings.keys().copied()),
            Region::Radius { center, radius } => {
                let b = self.bounds;
                let lo = geometry::cell([
                    (center[0] - radius).max(b[0]),
                    (center[1] - radius).max(b[1]),
                ]);
                let hi = geometry::cell([
                    (center[0] + radius).min(b[2]),
                    (center[1] + radius).min(b[3]),
                ]);
                for x in lo.0..=hi.0 {
                    for y in lo.1..=hi.1 {
                        if let Some(keys) = self.procedural_cells.get(&(x, y)) {
                            ids.extend(keys);
                        }
                    }
                }
            }
        }
        let mut ids: Vec<_> = ids.into_iter().collect();
        ids.sort_unstable();
        for id in ids {
            if !region.intersects_bounds(self.procedural_buildings[&id].bounds) {
                continue;
            }
            let source = self.procedural_mesh(id);
            for t in source.vertices.chunks_exact(3) {
                let points: [[f32; 3]; 3] = std::array::from_fn(|i| t[i].position);
                if !geometry::polygon_region(&points.map(|p| [p[0], p[1]]), region) {
                    continue;
                }
                // Match the compiled Triangle -> Mesh mapping, including flat normals,
                // per-corner baked colors and the facade/material UV conventions.
                let kind = if t[0].surface[0] == SurfaceKind::Masonry as u32 as f32 {
                    SurfaceKind::Masonry
                } else if t[0].surface[0] == SurfaceKind::Glazing as u32 as f32 {
                    SurfaceKind::Glazing
                } else if t[0].surface[0] == SurfaceKind::Facade as u32 as f32 {
                    SurfaceKind::Facade
                } else {
                    SurfaceKind::Prop
                };
                let facade = matches!(
                    kind,
                    SurfaceKind::Masonry | SurfaceKind::Glazing | SurfaceKind::Facade
                );
                let start = mesh.vertices.len();
                mesh.triangle(points, t[0].color, kind, [0., 0., t[0].surface[3]]);
                for (v, source) in mesh.vertices[start..].iter_mut().zip(t) {
                    v.color = source.color;
                    v.normal = t[0].normal;
                    if facade {
                        v.surface[1..3].copy_from_slice(&source.surface[1..3]);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn signature(mesh: &Mesh) -> Vec<[u32; 39]> {
        let mut result: Vec<_> = mesh
            .vertices
            .chunks_exact(3)
            .map(|t| {
                let mut bits = [0; 39];
                for (i, v) in t.iter().enumerate() {
                    for (j, value) in v
                        .position
                        .into_iter()
                        .chain(v.normal)
                        .chain(v.color)
                        .chain(v.surface)
                        .enumerate()
                    {
                        bits[i * 13 + j] = value.to_bits();
                    }
                }
                bits
            })
            .collect();
        result.sort_unstable();
        result
    }
    #[test]
    fn deferred_families_keep_exact_render_attributes_and_collision_volumes() {
        for family in crate::BuildingFamily::ALL {
            for points in ["0,0,0.3,0,0.3,0.2,0,0.2", "0,0,0,0.2,0.3,0.2,0.3,0"] {
                let source=format!("tower=building(0.42,{{material:glass,family:{},floor_height:0.0125,seed:42}},{});",family.keyword(),points);
                let city = City::from_program(&crate::parse(&source).unwrap(), None).unwrap();
                let building = city.procedural_buildings.values().next().unwrap();
                let (reference, volumes) = building.recipe.geometry();
                assert!(
                    city.triangles.is_empty(),
                    "render-only triangles remain resident"
                );
                assert!(city.building_meshes.lock().unwrap().entries.is_empty());
                assert_eq!(city.building_volumes[&0].len(), volumes.len());
                for (actual, expected) in city.building_volumes[&0].iter().zip(&volumes) {
                    // Public collision behavior must remain the same at every sampled deck.
                    for x in [-0.01, 0.01, 0.15, 0.29, 0.31] {
                        for y in [-0.01, 0.01, 0.1, 0.19, 0.21] {
                            for z in [0., 0.02, 0.2, 0.4] {
                                assert_eq!(
                                    actual.intersects([x, y], 0.005, z, z + 0.015),
                                    expected.intersects([x, y], 0.005, z, z + 0.015)
                                );
                            }
                        }
                    }
                }
                let render_bounds = geometry::bounds(
                    reference
                        .vertices
                        .iter()
                        .map(|v| [v.position[0], v.position[1]]),
                );
                assert_eq!(city.bounds, render_bounds);
                assert_eq!(building.bounds, render_bounds);
                assert_eq!(
                    signature(&city.mesh(Region::WholeMap, 0.04).unwrap()),
                    signature(&reference)
                );
                for center in [[-0.01, 0.01], [0.15, 0.1], [0.301, 0.201], [2., 2.]] {
                    let region = Region::Radius {
                        center,
                        radius: 0.04,
                    };
                    let expected = Mesh {
                        vertices: reference
                            .vertices
                            .chunks_exact(3)
                            .filter(|t| {
                                geometry::polygon_region(
                                    &std::array::from_fn::<_, 3, _>(|i| {
                                        [t[i].position[0], t[i].position[1]]
                                    }),
                                    region,
                                )
                            })
                            .flatten()
                            .copied()
                            .collect(),
                    };
                    assert_eq!(
                        signature(&city.mesh(region, 0.04).unwrap()),
                        signature(&expected),
                        "{family:?} {center:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn travel_evicts_cached_meshes_and_rebuilds_identical_geometry() {
        let mut source = String::new();
        for id in 0..20 {
            let x = id as f32 * 2.;
            source.push_str(&format!("b{id}=building(1.2,{{material:glass,family:podium,floor_height:0.0125,seed:42}}, {x},0,{},0,{},0.2,{x},0.2);",x+0.3,x+0.3));
        }
        let city = City::from_program(&crate::parse(&source).unwrap(), None).unwrap();
        let first = city
            .mesh(
                Region::Radius {
                    center: [0.15, 0.1],
                    radius: 0.1,
                },
                0.04,
            )
            .unwrap();
        for id in 1..20 {
            city.mesh(
                Region::Radius {
                    center: [id as f32 * 2. + 0.15, 0.1],
                    radius: 0.1,
                },
                0.04,
            )
            .unwrap();
            assert!(city.building_meshes.lock().unwrap().bytes <= MESH_BUDGET);
        }
        assert!(!city
            .building_meshes
            .lock()
            .unwrap()
            .entries
            .iter()
            .any(|(id, _)| *id == 0));
        let returned = city
            .mesh(
                Region::Radius {
                    center: [0.15, 0.1],
                    radius: 0.1,
                },
                0.04,
            )
            .unwrap();
        assert_eq!(signature(&first), signature(&returned));
        assert!(city.building_meshes.lock().unwrap().bytes <= MESH_BUDGET);
    }
}
