//! Exact dry excavation footprints, independent of PNG resolution.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct TerrainCut {
    pub points: Vec<[f32; 2]>,
    pub height: f32,
    pub slope: [f32; 2],
    winding: f32,
    bounds: [f32; 4],
}

impl TerrainCut {
    pub fn from_program(program: &Program) -> Result<Vec<Self>, String> {
        let mut cuts = Vec::new();
        for statement in &program.statements {
            let Statement::Call(call) = statement else {
                continue;
            };
            if call.name != "terrain_cut" {
                continue;
            }
            let number = |arg: &Argument| {
                match arg {
                    Argument::Number(v) => v.parse::<f32>().ok().filter(|v| v.is_finite()),
                    _ => None,
                }
                .ok_or("terrain_cut requires finite numeric coordinates and heights")
            };
            let Some((first, mut args)) = call.args.split_first() else {
                return Err("terrain_cut requires height and a convex polygon".into());
            };
            let height = number(first)?;
            let mut slope = [0.; 2];
            if let Some(Argument::Object(options)) = args.first() {
                let mut seen = [false; 2];
                for (name, value) in options {
                    let axis = match name.as_str() {
                        "slope_x" => 0,
                        "slope_y" => 1,
                        _ => return Err(format!("Unknown terrain_cut option {name}")),
                    };
                    if seen[axis] {
                        return Err(format!("Duplicate terrain_cut option {name}"));
                    }
                    seen[axis] = true;
                    slope[axis] = number(value)?;
                }
                args = &args[1..];
            }
            if args.len() < 6 || args.len() % 2 != 0 {
                return Err("terrain_cut requires at least three coordinate pairs".into());
            }
            let points = args
                .chunks_exact(2)
                .map(|p| Ok([number(&p[0])?, number(&p[1])?]))
                .collect::<Result<Vec<_>, String>>()?;
            let area: f32 = (0..points.len())
                .map(|i| {
                    let a = points[i];
                    let b = points[(i + 1) % points.len()];
                    a[0] * b[1] - a[1] * b[0]
                })
                .sum();
            if area.abs() < 1e-10 {
                return Err("terrain_cut polygon has no area".into());
            }
            let winding = area.signum();
            for i in 0..points.len() {
                let a = points[i];
                let b = points[(i + 1) % points.len()];
                if (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-7
                    || points.iter().any(|p| winding * cross(a, b, *p) < -1e-7)
                {
                    return Err("terrain_cut polygon must be convex with distinct edges".into());
                }
            }
            let bounds = points.iter().fold(
                [
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ],
                |b, p| {
                    [
                        b[0].min(p[0]),
                        b[1].min(p[1]),
                        b[2].max(p[0]),
                        b[3].max(p[1]),
                    ]
                },
            );
            let cut = Self {
                points,
                height,
                slope,
                winding,
                bounds,
            };
            if cut.points.iter().any(|p| !cut.height_at(*p).is_finite()) {
                return Err("terrain_cut plane exceeds finite heights".into());
            }
            cuts.push(cut);
        }
        Ok(cuts)
    }

    pub fn contains(&self, p: [f32; 2]) -> bool {
        p[0] >= self.bounds[0]
            && p[1] >= self.bounds[1]
            && p[0] <= self.bounds[2]
            && p[1] <= self.bounds[3]
            && (0..self.points.len()).all(|i| {
                self.winding * cross(self.points[i], self.points[(i + 1) % self.points.len()], p)
                    >= -1e-8
            })
    }

    pub fn height_at(&self, p: [f32; 2]) -> f32 {
        self.height
            + self.slope[0] * (p[0] - self.points[0][0])
            + self.slope[1] * (p[1] - self.points[0][1])
    }

    fn overlaps(&self, polygon: &[[f32; 3]]) -> bool {
        (0..2).all(|axis| {
            polygon.iter().any(|p| p[axis] >= self.bounds[axis])
                && polygon.iter().any(|p| p[axis] <= self.bounds[axis + 2])
        })
    }
}

fn cross(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

fn half_plane(polygon: &[[f32; 3]], a: [f32; 2], b: [f32; 2], sign: f32) -> Vec<[f32; 3]> {
    clip_by(polygon, |p| sign * cross(a, b, [p[0], p[1]]))
}

fn clip_by(polygon: &[[f32; 3]], distance: impl Fn([f32; 3]) -> f32) -> Vec<[f32; 3]> {
    let mut out = Vec::new();
    for i in 0..polygon.len() {
        let p = polygon[i];
        let q = polygon[(i + 1) % polygon.len()];
        let dp = distance(p);
        let dq = distance(q);
        if dp >= 0. {
            out.push(p);
        }
        if (dp < 0.) != (dq < 0.) {
            let t = dp / (dp - dq);
            out.push(std::array::from_fn(|axis| {
                p[axis] + (q[axis] - p[axis]) * t
            }));
        }
    }
    out
}

impl ElevationMap {
    pub(super) fn cut_ceiling_in(&self, min: [f32; 2], max: [f32; 2]) -> Option<f32> {
        let corners = [min, [max[0], min[1]], max, [min[0], max[1]]];
        self.spec
            .cuts
            .iter()
            .filter(|cut| corners.iter().all(|p| cut.contains(*p)))
            .map(|cut| {
                corners
                    .iter()
                    .map(|p| cut.height_at(*p))
                    .fold(f32::NEG_INFINITY, f32::max)
            })
            .min_by(f32::total_cmp)
    }

    pub fn cut_at(&self, p: [f32; 2]) -> Option<&TerrainCut> {
        self.spec
            .cuts
            .iter()
            .filter(|cut| cut.contains(p))
            .min_by(|a, b| a.height_at(p).total_cmp(&b.height_at(p)))
    }

    /// Replace terrain inside cuts with their dry floor; water omits those interiors.
    pub fn cut_triangle(
        &self,
        triangle: [[f32; 3]; 3],
        water: bool,
        mut visit: impl FnMut([[f32; 3]; 3]),
    ) {
        if self.spec.cuts.is_empty() {
            visit(triangle);
            return;
        }
        let mut polygons = vec![triangle.to_vec()];
        for cut in &self.spec.cuts {
            let mut next = Vec::new();
            for polygon in polygons {
                if !cut.overlaps(&polygon) {
                    next.push(polygon);
                    continue;
                }
                let mut inside = polygon;
                for i in 0..cut.points.len() {
                    let a = cut.points[i];
                    let b = cut.points[(i + 1) % cut.points.len()];
                    let outside = half_plane(&inside, a, b, -cut.winding);
                    if outside.len() >= 3 {
                        next.push(outside);
                    }
                    inside = half_plane(&inside, a, b, cut.winding);
                    if inside.len() < 3 {
                        break;
                    }
                }
                if !water && inside.len() >= 3 {
                    let difference = |p: [f32; 3]| p[2] - cut.height_at([p[0], p[1]]);
                    if inside.iter().any(|p| difference(*p) < 0.) {
                        let unchanged = clip_by(&inside, |p| -difference(p));
                        if unchanged.len() >= 3 {
                            next.push(unchanged);
                        }
                        inside = clip_by(&inside, difference);
                    }
                    for p in &mut inside {
                        p[2] = cut.height_at([p[0], p[1]]);
                    }
                    if inside.len() >= 3 {
                        next.push(inside);
                    }
                }
            }
            polygons = next;
        }
        for polygon in polygons {
            for i in 1..polygon.len() - 1 {
                let triangle = [polygon[0], polygon[i], polygon[i + 1]];
                if cross(
                    triangle[0][..2].try_into().unwrap(),
                    triangle[1][..2].try_into().unwrap(),
                    triangle[2][..2].try_into().unwrap(),
                )
                .abs()
                    > 1e-12
                {
                    visit(triangle);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
