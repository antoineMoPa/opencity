//! Shared section/edge/street/roof kit. Authored footprints and height remain authoritative.
use super::{Mesh, SurfaceKind, Vertex};
use crate::building::sample;
use crate::{
    BuildingFamily as Family, BuildingGround, BuildingMaterial, BuildingPalette as Palette,
    BuildingSection, GlassTint, LandscapeFeature, LandscapeKind,
};
mod parts;

#[derive(Clone, Copy, Debug)]
pub(super) struct BuildingVolume {
    recipe: BuildingRecipe,
    min: [f32; 3],
    max: [f32; 3],
}
impl BuildingVolume {
    pub(super) fn intersects(self, point: [f32; 2], radius: f32, bottom: f32, top: f32) -> bool {
        let r = self.recipe;
        if r.origin[2] + self.min[2] >= top || r.origin[2] + self.max[2] <= bottom {
            return false;
        }
        let d = [point[0] - r.origin[0], point[1] - r.origin[1]];
        let local = [
            d[0] * r.along[0] + d[1] * r.along[1],
            d[0] * r.inward[0] + d[1] * r.inward[1],
        ];
        (local[0] - local[0].clamp(self.min[0], self.max[0])).powi(2)
            + (local[1] - local[1].clamp(self.min[1], self.max[1])).powi(2)
            <= radius * radius
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BuildingRecipe {
    pub family: Family,
    pub palette: Palette,
    pub glazing: GlassTint,
    pub seed: u64,
    pub downtown_index: f32,
    pub weathering: f32,
    pub podium_floors: u8,
    pub tower_ratio: f32,
    pub section: u32,
    pub ground: BuildingGround,
    pub solid_corners: bool,
    pub setback: bool,
    pub roof_units: u8,
    pub width: f32,
    pub depth: f32,
    pub height: f32,
    pub floor_height: f32,
    pub origin: [f32; 3],
    pub along: [f32; 2],
    pub inward: [f32; 2],
}
impl BuildingRecipe {
    pub fn from_feature(f: &LandscapeFeature) -> Option<Self> {
        let style = f.building_style?;
        let a = style.architecture;
        if f.kind != LandscapeKind::Building
            || f.model.is_some()
            || f.points.len() != 4
            || a.family == Family::Legacy
            || (a.family == Family::Auto && style.material != BuildingMaterial::Glass)
        {
            return None;
        }
        let i = f.front_edge.unwrap_or(0) % 4;
        let p = f.points[i];
        let q = f.points[(i + 1) % 4];
        let width = (q[0] - p[0]).hypot(q[1] - p[1]);
        if width < 0.001 {
            return None;
        }
        let along = [(q[0] - p[0]) / width, (q[1] - p[1]) / width];
        let r = f.points[(i + 3) % 4];
        let across = [r[0] - p[0], r[1] - p[1]];
        let depth = across[0].hypot(across[1]);
        if depth < 0.001 || (across[0] * along[0] + across[1] * along[1]).abs() > depth * 0.002 {
            return None;
        }
        let inward = [across[0] / depth, across[1] / depth];
        let opposite = f.points[(i + 2) % 4];
        if (opposite[0] - q[0] - across[0]).hypot(opposite[1] - q[1] - across[1])
            > width.max(depth) * 0.002
        {
            return None;
        }
        let height = f.height?.max(0.);
        if height < 0.002 {
            return None;
        }
        let mut seed = 0xcbf29ce484222325u64 ^ u64::from(a.seed);
        for p in &f.points {
            for v in p {
                for b in v.to_bits().to_le_bytes() {
                    seed = (seed ^ u64::from(b)).wrapping_mul(0x100000001b3);
                }
            }
        }
        let index = a
            .downtown_index
            .unwrap_or_else(|| ((height / style.floor_height - 2.) / 22.).clamp(0., 1.));
        let family = if a.family == Family::Auto {
            Family::for_index(index, seed)
        } else {
            a.family
        };
        let palette = if a.palette == Palette::Seeded {
            Palette::seeded(family, seed)
        } else {
            a.palette
        };
        let glazing = if a.glazing == GlassTint::Seeded {
            match family {
                Family::Heritage => GlassTint::Smoke,
                Family::Civic => GlassTint::Silver,
                Family::CurtainWall => GlassTint::Blue,
                _ => GlassTint::Teal,
            }
        } else {
            a.glazing
        };
        Some(Self {
            family,
            palette,
            glazing,
            seed,
            downtown_index: index,
            weathering: a.weathering,
            podium_floors: a.podium_floors,
            tower_ratio: a.tower_ratio,
            width,
            depth,
            height,
            section: match a.section {
                BuildingSection::Piers => 0,
                BuildingSection::Bands => 1,
                BuildingSection::Frame => 2,
                BuildingSection::Seeded => (seed >> 8) as u32 % 3,
            },
            ground: if a.ground == BuildingGround::Auto {
                match family {
                    Family::CurtainWall | Family::Civic => BuildingGround::Lobby,
                    Family::ConcreteFrame => BuildingGround::Arcade,
                    _ => BuildingGround::Retail,
                }
            } else {
                a.ground
            },
            solid_corners: a
                .solid_corners
                .unwrap_or(!matches!(family, Family::CurtainWall | Family::Podium)),
            setback: a.setback,
            roof_units: a.roof_units.unwrap_or(1 + (seed % 4) as u8),
            floor_height: style.floor_height.min(height * 0.5),
            origin: [p[0], p[1], f.elevation],
            along,
            inward,
        })
    }
    pub fn mesh(self) -> Mesh {
        parts::mesh(self)
    }
    pub(super) fn geometry(self) -> (Mesh, Vec<BuildingVolume>) {
        parts::geometry(self)
    }
    fn point(self, p: [f32; 3]) -> [f32; 3] {
        [
            self.origin[0] + self.along[0] * p[0] + self.inward[0] * p[1],
            self.origin[1] + self.along[1] * p[0] + self.inward[1] * p[1],
            self.origin[2] + p[2],
        ]
    }
    fn vector(self, p: [f32; 3]) -> [f32; 3] {
        [
            self.along[0] * p[0] + self.inward[0] * p[1],
            self.along[1] * p[0] + self.inward[1] * p[1],
            p[2],
        ]
    }
    fn packed(self, family: u32, street: bool) -> f32 {
        // All 24 bits are exact f32 integers. UVs remain interpolated.
        (family
            | (self.palette as u32) << 3
            | (self.glazing as u32) << 7
            | ((self.weathering * 15.).round() as u32) << 9
            | ((self.seed % 128) as u32) << 13
            | u32::from(street) << 20
            | self.section << 21
            | u32::from(self.solid_corners) << 23) as f32
    }
    fn cuboid(
        self,
        mesh: &mut Mesh,
        min: [f32; 3],
        max: [f32; 3],
        color: [f32; 3],
        family: Option<u32>,
        street: bool,
    ) {
        if (0..3).any(|i| max[i] - min[i] < 0.000001) {
            return;
        }
        // Keep the facade skin behind applied stone/metal trim in both renderers.
        // The two-centimeter reveal avoids coplanar surfaces at panel edges.
        let body = family.is_some_and(|f| f < 6 || f == 7);
        let inset = if body {
            (self.floor_height / 3.25 * 0.02)
                .min((max[0] - min[0]) * 0.01)
                .min((max[1] - min[1]) * 0.01)
        } else {
            0.
        };
        let min = [min[0] + inset, min[1] + inset, min[2]];
        let max = [max[0] - inset, max[1] - inset, max[2]];
        let start = mesh.vertices.len();
        let normals = [
            [0., -1., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [-1., 0., 0.],
            [0., 0., 1.],
            [0., 0., -1.],
        ];
        let corners = [
            [
                [max[0], min[1], min[2]],
                [min[0], min[1], min[2]],
                [min[0], min[1], max[2]],
                [max[0], min[1], max[2]],
            ],
            [
                [max[0], max[1], min[2]],
                [max[0], min[1], min[2]],
                [max[0], min[1], max[2]],
                [max[0], max[1], max[2]],
            ],
            [
                [min[0], max[1], min[2]],
                [max[0], max[1], min[2]],
                [max[0], max[1], max[2]],
                [min[0], max[1], max[2]],
            ],
            [
                [min[0], min[1], min[2]],
                [min[0], max[1], min[2]],
                [min[0], max[1], max[2]],
                [min[0], min[1], max[2]],
            ],
            [
                [min[0], min[1], max[2]],
                [max[0], min[1], max[2]],
                [max[0], max[1], max[2]],
                [min[0], max[1], max[2]],
            ],
            [
                [min[0], min[1], min[2]],
                [min[0], max[1], min[2]],
                [max[0], max[1], min[2]],
                [max[0], min[1], min[2]],
            ],
        ];
        for (face, p) in corners.into_iter().enumerate() {
            let plain = family.is_some_and(|f| (8..16).contains(&f));
            let glazing = family == Some(16);
            let facade = family.is_some() && (face < 4 || family == Some(6) || plain || glazing);
            let edge_length = |a: usize, b: usize| {
                (0..3)
                    .map(|axis| (p[a][axis] - p[b][axis]).powi(2))
                    .sum::<f32>()
                    .sqrt()
            };
            let h = edge_length(3, 0).max(0.00001);
            let w = edge_length(1, 0).max(0.00001);
            let columns = (w / (self.floor_height * 0.96)).round().max(1.);
            let rows = if street {
                1.
            } else {
                (h / self.floor_height).round().max(1.)
            };
            let uv = [[0., 0.], [columns, 0.], [columns, rows], [0., rows]];
            let indices = if face < 4 {
                [0, 2, 1, 0, 3, 2]
            } else {
                [0, 1, 2, 0, 2, 3]
            };
            for i in indices {
                mesh.vertices.push(Vertex {
                    position: self.point(p[i]),
                    normal: self.vector(normals[face]),
                    color,
                    surface: if facade {
                        [
                            if plain {
                                SurfaceKind::Masonry
                            } else if glazing {
                                SurfaceKind::Glazing
                            } else {
                                SurfaceKind::Facade
                            } as u32 as f32,
                            uv[i][0],
                            uv[i][1],
                            self.packed(family.unwrap() % 8, street),
                        ]
                    } else {
                        // Roof membranes, copings and metalwork share filtered weathering.
                        [
                            SurfaceKind::Masonry as u32 as f32,
                            uv[i][0],
                            uv[i][1],
                            self.packed(6, false),
                        ]
                    },
                });
            }
        }
        // Frame orientation may reverse triangle winding; explicit normals still face outwards.
        if self.along[0] * self.inward[1] - self.along[1] * self.inward[0] < 0. {
            for t in mesh.vertices[start..].chunks_exact_mut(3) {
                t.swap(1, 2);
            }
        }
    }
}
