#[derive(Clone, Debug, Default, PartialEq)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    Road(RoadDefinition),
    Block(BlockDefinition),
    Call(Call),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadDefinition {
    pub name: String,
    pub elements: Vec<RoadElement>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadElement {
    pub kind: RoadElementKind,
    /// `$1` is represented as 1.
    pub catch: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoadElementKind {
    BusStop,
    Sidewalk,
    ParkingLane,
    BikeLane,
    BusLane,
    Middle,
    CarLane,
    BuildingEdge,
    Shoulder,
    Verge,
    Barrier,
}

impl RoadElementKind {
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        Some(match keyword {
            "busstop" => Self::BusStop,
            "sidewalk" => Self::Sidewalk,
            "parkinglane" => Self::ParkingLane,
            "bikelane" => Self::BikeLane,
            "buslane" => Self::BusLane,
            "middle" => Self::Middle,
            "carlane" => Self::CarLane,
            "buildingedge" => Self::BuildingEdge,
            "shoulder" => Self::Shoulder,
            "verge" => Self::Verge,
            "barrier" => Self::Barrier,
            _ => return None,
        })
    }

    pub fn keyword(self) -> &'static str {
        match self {
            Self::BusStop => "busstop",
            Self::Sidewalk => "sidewalk",
            Self::ParkingLane => "parkinglane",
            Self::BikeLane => "bikelane",
            Self::BusLane => "buslane",
            Self::Middle => "middle",
            Self::CarLane => "carlane",
            Self::BuildingEdge => "buildingedge",
            Self::Shoulder => "shoulder",
            Self::Verge => "verge",
            Self::Barrier => "barrier",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockDefinition {
    pub name: String,
    pub calls: Vec<Call>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub assigns: Vec<String>,
    pub name: String,
    pub args: Vec<Argument>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Argument {
    /// The source spelling is retained, so formatting a program preserves decimals.
    Number(String),
    Identifier(String),
}
