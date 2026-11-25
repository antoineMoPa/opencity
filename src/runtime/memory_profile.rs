//! Opt-in ownership accounting. Shared allocation slack stays at city level.
use super::*;
use std::mem::size_of;

pub struct BuildingDetailMemory {
    pub feature: usize,
    pub scenery_vertices: usize,
    pub prop_vertices: usize,
    pub prop_capacity_bytes: usize,
    pub prop_meshes: usize,
}
#[derive(Default)]
pub struct BuildingMemory {
    pub feature: usize,
    pub triangles: usize,
    pub proxy_triangles: usize,
    pub cell_references: usize,
    pub support_references: usize,
    pub building_references: usize,
    pub volume_bytes: usize,
    pub recipe_bytes: usize,
    pub recipe_references: usize,
    pub surfaces: Vec<(SurfaceKind, usize)>,
}
pub struct CityMemory {
    pub pointer_bytes: usize,
    pub triangle_bytes: usize,
    pub triangle_count: usize,
    pub triangle_capacity: usize,
    pub cell_reference_count: usize,
    pub cell_reference_capacity: usize,
    pub support_reference_count: usize,
    pub support_reference_capacity: usize,
    pub buildings: Vec<BuildingMemory>,
    pub recipe_count: usize,
    pub recipe_bytes: usize,
    pub recipe_index_bytes: usize,
    pub building_mesh_cache_bytes: usize,
    pub building_mesh_cache_budget: usize,
}
impl City {
    pub fn building_memory(&self) -> CityMemory {
        let mut buildings = Vec::new();
        let mut owners = vec![u32::MAX; self.triangles.len()];
        let mut feature_rows = vec![u32::MAX; self.landscape.len()];
        for (feature, range) in &self.building_triangle_ranges {
            let slot = u32::try_from(buildings.len()).expect("building profile row count");
            feature_rows[*feature] = slot;
            owners[range.clone()].fill(slot);
            let mut row = BuildingMemory {
                feature: *feature,
                triangles: range.len(),
                ..Default::default()
            };
            for t in &self.triangles[range.clone()] {
                row.proxy_triangles += usize::from(t.model_proxy);
                if let Some((_, count)) = row.surfaces.iter_mut().find(|(kind, _)| *kind == t.kind)
                {
                    *count += 1;
                } else {
                    row.surfaces.push((t.kind, 1));
                }
            }
            row.volume_bytes = self.building_volumes.get(feature).map_or(0, |v| {
                v.capacity() * size_of::<architecture::BuildingVolume>()
            });
            row.recipe_bytes = usize::from(self.procedural_buildings.contains_key(feature))
                * size_of::<super::procedural::Building>();
            buildings.push(row);
        }
        for indices in self.cells.values() {
            for &id in indices {
                if owners[id] != u32::MAX {
                    buildings[owners[id] as usize].cell_references += 1;
                }
            }
        }
        for indices in self.support_cells.values() {
            for &id in indices {
                if owners[id] != u32::MAX {
                    buildings[owners[id] as usize].support_references += 1;
                }
            }
        }
        for indices in self.building_cells.values() {
            for &id in indices {
                if feature_rows[id] != u32::MAX {
                    buildings[feature_rows[id] as usize].building_references += 1;
                }
            }
        }
        for ids in self.procedural_cells.values() {
            for &id in ids {
                if feature_rows[id] != u32::MAX {
                    buildings[feature_rows[id] as usize].recipe_references += 1;
                }
            }
        }
        CityMemory {
            recipe_count: self.procedural_buildings.len(),
            recipe_bytes: self.procedural_buildings.len()
                * size_of::<super::procedural::Building>(),
            recipe_index_bytes: self
                .procedural_cells
                .values()
                .map(Vec::capacity)
                .sum::<usize>()
                * size_of::<usize>(),
            building_mesh_cache_bytes: self
                .building_meshes
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .bytes(),
            building_mesh_cache_budget: super::procedural::MESH_BUDGET,
            pointer_bytes: size_of::<usize>(),
            triangle_bytes: size_of::<Triangle>(),
            triangle_count: self.triangles.len(),
            triangle_capacity: self.triangles.capacity(),
            cell_reference_count: self.cells.values().map(Vec::len).sum(),
            cell_reference_capacity: self.cells.values().map(Vec::capacity).sum(),
            support_reference_count: self.support_cells.values().map(Vec::len).sum(),
            support_reference_capacity: self.support_cells.values().map(Vec::capacity).sum(),
            buildings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ownership_distinguishes_model_proxies_and_procedural_geometry() {
        let source = "plain=building(0.1,-0.2,-0.2,0,-0.2,0,0,-0.2,0);
            imported=building(0.1,\"model.glb\",1,1,1.2,1,1.2,1.2,1,1.2);
            tower=building(0.4,{material:glass,family:podium,floor_height:0.0125},2,0,2.3,0,2.3,0.2,2,0.2);";
        let city = City::from_program(&crate::parse(source).unwrap(), None).unwrap();
        let report = city.building_memory();
        let named = |name| {
            report
                .buildings
                .iter()
                .find(|b| city.landscape[b.feature].name.as_deref() == Some(name))
                .unwrap()
        };
        assert_eq!(named("plain").triangles, 10);
        assert_eq!(named("plain").proxy_triangles, 0);
        assert_eq!(named("imported").triangles, 10);
        assert_eq!(named("imported").proxy_triangles, 10);
        let tower = named("tower");
        assert_eq!(tower.proxy_triangles, 0);
        assert!(tower.volume_bytes > 0);
        assert_eq!(tower.triangles, 0);
        assert!(tower.recipe_bytes > 0);
        let visible = city
            .mesh_with_external_models(Region::WholeMap, 0.04)
            .unwrap();
        assert_eq!(
            visible.vertices.len() / 3,
            named("plain").triangles
                + self::architecture::BuildingRecipe::from_feature(&city.landscape[tower.feature])
                    .unwrap()
                    .mesh()
                    .vertices
                    .len()
                    / 3
        );
        assert_eq!(
            report.buildings.iter().map(|b| b.triangles).sum::<usize>(),
            report.triangle_count
        );
        assert!(tower.recipe_references > 0);
        assert_eq!(tower.cell_references, 0);
        assert_eq!(report.recipe_count, 1);
    }
}
