//! Typed sign designs shared by the editor, game, geometry cache and physics.
mod geometry;
mod source;
#[cfg(test)]
mod tests;
mod text;
#[cfg(feature = "expansion-ui")]
mod ui;
use super::*;
pub(super) use source::{option_allowed, read, write};
use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SignArrow {
    Left,
    Right,
    Straight,
    ExitRight,
}
impl SignArrow {
    pub const ALL: [Self; 4] = [Self::Left, Self::Right, Self::Straight, Self::ExitRight];
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Straight => "straight",
            Self::ExitRight => "exit_right",
        }
    }
    pub fn from_keyword(s: &str) -> Option<Self> {
        match s {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "straight" => Some(Self::Straight),
            "exit_right" => Some(Self::ExitRight),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SignDesign {
    Stop {
        cluster: bool,
    },
    Speed {
        limit: u16,
    },
    OneWay {
        arrow: SignArrow,
    },
    Yield,
    Curve {
        arrow: SignArrow,
    },
    Highway {
        destination: String,
        secondary: String,
        exit: String,
        arrow: SignArrow,
    },
}
#[derive(Clone, Copy, Debug)]
pub struct SignPartProfile {
    /// Metres in the assembly's local coordinates; Z is up.
    pub center: Vec3,
    pub half: Vec3,
    pub pole: bool,
    pub mass: f32,
}
impl SignDesign {
    pub fn for_kind(kind: Kind) -> Option<Self> {
        Some(match kind {
            Kind::StopSign => Self::Stop { cluster: false },
            Kind::SpeedSign => Self::Speed { limit: 50 },
            Kind::OneWaySign => Self::OneWay {
                arrow: SignArrow::Right,
            },
            Kind::YieldSign => Self::Yield,
            Kind::CurveSign => Self::Curve {
                arrow: SignArrow::Right,
            },
            Kind::HighwaySign => Self::Highway {
                destination: "Québec".into(),
                secondary: "Centre-ville".into(),
                exit: "12".into(),
                arrow: SignArrow::ExitRight,
            },
            _ => return None,
        })
    }
    pub fn kind(&self) -> Kind {
        match self {
            Self::Stop { .. } => Kind::StopSign,
            Self::Speed { .. } => Kind::SpeedSign,
            Self::OneWay { .. } => Kind::OneWaySign,
            Self::Yield => Kind::YieldSign,
            Self::Curve { .. } => Kind::CurveSign,
            Self::Highway { .. } => Kind::HighwaySign,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Speed { limit } if !(10..=130).contains(limit) || limit % 10 != 0 => {
                return Err("Sign speed must be a multiple of 10 between 10 and 130 km/h".into())
            }
            Self::OneWay { arrow } | Self::Curve { arrow }
                if !matches!(arrow, SignArrow::Left | SignArrow::Right) =>
            {
                return Err("This sign needs a left or right arrow".into())
            }
            Self::Highway {
                destination,
                secondary,
                exit,
                ..
            } => {
                for (label, value, max) in [
                    ("Destination", destination, 32),
                    ("Second line", secondary, 32),
                    ("Exit", exit, 12),
                ] {
                    if value.chars().count() > max
                        || value.chars().any(|c| c.is_control() || !text::has_glyph(c))
                    {
                        return Err(format!(
                            "{label}: unsupported character or more than {max} characters"
                        ));
                    }
                }
                if destination.chars().all(char::is_whitespace) {
                    return Err("A highway sign needs a destination".into());
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub fn profiles(&self) -> Vec<SignPartProfile> {
        geometry::profiles(self)
    }
    pub fn parts(&self) -> Vec<Arc<Mesh>> {
        struct Cached {
            design: SignDesign,
            parts: Vec<Arc<Mesh>>,
            bytes: usize,
        }
        #[derive(Default)]
        struct Cache {
            entries: VecDeque<Cached>,
            bytes: usize,
        }
        static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
        const BUDGET: usize = 8 * 1024 * 1024;
        let cache = CACHE.get_or_init(|| Mutex::new(Cache::default()));
        {
            let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(i) = c.entries.iter().position(|e| e.design == *self) {
                let e = c.entries.remove(i).unwrap();
                let parts = e.parts.clone();
                c.entries.push_back(e);
                return parts;
            }
        }
        let parts: Vec<_> = geometry::build(self)
            .into_iter()
            .map(|mut mesh| {
                for v in &mut mesh.vertices {
                    v.surface[1] = 1.;
                }
                Arc::new(mesh)
            })
            .collect();
        let bytes = parts
            .iter()
            .map(|p| p.vertices.len() * std::mem::size_of::<crate::runtime::Vertex>())
            .sum();
        let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = c.entries.iter().find(|e| e.design == *self) {
            return e.parts.clone();
        }
        if bytes <= BUDGET {
            while c.bytes + bytes > BUDGET || c.entries.len() >= 128 {
                if let Some(e) = c.entries.pop_front() {
                    c.bytes -= e.bytes;
                } else {
                    break;
                }
            }
            c.bytes += bytes;
            c.entries.push_back(Cached {
                design: self.clone(),
                parts: parts.clone(),
                bytes,
            });
        }
        parts
    }
}
