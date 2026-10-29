//! Shared deterministic zoning expansion. Preview and games consume these exact
//! building calls; clients may change appearance but must retain placement/height.
use super::scaffold_frontage as frontage;
use super::scaffold_residential as residential;
use super::City;
use crate::{LandscapeFeature, Program};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaffoldRoof {
    Flat,
    Gable,
    Hipped,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BuildingScaffold {
    pub id: String,
    pub feature: LandscapeFeature,
    pub roof: ScaffoldRoof,
}
/// Stable generated volume records, useful without any game-specific decoration.
pub fn buildings(program: &Program) -> Vec<BuildingScaffold> {
    crate::project_landscape_features(program)
        .features
        .iter()
        .filter_map(BuildingScaffold::from_feature)
        .collect()
}
/// Expand with caller-owned elevation loading. `compile` must compile raw calls
/// (City::from_program), not recursively invoke this function. The same routine
/// powers the editor, native game and embedded/browser game.
pub fn expand(
    source: &Program,
    mut compile: impl FnMut(&Program) -> Result<City, String>,
) -> Result<(Program, City), String> {
    let (program, mut city, changed) = expand_stages(source, &mut compile)?;
    if changed {
        city = compile(&program)?;
    }
    Ok((program, city))
}
/// Generate building calls without compiling the final snapshot. Editors only
/// need these calls; games can dress them before their final City compilation.
pub fn expand_program(
    source: &Program,
    mut compile: impl FnMut(&Program) -> Result<City, String>,
) -> Result<Program, String> {
    expand_stages(source, &mut compile).map(|(program, _, _)| program)
}

fn expand_stages(
    source: &Program,
    compile: &mut impl FnMut(&Program) -> Result<City, String>,
) -> Result<(Program, City, bool), String> {
    let strips = frontage::reservations(source)?;
    let source = frontage::prepare(source, &strips);
    let (mut program, areas) = residential::prepare(&source)?;
    let mut city = compile(&program)?;
    let mut changed = frontage::populate(&mut program, &city) > 0;
    // Legacy residential placement needs shops as obstacles. Explicit frontage
    // zones are populated together and need no intermediate recompilation.
    if changed && !areas.is_empty() {
        city = compile(&program)?;
        changed = false;
    }
    if !areas.is_empty() && residential::populate(&mut program, &city, &areas) > 0 {
        changed = true;
    }
    Ok((program, city, changed))
}
#[doc(hidden)]
pub use super::scaffold_frontage::{contains, overlap, prepare, reservations, strip_footprints};

impl BuildingScaffold {
    pub fn from_feature(feature: &LandscapeFeature) -> Option<Self> {
        let id = feature.name.as_ref()?;
        if !id.starts_with("generated_shop_") && !id.starts_with("generated_home_") {
            return None;
        }
        let roof = match frontage::seed(id) % 6 {
            0 => ScaffoldRoof::Gable,
            1 => ScaffoldRoof::Hipped,
            _ => ScaffoldRoof::Flat,
        };
        Some(Self {
            id: id.clone(),
            feature: feature.clone(),
            roof,
        })
    }
    /// Closed, low-poly neutral volume. Roofs stay inside the allocated height;
    /// games may dress or replace this skin while keeping the same proportions.
    pub fn mesh(&self) -> super::Mesh {
        let mut mesh = super::Mesh::default();
        if self.feature.points.len() != 4 {
            return mesh;
        }
        let p = &self.feature.points;
        let h = self.feature.height.unwrap_or(0.);
        let rise = if self.roof == ScaffoldRoof::Flat {
            0.
        } else {
            (h * 0.18).min(0.025)
        };
        let z = self.feature.elevation;
        let b: [[f32; 3]; 4] = std::array::from_fn(|i| [p[i][0], p[i][1], z]);
        let t: [[f32; 3]; 4] = std::array::from_fn(|i| [p[i][0], p[i][1], z + h - rise]);
        let color = [0.55, 0.57, 0.56];
        let mut triangle =
            |a, b, c| mesh.triangle([a, b, c], color, super::SurfaceKind::Building, [z, 0., 0.]);
        triangle(b[2], b[1], b[0]);
        triangle(b[3], b[2], b[0]);
        for i in 0..4 {
            let j = (i + 1) % 4;
            triangle(b[i], b[j], t[j]);
            triangle(b[i], t[j], t[i]);
        }
        match self.roof {
            ScaffoldRoof::Flat => {
                triangle(t[0], t[1], t[2]);
                triangle(t[0], t[2], t[3]);
            }
            ScaffoldRoof::Gable => {
                let a = [(p[0][0] + p[3][0]) * 0.5, (p[0][1] + p[3][1]) * 0.5, z + h];
                let b = [(p[1][0] + p[2][0]) * 0.5, (p[1][1] + p[2][1]) * 0.5, z + h];
                triangle(t[0], t[1], b);
                triangle(t[0], b, a);
                triangle(a, b, t[2]);
                triangle(a, t[2], t[3]);
                triangle(t[0], a, t[3]);
                triangle(t[1], t[2], b);
            }
            ScaffoldRoof::Hipped => {
                let top = [
                    p.iter().map(|p| p[0]).sum::<f32>() / 4.,
                    p.iter().map(|p| p[1]).sum::<f32>() / 4.,
                    z + h,
                ];
                for i in 0..4 {
                    triangle(t[i], t[(i + 1) % 4], top);
                }
            }
        }
        mesh
    }
}
