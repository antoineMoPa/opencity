#[derive(Clone, Debug, Default, PartialEq)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    Road(RoadDefinition),
    Block(BlockDefinition),
    Network(crate::network::Definition),
    Call(Call),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadDefinition {
    pub name: String,
    pub elements: Vec<RoadElement>,
    /// Permitted travel relative to each placement's start and end points.
    pub direction: RoadDirection,
    pub role: RoadRole,
}

/// Semantic role; independent of user-visible names and placement prefixes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoadRole {
    #[default]
    Ordinary,
    Ramp,
    Merge,
}
impl RoadRole {
    pub fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "ordinary" => Some(Self::Ordinary),
            "ramp" => Some(Self::Ramp),
            "merge" => Some(Self::Merge),
            _ => None,
        }
    }
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Ordinary => "ordinary",
            Self::Ramp => "ramp",
            Self::Merge => "merge",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoadDirection {
    #[default]
    Both,
    Forward,
    Reverse,
}

impl RoadDirection {
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        match keyword {
            "both" => Some(Self::Both),
            "forward" => Some(Self::Forward),
            "reverse" => Some(Self::Reverse),
            _ => None,
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Forward => "forward",
            Self::Reverse => "reverse",
        }
    }

    pub fn permits(self, from_start: bool) -> bool {
        match self {
            Self::Both => true,
            Self::Forward => from_start,
            Self::Reverse => !from_start,
        }
    }
}

impl RoadDefinition {
    /// Travel on a cross-section lane, relative to the placement's point order.
    /// With a median, left-hand lanes run in reverse and right-hand lanes forward.
    pub fn lane_direction(&self, element_index: usize) -> Option<RoadDirection> {
        let element = self.elements.get(element_index)?;
        if !element.kind.is_travel_lane() {
            return None;
        }
        if self.direction != RoadDirection::Both {
            return Some(self.direction);
        }
        match self
            .elements
            .iter()
            .position(|element| element.kind == RoadElementKind::Middle)
        {
            Some(median) if element_index < median => Some(RoadDirection::Reverse),
            Some(median) if element_index > median => Some(RoadDirection::Forward),
            _ => Some(RoadDirection::Both),
        }
    }
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
    Verge,
    Barrier,
    TrainTrack,
    TramTrack,
    SubwayTrack,
    Runway,
    Taxiway,
    Apron,
}

impl RoadElementKind {
    pub fn is_travel_lane(self) -> bool {
        matches!(
            self,
            Self::CarLane
                | Self::BikeLane
                | Self::BusLane
                | Self::TrainTrack
                | Self::TramTrack
                | Self::SubwayTrack
                | Self::Runway
                | Self::Taxiway
        )
    }
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
            "verge" => Self::Verge,
            "barrier" => Self::Barrier,
            "traintrack" => Self::TrainTrack,
            "tramtrack" => Self::TramTrack,
            "subwaytrack" => Self::SubwayTrack,
            "runway" => Self::Runway,
            "taxiway" => Self::Taxiway,
            "apron" => Self::Apron,
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
            Self::Verge => "verge",
            Self::Barrier => "barrier",
            Self::TrainTrack => "traintrack",
            Self::TramTrack => "tramtrack",
            Self::SubwayTrack => "subwaytrack",
            Self::Runway => "runway",
            Self::Taxiway => "taxiway",
            Self::Apron => "apron",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockDefinition {
    pub name: String,
    /// Destructured, typed object parameters; `None` keeps the legacy signature.
    pub parameters: Option<Vec<BlockParameter>>,
    pub calls: Vec<Call>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockParameter {
    pub name: String,
    pub kind: BlockParameterType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockParameterType {
    Number,
    String,
    Identifier,
}

impl BlockParameterType {
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        match keyword {
            "number" => Some(Self::Number),
            "string" => Some(Self::String),
            "identifier" => Some(Self::Identifier),
            _ => None,
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::String => "string",
            Self::Identifier => "identifier",
        }
    }
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
    /// A quoted asset path or other literal string.
    String(String),
    /// Named values, in source order.
    Object(Vec<(String, Argument)>),
}
