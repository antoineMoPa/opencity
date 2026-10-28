//! Building facade styles shared by landscape projection and renderers.

use crate::Argument;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BuildingMaterial {
    #[default]
    Concrete,
    Glass,
    Brick,
    RedBrick,
    BrownBrick,
    BuffBrick,
    PaleBrick,
    Limestone,
}

impl BuildingMaterial {
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        match keyword {
            "concrete" => Some(Self::Concrete),
            "glass" => Some(Self::Glass),
            "brick" => Some(Self::Brick),
            "red_brick" => Some(Self::RedBrick),
            "brown_brick" => Some(Self::BrownBrick),
            "buff_brick" => Some(Self::BuffBrick),
            "pale_brick" => Some(Self::PaleBrick),
            "limestone" => Some(Self::Limestone),
            _ => None,
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            Self::Concrete => "concrete",
            Self::Glass => "glass",
            Self::Brick => "brick",
            Self::RedBrick => "red_brick",
            Self::BrownBrick => "brown_brick",
            Self::BuffBrick => "buff_brick",
            Self::PaleBrick => "pale_brick",
            Self::Limestone => "limestone",
        }
    }

    /// Facade shader selector; zero is reserved for ordinary surfaces.
    pub fn shader_id(self) -> u32 {
        match self {
            Self::Concrete => 2,
            Self::Glass => 1,
            Self::Brick => 3,
            Self::RedBrick => 11,
            Self::BrownBrick => 12,
            Self::BuffBrick => 13,
            Self::PaleBrick => 14,
            Self::Limestone => 15,
        }
    }

    pub fn wall_rgb(self) -> [u8; 3] {
        match self {
            Self::Concrete => [174, 180, 176],
            Self::Glass => [112, 157, 174],
            Self::Brick => [164, 109, 91],
            Self::RedBrick => [151, 83, 62],
            Self::BrownBrick => [119, 91, 71],
            Self::BuffBrick => [185, 161, 112],
            Self::PaleBrick => [206, 199, 165],
            Self::Limestone => [152, 155, 142],
        }
    }

    pub fn roof_rgb(self) -> [u8; 3] {
        match self {
            Self::Concrete => [192, 198, 194],
            Self::Glass => [151, 173, 181],
            Self::Brick => [154, 143, 133],
            Self::RedBrick
            | Self::BrownBrick
            | Self::BuffBrick
            | Self::PaleBrick
            | Self::Limestone => [83, 95, 91],
        }
    }

    /// Fixed world lighting for polygon edges, shared by native and GLB facades.
    pub fn wall_rgb_for_edge(self, a: [f32; 2], b: [f32; 2]) -> [u8; 3] {
        let shade = if (b[0] - a[0]).abs() > (b[1] - a[1]).abs() {
            if b[0] > a[0] {
                0.84
            } else {
                1.0
            }
        } else if b[1] > a[1] {
            0.92
        } else {
            0.77
        };
        self.wall_rgb()
            .map(|channel| (channel as f32 * shade) as u8)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildingStyle {
    pub material: BuildingMaterial,
    /// Spacing between procedural facade floors, in world units.
    pub floor_height: f32,
}

impl Default for BuildingStyle {
    fn default() -> Self {
        Self {
            material: BuildingMaterial::default(),
            floor_height: 0.04,
        }
    }
}

impl BuildingStyle {
    pub(crate) fn parse_options(properties: &[(String, Argument)]) -> Result<Self, String> {
        let mut style = Self::default();
        for (index, (key, value)) in properties.iter().enumerate() {
            if properties[..index]
                .iter()
                .any(|(previous, _)| previous == key)
            {
                return Err(format!("duplicate building option '{key}'"));
            }
            match key.as_str() {
                "material" => {
                    let Argument::Identifier(keyword) = value else {
                        return Err("unknown building material".into());
                    };
                    style.material = BuildingMaterial::from_keyword(keyword)
                        .ok_or("unknown building material")?;
                }
                "floor_height" => {
                    let Argument::Number(text) = value else {
                        return Err("building floor_height must be a positive finite number".into());
                    };
                    style.floor_height = text
                        .parse::<f32>()
                        .ok()
                        .filter(|height| height.is_finite() && *height > 0.0)
                        .ok_or("building floor_height must be a positive finite number")?;
                }
                _ => return Err(format!("unknown building option '{key}'")),
            }
        }
        Ok(style)
    }
}
