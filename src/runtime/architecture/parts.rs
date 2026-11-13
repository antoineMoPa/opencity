use super::*;

struct Kit {
    recipe: BuildingRecipe,
    mesh: Mesh,
    volumes: Vec<BuildingVolume>,
    unit: f32,
    wall_top: f32,
    street: f32,
    pad: f32,
    entry: f32,
}
impl Kit {
    fn piece(&mut self, min: [f32; 3], max: [f32; 3], color: [f32; 3], skin: Option<u32>) {
        let r = self.recipe;
        let min = [min[0], min[1] + self.entry, min[2]];
        let max = [max[0], max[1] + self.entry, max[2]];
        let min = std::array::from_fn(|i| min[i].clamp(0., [r.width, r.depth, r.height][i]));
        let max = std::array::from_fn(|i| max[i].clamp(0., [r.width, r.depth, r.height][i]));
        if (0..3).all(|i| max[i] > min[i]) {
            self.volumes.push(BuildingVolume {
                recipe: r,
                min,
                max,
            });
        }
        r.cuboid(&mut self.mesh, min, max, color, skin, false);
    }
    fn slab(&mut self, x: f32, y: f32, w: f32, d: f32, z: f32, h: f32, color: [f32; 3]) {
        self.piece([x, y, z], [x + w, y + d, z + h], color, None);
    }
    fn stone(&mut self, x: f32, y: f32, w: f32, d: f32, z: f32, h: f32) {
        self.piece(
            [x, y, z],
            [x + w, y + d, z + h],
            self.recipe.palette.color(),
            Some(8 + self.recipe.family as u32),
        );
    }
    fn body(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        d: f32,
        base: f32,
        top: f32,
        family: Family,
        street: bool,
    ) {
        self.body_skin(x, y, w, d, base, top, family as u32, street);
    }
    fn body_skin(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        d: f32,
        base: f32,
        top: f32,
        family: u32,
        street: bool,
    ) {
        if top <= base {
            return;
        }
        self.volumes.push(BuildingVolume {
            recipe: self.recipe,
            min: [x, y + self.entry, base],
            max: [x + w, y + d + self.entry, top],
        });
        self.recipe.cuboid(
            &mut self.mesh,
            [x, y + self.entry, base],
            [x + w, y + d + self.entry, top],
            self.recipe.palette.color(),
            Some(family),
            street,
        );
        if !street && self.recipe.solid_corners {
            let u = self.unit;
            let t = (u * 0.36).min(w * 0.08).min(d * 0.08);
            for xx in [x, x + w - t] {
                for yy in [y, y + d - t] {
                    self.stone(xx, yy, t, t, base, top - base);
                }
            }
        }
    }
    fn roof(&mut self, x: f32, y: f32, w: f32, d: f32, z: f32, equipment: u8) {
        let u = self.unit;
        let metal = [0.35, 0.37, 0.34];
        self.piece(
            [x, y, z],
            [x + w, y + d, z + 0.20 * u],
            [0.27, 0.29, 0.26],
            Some(14),
        );
        let t = (0.26 * u).min(w * 0.06).min(d * 0.06);
        for yy in [y, y + d - t] {
            self.stone(x, yy, w, t, z + 0.055 * u, 0.85 * u);
            self.slab(
                x - 0.04 * u,
                yy - 0.065 * u,
                w + 0.08 * u,
                t + 0.13 * u,
                z + 0.89 * u,
                0.10 * u,
                metal,
            );
        }
        for xx in [x, x + w - t] {
            self.stone(xx, y, t, d, z + 0.055 * u, 0.85 * u);
            self.slab(
                xx - 0.065 * u,
                y - 0.04 * u,
                t + 0.13 * u,
                d + 0.08 * u,
                z + 0.89 * u,
                0.10 * u,
                metal,
            );
        }
        if equipment == 0 {
            return;
        }
        let su = u.min((w - 0.001) / 8.).min((d - 0.001) / 8.).max(0.000001);
        let count = usize::from(equipment);
        let columns = (count + 1).min(((w / su - 2.) / 2.6).floor().max(3.) as usize);
        let rows = (count + 1).div_ceil(columns);
        let cw = (w - 2. * su) / columns as f32;
        let cd = (d - 2. * su) / rows as f32;
        let core_x = x + su + cw / 2.;
        let core_y = y + d - su - cd / 2.;
        self.stone(
            core_x - 1.2 * su,
            core_y - 1.3 * su,
            2.4 * su,
            2.6 * su,
            z + 0.10 * su,
            2.3 * su,
        );
        self.slab(
            core_x - 1.29 * su,
            core_y - 1.39 * su,
            2.58 * su,
            2.78 * su,
            z + 2.37 * su,
            0.14 * su,
            metal,
        );
        self.slab(
            core_x - 0.45 * su,
            core_y - 1.355 * su,
            0.9 * su,
            0.07 * su,
            z + 0.17 * su,
            1.9 * su,
            [0.13, 0.18, 0.19],
        );
        for i in 0..count {
            let col = (i + 1) % columns;
            let row = (i + 1) / columns;
            let xx = x + su + cw * (col as f32 + 0.5);
            // Preserve access around the optional heritage/concrete penthouses.
            let yy = if matches!(self.recipe.family, Family::Heritage | Family::ConcreteFrame) {
                y + 1.4 * su
            } else {
                y + d - su - cd * (row as f32 + 0.5)
            };
            let uw = (1.25 + sample(self.recipe.seed, i as u32 + 42) * 0.45) * su;
            let uh = (0.9 + sample(self.recipe.seed, i as u32 + 60) * 0.4) * su;
            for dx in [-0.45, 0.45] {
                self.slab(
                    xx + (dx - 0.06) * su,
                    yy - 0.6 * su,
                    0.12 * su,
                    1.2 * su,
                    z + 0.105 * su,
                    0.35 * su,
                    [0.13, 0.18, 0.19],
                );
            }
            self.slab(
                xx - uw / 2.,
                yy - 0.82 * su,
                uw,
                1.64 * su,
                z + 0.45 * su,
                uh,
                [0.43, 0.47, 0.44],
            );
            self.slab(
                xx - uw / 2. - 0.05 * su,
                yy - 0.87 * su,
                uw + 0.10 * su,
                1.74 * su,
                z + 0.445 * su + uh,
                0.09 * su,
                metal,
            );
            let fan = uw * 0.83;
            self.piece(
                [xx - fan / 2., yy - fan / 2., z + 0.54 * su + uh],
                [xx + fan / 2., yy + fan / 2., z + 0.56 * su + uh],
                [0.13, 0.18, 0.19],
                Some(6),
            );
            for line in 0..6 {
                self.slab(
                    xx - uw * 0.415,
                    yy - 0.845 * su,
                    uw * 0.83,
                    0.025 * su,
                    z + (0.5025 + line as f32 * 0.115) * su,
                    0.035 * su,
                    [0.13, 0.18, 0.19],
                );
            }
            self.slab(
                xx - 0.24 * su,
                yy + 0.565 * su,
                0.48 * su,
                1.15 * su,
                z + 0.20 * su,
                0.38 * su,
                metal,
            );
        }
        if equipment > 1 {
            self.slab(
                x + w * 0.78 - 0.185 * su,
                y + d * 0.80 - 0.185 * su,
                0.37 * su,
                0.37 * su,
                z + 0.055 * su,
                1.55 * su,
                metal,
            );
            self.slab(
                x + w * 0.78 - 0.325 * su,
                y + d * 0.80 - 0.325 * su,
                0.65 * su,
                0.65 * su,
                z + 1.58 * su,
                0.12 * su,
                [0.13, 0.18, 0.19],
            );
        }
    }
    fn street(&mut self, x: f32, y: f32, w: f32, d: f32) {
        let r = self.recipe;
        let u = self.unit;
        let arcade = r.ground == BuildingGround::Arcade;
        let inset = (if arcade { 1.12 * u } else { 0.27 * u })
            .min(w * 0.15)
            .min(d * 0.15);
        self.body(
            x + inset,
            y + inset,
            w - 2. * inset,
            d - 2. * inset,
            0.,
            self.street,
            r.family,
            true,
        );
        self.stone(x, y, w, d, self.street - 0.22 * u, 0.44 * u);
        let bays = (w / (u * 3.1)).round().max(1.) as usize;
        let center = x + w * (bays / 2) as f32 / bays as f32 + w / (2. * bays as f32);
        let portal = (w * 0.25).min(4.3 * u);
        self.slab(
            center - portal / 2.,
            y - 1.5 * u,
            portal,
            1.8 * u,
            self.street * 0.77,
            0.15 * u,
            [0.13, 0.18, 0.19],
        );
        for sx in [-1., 1.] {
            self.slab(
                center + sx * 0.19 * u,
                y + inset - 0.08 * u,
                0.035 * u,
                0.08 * u,
                self.street * 0.30,
                0.61 * u,
                [0.35, 0.37, 0.34],
            );
        }
        let pier = if arcade {
            0.48 * u
        } else if r.ground == BuildingGround::Retail {
            0.40 * u
        } else {
            0.28 * u
        };
        for i in 0..=bays {
            let xx = x + (w - pier) * i as f32 / bays as f32;
            self.stone(xx, y - 0.345 * u, pier, 0.65 * u, 0., self.street);
            self.stone(xx, y + d - 0.65 * u, pier, 0.65 * u, 0., self.street);
        }
        if r.ground == BuildingGround::Retail {
            for i in 0..bays {
                let sw = w / bays as f32;
                let sx = x + sw * i as f32;
                self.slab(
                    sx + 0.10 * u,
                    y,
                    sw - 0.20 * u,
                    0.12 * u,
                    self.street * 0.76,
                    0.40 * u,
                    [0.25, 0.27, 0.25],
                );
                for mark in 0..3 {
                    self.slab(
                        sx + sw * 0.25 + mark as f32 * 0.38 * u,
                        y,
                        0.22 * u,
                        0.025 * u,
                        self.street * 0.80,
                        0.035 * u,
                        [0.77, 0.72, 0.60],
                    );
                }
            }
        }
    }
    fn build(&mut self) {
        let r = self.recipe;
        let (u, p) = (self.unit, self.pad);
        let (w, d, h, b) = (
            r.width - 2. * p,
            r.depth - 2. * p - self.entry,
            self.wall_top,
            self.street,
        );
        let floors = ((h - b) / r.floor_height).round().max(1.) as usize;
        self.street(p, p, w, d);
        match r.family {
            Family::CurtainWall => {
                let lower = if r.setback && floors >= 3 {
                    floors.saturating_sub((floors * 18 / 100).max(2)).max(2)
                } else {
                    floors
                };
                let split = b + (h - b) * lower as f32 / floors as f32;
                self.body(p, p, w, d, b, split, r.family, false);
                if lower < floors {
                    self.roof(p, p, w, d, split, 0);
                    let tw = w * 0.86;
                    let td = d * 0.85;
                    let x = p + (w - tw) / 2. + w * 0.04;
                    self.body(x, p + (d - td) / 2., tw, td, split, h, r.family, false);
                    self.roof(x, p + (d - td) / 2., tw, td, h, r.roof_units);
                } else {
                    self.roof(p, p, w, d, h, r.roof_units);
                }
            }
            Family::Mixed => {
                let share = 0.32 + sample(r.seed, 80) * 0.22;
                let sw = w * share;
                let sf = ((floors as f32 * (0.55 + sample(r.seed, 81) * 0.25)).floor() as usize)
                    .max(2)
                    .min(floors);
                let sh = b + (h - b) * sf as f32 / floors as f32;
                self.body(p, p, sw, d, b, sh, Family::Mixed, false);
                self.roof(p, p, sw, d, sh, 0);
                self.body(
                    p + sw,
                    p + d * 0.01,
                    w - sw,
                    d * 0.86,
                    b,
                    h,
                    Family::CurtainWall,
                    false,
                );
                self.roof(p + sw, p + d * 0.01, w - sw, d * 0.86, h, r.roof_units);
            }
            Family::Podium => {
                let ph = (b + (r.podium_floors - 1) as f32 * r.floor_height).min(h * 0.45);
                if ph > b {
                    self.body(p, p, w, d, b, ph, Family::Mixed, false);
                }
                self.roof(p, p, w, d, ph, 0);
                let tw = w * r.tower_ratio;
                let td = d * (r.tower_ratio + 0.10).clamp(0.40, 0.78);
                let x = p + (w - tw) / 2. + (sample(r.seed, 82) - 0.5) * w * 0.20;
                let y = p + (d - td) / 2. + d * 0.09;
                self.body(x, y, tw, td, ph, h, Family::Podium, false);
                let count = ((h - ph) / r.floor_height).round().max(1.) as usize;
                for f in 1..count {
                    let z = ph + (h - ph) * f as f32 / count as f32;
                    self.stone(
                        x - 0.45 * u,
                        y - 0.45 * u,
                        tw + 0.9 * u,
                        td + 0.9 * u,
                        z,
                        0.16 * u,
                    );
                    self.piece(
                        [x - 0.45 * u, y - 0.425 * u, z + 0.18 * u],
                        [x + tw + 0.45 * u, y - 0.375 * u, z + 0.98 * u],
                        [0.13, 0.18, 0.19],
                        Some(16),
                    );
                }
                self.roof(x, y, tw, td, h, r.roof_units);
            }
            Family::Heritage => {
                let last = b + (h - b) * (floors - 1) as f32 / floors as f32;
                self.body(p, p, w, d, b, last, r.family, false);
                // Skin 7 retains the heritage proportions and arches only the upper row.
                self.body_skin(p, p, w, d, last, h, 7, false);
                self.stone(
                    p - 0.175 * u,
                    p - 0.175 * u,
                    w + 0.35 * u,
                    d + 0.35 * u,
                    h - 0.24 * u,
                    0.28 * u,
                );
                self.stone(
                    p - 0.40 * u,
                    p - 0.40 * u,
                    w + 0.80 * u,
                    d + 0.80 * u,
                    h + 0.17 * u,
                    0.36 * u,
                );
                self.stone(
                    p - 0.28 * u,
                    p - 0.28 * u,
                    w + 0.56 * u,
                    d + 0.56 * u,
                    h + 0.58 * u,
                    0.22 * u,
                );
                for f in (1..floors).step_by(2) {
                    self.stone(
                        p - 0.08 * u,
                        p - 0.08 * u,
                        w + 0.16 * u,
                        d + 0.16 * u,
                        b + (h - b) * f as f32 / floors as f32,
                        0.15 * u,
                    );
                }
                for xx in [p, p + w - 0.44 * u] {
                    self.stone(xx, p - 0.27 * u, 0.44 * u, 0.28 * u, 0., h);
                }
                self.roof(p, p, w, d, h + 0.85 * u, r.roof_units);
                if sample(r.seed, 83) > 0.45 {
                    self.stone(
                        p + w * 0.365,
                        p + d * 0.50,
                        w * 0.27,
                        d * 0.30,
                        h + 0.75 * u,
                        1.5 * u,
                    );
                }
            }
            Family::Civic => {
                self.body(p, p, w, d, b, h, r.family, false);
                self.body(
                    p + w * 0.35,
                    p - 0.66 * u,
                    w * 0.30,
                    0.80 * u,
                    b,
                    h,
                    r.family,
                    false,
                );
                let columns = (w / (u * 5.)).floor().max(2.) as usize;
                for i in 0..=columns {
                    self.stone(
                        p + (w - 0.33 * u) * i as f32 / columns as f32,
                        p - 0.51 * u,
                        0.33 * u,
                        0.52 * u,
                        b,
                        h - b,
                    );
                }
                for xx in [p, p + w - 0.50 * u] {
                    self.stone(xx, p - 0.47 * u, 0.50 * u, 0.48 * u, 0., h);
                    for z in 1..(h / (u * 0.85)) as usize {
                        self.stone(
                            xx,
                            p - 0.525 * u,
                            0.61 * u,
                            0.09 * u,
                            z as f32 * 0.85 * u,
                            0.39 * u,
                        );
                    }
                }
                self.stone(
                    p - 0.40 * u,
                    p - 0.40 * u,
                    w + 0.80 * u,
                    d + 0.80 * u,
                    h,
                    0.50 * u,
                );
                self.roof(p, p, w, d, h + 0.52 * u, r.roof_units);
                self.stone(
                    p + w * 0.35 - 0.5 * u,
                    p - 0.1 * u,
                    w * 0.30 + u,
                    1.2 * u,
                    h + 0.5 * u,
                    u,
                );
                self.stone(
                    p + w * 0.398,
                    p - 0.1 * u,
                    w * 0.204,
                    1.2 * u,
                    h + 1.45 * u,
                    0.50 * u,
                );
                self.stone(
                    p + w * 0.398 - 0.1 * u,
                    p - 0.2 * u,
                    w * 0.204 + 0.2 * u,
                    1.4 * u,
                    h + 1.95 * u,
                    0.12 * u,
                );
            }
            _ => {
                self.body(p, p, w, d, b, h, Family::ConcreteFrame, false);
                for f in 1..=floors {
                    self.stone(
                        p - 0.475 * u,
                        p - 0.475 * u,
                        w + 0.95 * u,
                        d + 0.95 * u,
                        b + (h - b) * f as f32 / floors as f32,
                        0.32 * u,
                    );
                }
                self.roof(p, p, w, d, h + 0.20 * u, r.roof_units);
                if r.setback {
                    self.stone(
                        p + w * 0.46,
                        p + d * 0.36,
                        w * 0.42,
                        d * 0.48,
                        h + 0.20 * u,
                        1.8 * u,
                    );
                }
            }
        }
    }
}
pub(super) fn mesh(recipe: BuildingRecipe) -> Mesh {
    geometry(recipe).0
}
pub(super) fn geometry(recipe: BuildingRecipe) -> (Mesh, Vec<BuildingVolume>) {
    let unit = (recipe.floor_height / 3.25)
        .min(recipe.height / 14.)
        .min(recipe.width / 8.)
        .min(recipe.depth / 8.);
    let pad = match recipe.family {
        Family::Heritage | Family::Civic => 0.4 * unit,
        Family::ConcreteFrame => 0.475 * unit,
        _ => 0.,
    };
    let mut kit = Kit {
        recipe: BuildingRecipe {
            origin: [0.; 3],
            along: [1., 0.],
            inward: [0., 1.],
            ..recipe
        },
        mesh: Mesh::default(),
        volumes: Vec::new(),
        unit,
        pad,
        entry: 1.5 * unit,
        wall_top: recipe.height - unit * 4.,
        street: (unit * 4.8).min(recipe.height * 0.28),
    };
    kit.build();
    // Bake before world placement: exact cuboids, stable precision and rotation-independent AO.
    super::super::ambient_occlusion::bake_cuboids_fast(
        &mut kit.mesh,
        recipe.floor_height * 1.25,
        Some(0.),
    );
    for v in &mut kit.mesh.vertices {
        v.position = recipe.point(v.position);
        v.normal = recipe.vector(v.normal);
    }
    if recipe.along[0] * recipe.inward[1] - recipe.along[1] * recipe.inward[0] < 0. {
        for triangle in kit.mesh.vertices.chunks_exact_mut(3) {
            triangle.swap(1, 2);
        }
    }
    for volume in &mut kit.volumes {
        volume.recipe = recipe;
    }
    (kit.mesh, kit.volumes)
}
