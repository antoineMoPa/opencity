//! Explicit recipe choices shared by map projection, the editor and games.
use crate::Argument;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum BuildingFamily {
    #[default]
    Auto = 6,
    Legacy = 7,
    CurtainWall = 0,
    Mixed = 1,
    Podium = 2,
    Heritage = 3,
    Civic = 4,
    ConcreteFrame = 5,
}
impl BuildingFamily {
    pub const ALL: [Self; 6] = [
        Self::CurtainWall,
        Self::Mixed,
        Self::Podium,
        Self::Heritage,
        Self::Civic,
        Self::ConcreteFrame,
    ];
    pub fn from_keyword(s: &str) -> Option<Self> {
        Some(match s {
            "auto" => Self::Auto,
            "legacy" => Self::Legacy,
            "curtain_wall" => Self::CurtainWall,
            "mixed" => Self::Mixed,
            "podium" => Self::Podium,
            "heritage" => Self::Heritage,
            "civic" => Self::Civic,
            "concrete_frame" => Self::ConcreteFrame,
            _ => return None,
        })
    }
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Legacy => "legacy",
            Self::CurtainWall => "curtain_wall",
            Self::Mixed => "mixed",
            Self::Podium => "podium",
            Self::Heritage => "heritage",
            Self::Civic => "civic",
            Self::ConcreteFrame => "concrete_frame",
        }
    }
    pub fn for_index(index: f32, seed: u64) -> Self {
        let outer = [2., 9., 5., 42., 14., 28.];
        let core = [36., 20., 30., 3., 3., 8.];
        let mut draw = sample(seed, 3) * 100.;
        for (i, family) in Self::ALL.into_iter().enumerate() {
            draw -= outer[i] * (1. - index) + core[i] * index;
            if draw < 0. {
                return family;
            }
        }
        Self::ConcreteFrame
    }
    pub fn floors_at(self, index: f32, seed: u64, maximum: u8) -> u8 {
        let ceiling = maximum
            .min(match self {
                Self::Heritage => 8,
                Self::Civic => 9,
                Self::Mixed | Self::ConcreteFrame => 24,
                _ => 40,
            })
            .max(3);
        let variation = sample(seed, 4);
        match self {
            Self::Heritage => ceiling.min(3 + (variation * 4. + index * 1.8) as u8),
            Self::Civic => ceiling.min(3 + (variation * 4. + index * 2.5) as u8),
            _ => (3. + (ceiling - 3) as f32 * index.powf(0.85) * (0.55 + variation * 0.45)).round()
                as u8,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum BuildingPalette {
    #[default]
    Seeded = 9,
    Limestone = 0,
    Sandstone = 1,
    Granite = 2,
    Basalt = 3,
    Chalk = 4,
    Buff = 5,
    Brownstone = 6,
    Concrete = 7,
    Terracotta = 8,
}
impl BuildingPalette {
    pub fn from_keyword(s: &str) -> Option<Self> {
        Some(match s {
            "seeded" => Self::Seeded,
            "limestone" => Self::Limestone,
            "sandstone" => Self::Sandstone,
            "granite" => Self::Granite,
            "basalt" => Self::Basalt,
            "chalk" => Self::Chalk,
            "buff" => Self::Buff,
            "brownstone" => Self::Brownstone,
            "concrete" => Self::Concrete,
            "terracotta" => Self::Terracotta,
            _ => return None,
        })
    }
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Seeded => "seeded",
            Self::Limestone => "limestone",
            Self::Sandstone => "sandstone",
            Self::Granite => "granite",
            Self::Basalt => "basalt",
            Self::Chalk => "chalk",
            Self::Buff => "buff",
            Self::Brownstone => "brownstone",
            Self::Concrete => "concrete",
            Self::Terracotta => "terracotta",
        }
    }
    pub fn color(self) -> [f32; 3] {
        match self {
            Self::Limestone => [0.63, 0.55, 0.41],
            Self::Sandstone => [0.57, 0.34, 0.27],
            Self::Granite => [0.40, 0.44, 0.45],
            Self::Basalt => [0.16, 0.20, 0.22],
            Self::Chalk => [0.76, 0.74, 0.65],
            Self::Buff => [0.67, 0.49, 0.29],
            Self::Brownstone => [0.34, 0.23, 0.17],
            Self::Concrete | Self::Seeded => [0.48, 0.50, 0.48],
            Self::Terracotta => [0.45, 0.21, 0.13],
        }
    }
    pub fn seeded(family: BuildingFamily, seed: u64) -> Self {
        use BuildingPalette::*;
        let choices: &[Self] = match family {
            BuildingFamily::CurtainWall => &[Granite, Chalk, Concrete],
            BuildingFamily::Mixed => &[Limestone, Sandstone, Granite, Chalk, Buff],
            BuildingFamily::Podium => &[Limestone, Granite, Chalk, Concrete],
            BuildingFamily::Heritage => &[Limestone, Sandstone, Buff, Brownstone, Terracotta],
            BuildingFamily::Civic => &[Limestone, Granite, Chalk, Buff],
            _ => &[Granite, Basalt, Concrete],
        };
        choices[(sample(seed, 18) * choices.len() as f32) as usize % choices.len()]
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum GlassTint {
    #[default]
    Seeded = 4,
    Blue = 0,
    Teal = 1,
    Silver = 2,
    Smoke = 3,
}
impl GlassTint {
    pub fn from_keyword(s: &str) -> Option<Self> {
        Some(match s {
            "seeded" => Self::Seeded,
            "blue" => Self::Blue,
            "teal" => Self::Teal,
            "silver" => Self::Silver,
            "smoke" => Self::Smoke,
            _ => return None,
        })
    }
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Seeded => "seeded",
            Self::Blue => "blue",
            Self::Teal => "teal",
            Self::Silver => "silver",
            Self::Smoke => "smoke",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildingArchitecture {
    pub family: BuildingFamily,
    pub palette: BuildingPalette,
    pub glazing: GlassTint,
    pub weathering: f32,
    pub seed: u32,
    pub downtown_index: Option<f32>,
    pub podium_floors: u8,
    pub tower_ratio: f32,
    pub section: BuildingSection,
    pub ground: BuildingGround,
    pub solid_corners: Option<bool>,
    pub setback: bool,
    pub roof_units: Option<u8>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BuildingSection {
    #[default]
    Seeded,
    Piers,
    Bands,
    Frame,
}
impl BuildingSection {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Seeded => "seeded",
            Self::Piers => "piers",
            Self::Bands => "bands",
            Self::Frame => "frame",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BuildingGround {
    #[default]
    Auto,
    Lobby,
    Retail,
    Arcade,
}
impl BuildingGround {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Lobby => "lobby",
            Self::Retail => "retail",
            Self::Arcade => "arcade",
        }
    }
}
impl Default for BuildingArchitecture {
    fn default() -> Self {
        Self {
            family: BuildingFamily::Auto,
            palette: BuildingPalette::Seeded,
            glazing: GlassTint::Seeded,
            weathering: 0.25,
            seed: 0,
            downtown_index: None,
            podium_floors: 2,
            tower_ratio: 0.48,
            section: BuildingSection::Seeded,
            ground: BuildingGround::Auto,
            solid_corners: None,
            setback: true,
            roof_units: None,
        }
    }
}
impl BuildingArchitecture {
    /// Non-default options, preserving map-authored recipes through property edits.
    pub fn options(self) -> Vec<(String, Argument)> {
        let mut out = Vec::new();
        let default = Self::default();
        for (key, value, include) in [
            (
                "family",
                self.family.keyword(),
                self.family != default.family,
            ),
            (
                "palette",
                self.palette.keyword(),
                self.palette != default.palette,
            ),
            (
                "glazing",
                self.glazing.keyword(),
                self.glazing != default.glazing,
            ),
            (
                "section",
                self.section.keyword(),
                self.section != default.section,
            ),
            (
                "ground",
                self.ground.keyword(),
                self.ground != default.ground,
            ),
        ] {
            if include {
                out.push((key.into(), Argument::Identifier(value.into())));
            }
        }
        for (key, value, include) in [
            (
                "weathering",
                self.weathering,
                self.weathering != default.weathering,
            ),
            (
                "tower_ratio",
                self.tower_ratio,
                self.tower_ratio != default.tower_ratio,
            ),
        ] {
            if include {
                out.push((key.into(), Argument::Number(value.to_string())));
            }
        }
        for (key, value, include) in [
            ("seed", self.seed, self.seed != default.seed),
            (
                "podium_floors",
                u32::from(self.podium_floors),
                self.podium_floors != default.podium_floors,
            ),
        ] {
            if include {
                out.push((key.into(), Argument::Number(value.to_string())));
            }
        }
        if let Some(index) = self.downtown_index {
            out.push(("downtown_index".into(), Argument::Number(index.to_string())));
        }
        if !self.setback {
            out.push(("setback".into(), Argument::Identifier("off".into())));
        }
        if let Some(solid) = self.solid_corners {
            out.push((
                "corners".into(),
                Argument::Identifier(if solid { "stone" } else { "glass" }.into()),
            ));
        }
        if let Some(units) = self.roof_units {
            out.push(("roof_units".into(), Argument::Number(units.to_string())));
        }
        out
    }
    pub(super) fn parse(&mut self, key: &str, value: &Argument) -> Result<bool, String> {
        let keyword = || match value {
            Argument::Identifier(s) => Ok(s.as_str()),
            _ => Err(format!("building {key} must be an identifier")),
        };
        let number = || match value {
            Argument::Number(s) => s
                .parse::<f32>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or_else(|| format!("building {key} must be finite")),
            _ => Err(format!("building {key} must be a number")),
        };
        match key {
            "family" => {
                self.family =
                    BuildingFamily::from_keyword(keyword()?).ok_or("unknown building family")?
            }
            "palette" => {
                self.palette =
                    BuildingPalette::from_keyword(keyword()?).ok_or("unknown building palette")?
            }
            "glazing" => {
                self.glazing =
                    GlassTint::from_keyword(keyword()?).ok_or("unknown building glazing")?
            }
            "section" => {
                self.section = match keyword()? {
                    "seeded" => BuildingSection::Seeded,
                    "piers" => BuildingSection::Piers,
                    "bands" => BuildingSection::Bands,
                    "frame" => BuildingSection::Frame,
                    _ => return Err("unknown building section".into()),
                }
            }
            "ground" => {
                self.ground = match keyword()? {
                    "auto" => BuildingGround::Auto,
                    "lobby" => BuildingGround::Lobby,
                    "retail" => BuildingGround::Retail,
                    "arcade" => BuildingGround::Arcade,
                    _ => return Err("unknown building ground floor".into()),
                }
            }
            "setback" => {
                self.setback = match keyword()? {
                    "on" => true,
                    "off" => false,
                    _ => return Err("building setback must be on or off".into()),
                }
            }
            "corners" => {
                self.solid_corners = Some(match keyword()? {
                    "stone" => true,
                    "glass" => false,
                    _ => return Err("building corners must be stone or glass".into()),
                })
            }
            "roof_units" => {
                let n = number()?;
                if !(0.0..=8.0).contains(&n) || n.fract() != 0. {
                    return Err("building roof_units must be an integer 0..8".into());
                }
                self.roof_units = Some(n as u8);
            }
            "weathering" | "downtown_index" => {
                let n = number()?;
                if !(0.0..=1.0).contains(&n) {
                    return Err(format!("building {key} must be 0..1"));
                }
                if key == "weathering" {
                    self.weathering = n;
                } else {
                    self.downtown_index = Some(n);
                }
            }
            "seed" => {
                let Argument::Number(s) = value else {
                    return Err("building seed must be an unsigned integer".into());
                };
                self.seed = s
                    .parse()
                    .map_err(|_| "building seed must be an unsigned integer")?;
            }
            "podium_floors" => {
                let n = number()?;
                if n != 1. && n != 2. {
                    return Err("building podium_floors must be 1 or 2".into());
                }
                self.podium_floors = n as u8;
            }
            "tower_ratio" => {
                let n = number()?;
                if !(0.35..=0.70).contains(&n) {
                    return Err("building tower_ratio must be 0.35..0.70".into());
                }
                self.tower_ratio = n;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
/// A map-authored center/reach; callers store the resulting index in `.road` building options.
#[derive(Clone, Copy, Debug)]
pub struct DowntownField {
    pub center: [f32; 2],
    pub reach: f32,
    pub noise: f32,
    pub seed: u64,
}
impl DowntownField {
    pub fn index(self, p: [f32; 2]) -> f32 {
        let distance = (p[0] - self.center[0]).hypot(p[1] - self.center[1]);
        let radial =
            ((distance / self.reach.max(0.0001)).min(1.) * std::f32::consts::FRAC_PI_2).cos();
        let layer = |spacing: f32, channel: u32| {
            let x = p[0] / spacing;
            let y = p[1] / spacing;
            let ix = x.floor() as i32;
            let iy = y.floor() as i32;
            let sx = (x - x.floor()).powi(2) * (3. - 2. * (x - x.floor()));
            let sy = (y - y.floor()).powi(2) * (3. - 2. * (y - y.floor()));
            let at = |a: i32, b: i32| {
                sample(
                    self.seed
                        ^ (a as u64).wrapping_mul(73856093)
                        ^ (b as u64).wrapping_mul(19349663),
                    channel,
                ) * 2.
                    - 1.
            };
            let a = at(ix, iy) * (1. - sx) + at(ix + 1, iy) * sx;
            let b = at(ix, iy + 1) * (1. - sx) + at(ix + 1, iy + 1) * sx;
            a * (1. - sy) + b * sy
        };
        (radial
            + self.noise
                * (layer(self.reach * 0.31, 90) * 0.78 + layer(self.reach * 0.125, 91) * 0.22))
            .clamp(0., 1.)
    }
}
pub fn sample(seed: u64, channel: u32) -> f32 {
    let mut n = seed ^ (u64::from(channel) + 1).wrapping_mul(0x9e3779b97f4a7c15);
    n = (n ^ (n >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    n = (n ^ (n >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((n ^ (n >> 31)) >> 40) as u32 as f32 / 16_777_216.
}
