//! One sign visibility policy for OpenCity, previews, game drawing and physics.
use super::*;
use std::collections::HashMap;
impl Placement {
    /// Any support or panel touching a building's volume hides the whole assembly.
    pub fn sign_overlaps_building(&self, city: &City) -> bool {
        let Some(design) = self
            .sign
            .clone()
            .or_else(|| SignDesign::for_kind(self.kind))
        else {
            return false;
        };
        let transform = self.transform();
        design.profiles().iter().any(|profile| {
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for x in [-1., 1.] {
                for y in [-1., 1.] {
                    for z in [-1., 1.] {
                        let p = transform.transform_point3(
                            (profile.center + profile.half * Vec3::new(x, y, z)) / UNITS,
                        );
                        min = min.min(p);
                        max = max.max(p);
                    }
                }
            }
            let margin = Vec3::splat(0.0003); // 3 cm clearance prevents wall contact artifacts.
            city.building_bounds_overlap((min - margin).to_array(), (max + margin).to_array())
        })
    }
}
/// Stable source/member IDs survive filtering. Repeated coincident sign assemblies
/// render and collide once; source entries remain available for subsequent editing.
pub fn visible_placements(entries: &[Entry], city: &City) -> Vec<(usize, usize, Placement)> {
    let mut result = Vec::new();
    let mut seen = HashMap::<(i32, i32), Vec<Placement>>::new();
    for (entry, e) in entries.iter().enumerate() {
        for (index, p) in e.placements(city).unwrap_or_default() {
            if p.kind.is_road_sign() {
                if p.sign_overlaps_building(city) {
                    continue;
                }
                let tile = (
                    (p.position.x / 0.5).floor() as i32,
                    (p.position.y / 0.5).floor() as i32,
                );
                let matrix = p.transform();
                let duplicate = (-1..=1).any(|x| {
                    (-1..=1).any(|y| {
                        seen.get(&(tile.0 + x, tile.1 + y)).is_some_and(|items| {
                            items.iter().any(|other| {
                                (other.position - p.position).length_squared() < 1e-10
                                    && other.transform().abs_diff_eq(matrix, 1e-5)
                            })
                        })
                    })
                });
                if duplicate {
                    continue;
                }
                seen.entry(tile).or_default().push(p.clone());
            }
            result.push((entry, index, p));
        }
    }
    result
}
