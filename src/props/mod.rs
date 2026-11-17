//! Game-owned prop catalogue and ordinary `.road` placement calls.
mod model;
mod signs;
mod visibility;
pub use visibility::visible_placements;
mod source;
use glam::{Mat4, Vec2, Vec3};
use opencity::runtime::{City, Mesh, SurfaceKind};
pub use signs::{SignArrow, SignDesign, SignPartProfile};
pub use source::{apply, read, write};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub const UNITS: f32 = 100.;
pub const MAX_ROW: usize = 512;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Cone,
    RandomTree,
    Tree,
    TreeMaple2,
    TreeLinden,
    TreeLinden2,
    TreeLocust,
    TreeLocust2,
    Bench,
    TrashCan,
    Lamp,
    BusStopSign,
    StopSign,
    SpeedSign,
    OneWaySign,
    YieldSign,
    CurveSign,
    HighwaySign,
    ParkingSign,
}
impl Kind {
    pub const ALL: [Self; 13] = [
        Self::Cone,
        Self::RandomTree,
        Self::Bench,
        Self::TrashCan,
        Self::Lamp,
        Self::BusStopSign,
        Self::StopSign,
        Self::SpeedSign,
        Self::OneWaySign,
        Self::YieldSign,
        Self::CurveSign,
        Self::HighwaySign,
        Self::ParkingSign,
    ];
    pub const fn is_road_sign(self) -> bool {
        matches!(
            self,
            Self::StopSign
                | Self::SpeedSign
                | Self::OneWaySign
                | Self::YieldSign
                | Self::CurveSign
                | Self::HighwaySign
                | Self::ParkingSign
        )
    }
    pub fn is_tree(self) -> bool {
        matches!(
            self,
            Self::RandomTree
                | Self::Tree
                | Self::TreeMaple2
                | Self::TreeLinden
                | Self::TreeLinden2
                | Self::TreeLocust
                | Self::TreeLocust2
        )
    }
    pub fn street_tree(position: [f32; 3]) -> Self {
        let x = (position[0] * 10000.0).floor() as i32 as u32;
        let y = (position[1] * 12000.0).floor() as i32 as u32;
        let mut choice = x.wrapping_mul(0x9e37_79b9) ^ y.wrapping_mul(0x85eb_ca6b) ^ 0x27d4_eb2d;
        choice ^= choice >> 16;
        choice = choice.wrapping_mul(0x7feb_352d);
        choice ^= choice >> 15;
        let choice = choice as usize % 6;
        match choice {
            0 => Self::Tree,
            1 => Self::TreeMaple2,
            2 => Self::TreeLinden,
            3 => Self::TreeLinden2,
            4 => Self::TreeLocust,
            _ => Self::TreeLocust2,
        }
    }
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Cone => "cone",
            Self::RandomTree => "random_tree",
            Self::Tree => "tree",
            Self::TreeMaple2 => "tree_maple_2",
            Self::TreeLinden => "tree_linden",
            Self::TreeLinden2 => "tree_linden_2",
            Self::TreeLocust => "tree_locust",
            Self::TreeLocust2 => "tree_locust_2",
            Self::Bench => "bench",
            Self::TrashCan => "trash_can",
            Self::Lamp => "lamp",
            Self::BusStopSign => "bus_stop_sign",
            Self::HighwaySign => "highway_sign",
            Self::ParkingSign => "parking_sign",
            Self::CurveSign => "curve_sign",
            Self::YieldSign => "yield_sign",
            Self::OneWaySign => "one_way_sign",
            Self::SpeedSign => "speed_sign",
            Self::StopSign => "stop_sign",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Cone => "Cone",
            Self::RandomTree => "Random Tree",
            Self::Tree => "Broad maple",
            Self::TreeMaple2 => "Broad maple 2",
            Self::TreeLinden => "Upright maple",
            Self::TreeLinden2 => "Upright maple 2",
            Self::TreeLocust => "Young maple",
            Self::TreeLocust2 => "Young maple 2",
            Self::Bench => "Bench",
            Self::TrashCan => "Trash Can",
            Self::Lamp => "Lamp",
            Self::BusStopSign => "Bus stop sign",
            Self::HighwaySign => "Autoroute / exit / destination",
            Self::ParkingSign => "Stationnement / parking",
            Self::CurveSign => "Curve indicator",
            Self::YieldSign => "Cédez le passage",
            Self::OneWaySign => "Sens unique",
            Self::SpeedSign => "Maximum speed",
            Self::StopSign => "Arrêt",
        }
    }
    pub fn from_keyword(s: &str) -> Option<Self> {
        match s {
            "cone" => Some(Self::Cone),
            "random_tree" => Some(Self::RandomTree),
            "tree" => Some(Self::Tree),
            "tree_maple_2" => Some(Self::TreeMaple2),
            "tree_linden" => Some(Self::TreeLinden),
            "tree_linden_2" => Some(Self::TreeLinden2),
            "tree_locust" => Some(Self::TreeLocust),
            "tree_locust_2" => Some(Self::TreeLocust2),
            "bench" => Some(Self::Bench),
            "trash_can" => Some(Self::TrashCan),
            "lamp" => Some(Self::Lamp),
            "bus_stop_sign" => Some(Self::BusStopSign),
            "highway_sign" => Some(Self::HighwaySign),
            "parking_sign" => Some(Self::ParkingSign),
            "curve_sign" => Some(Self::CurveSign),
            "yield_sign" => Some(Self::YieldSign),
            "one_way_sign" => Some(Self::OneWaySign),
            "speed_sign" => Some(Self::SpeedSign),
            "stop_sign" => Some(Self::StopSign),
            _ => None,
        }
    }
    pub fn radius(self) -> f32 {
        match self {
            Self::Cone => 0.0026,
            Self::RandomTree
            | Self::Tree
            | Self::TreeMaple2
            | Self::TreeLinden
            | Self::TreeLinden2
            | Self::TreeLocust
            | Self::TreeLocust2 => 0.005,
            Self::Bench => 0.010,
            Self::TrashCan => 0.004,
            Self::Lamp => 0.0025,
            Self::BusStopSign => 0.0025,
            Self::HighwaySign => 0.025,
            Self::ParkingSign => 0.003,
            Self::StopSign
            | Self::SpeedSign
            | Self::OneWaySign
            | Self::YieldSign
            | Self::CurveSign => 0.005,
        }
    }
    pub fn spacing(self) -> f32 {
        match self {
            Self::Cone => 2.5,
            Self::RandomTree
            | Self::Tree
            | Self::TreeMaple2
            | Self::TreeLinden
            | Self::TreeLinden2
            | Self::TreeLocust
            | Self::TreeLocust2 => 6.,
            Self::Bench => 3.,
            Self::TrashCan => 2.,
            Self::Lamp => 8.,
            Self::BusStopSign => 12.,
            Self::HighwaySign => 20.,
            Self::ParkingSign => 45.,
            Self::StopSign
            | Self::SpeedSign
            | Self::OneWaySign
            | Self::YieldSign
            | Self::CurveSign => 8.,
        }
    }
    pub fn parts(self) -> &'static [Arc<Mesh>] {
        model::parts(self)
    }
    pub fn far_parts(self) -> &'static [Arc<Mesh>] {
        model::far_parts(self)
    }
    pub fn image_parts(self) -> &'static [Arc<Mesh>] {
        model::image_parts(self)
    }
}
#[derive(Clone)]
pub struct Instance {
    pub kind: Kind,
    pub index: usize,
    pub mesh: Arc<Mesh>,
    pub transform: Mat4,
    pub automatic: bool,
}
#[derive(Clone)]
pub struct Part {
    pub mesh: Arc<Mesh>,
    pub transform: Mat4,
    pub template: Option<(Kind, usize)>,
    pub automatic: bool,
    pub sign_profile: Option<SignPartProfile>,
}
impl Part {
    pub fn baked(mesh: Mesh) -> Self {
        Self {
            mesh: Arc::new(mesh),
            transform: Mat4::IDENTITY,
            template: None,
            automatic: false,
            sign_profile: None,
        }
    }
    pub fn vertex(&self, mut v: opencity::runtime::Vertex) -> opencity::runtime::Vertex {
        v.position = self
            .transform
            .transform_point3(Vec3::from_array(v.position))
            .to_array();
        v.normal = self
            .transform
            .transform_vector3(Vec3::from_array(v.normal))
            .to_array();
        v
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub kind: Kind,
    pub position: Vec3,
    pub heading: f32,
    pub sign: Option<SignDesign>,
}
impl Placement {
    pub fn transform(&self) -> Mat4 {
        Mat4::from_rotation_translation(
            glam::Quat::from_rotation_z(self.heading.to_radians()),
            self.position,
        )
    }
    pub fn parts(&self) -> Vec<Part> {
        let model_kind = match self.kind {
            Kind::RandomTree => Kind::street_tree(self.position.to_array()),
            concrete => concrete,
        };
        let design = self
            .sign
            .clone()
            .or_else(|| SignDesign::for_kind(model_kind));
        let profiles = design
            .as_ref()
            .map(SignDesign::profiles)
            .unwrap_or_default();
        let meshes = design
            .as_ref()
            .map(SignDesign::parts)
            .unwrap_or_else(|| model_kind.parts().to_vec());
        meshes
            .iter()
            .enumerate()
            .map(|(index, mesh)| Part {
                mesh: mesh.clone(),
                transform: self.transform(),
                template: Some((model_kind, index)),
                automatic: false,
                sign_profile: profiles.get(index).copied(),
            })
            .collect()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub a: Vec2,
    pub control: Vec2,
    pub b: Vec2,
    pub elevation: f32,
    pub spacing: f32,
    pub heading: f32,
    pub offsets: BTreeMap<usize, Vec3>,
    pub hidden: BTreeSet<usize>,
    pub sign: Option<SignDesign>,
}
impl Row {
    pub fn point(&self, t: f32) -> Vec2 {
        self.a * (1. - t).powi(2) + self.control * (2. * t * (1. - t)) + self.b * t * t
    }
    pub fn points(&self) -> Result<Vec<(usize, Vec2, f32)>, String> {
        if ![self.a, self.b, self.control].iter().all(|p| p.is_finite())
            || !self.spacing.is_finite()
            || self.spacing < 0.6
            || !self.elevation.is_finite()
            || !self.heading.is_finite()
        {
            return Err("Invalid prop row geometry or spacing (minimum 0.6 m)".into());
        }
        let mut lengths = vec![0.];
        for i in 1..=128 {
            lengths.push(
                lengths[i - 1]
                    + self
                        .point(i as f32 / 128.)
                        .distance(self.point((i - 1) as f32 / 128.)),
            );
        }
        let intervals = (lengths[128] * UNITS / self.spacing).floor();
        if !intervals.is_finite() || intervals >= MAX_ROW as f32 {
            return Err(format!(
                "A prop row supports at most {MAX_ROW} objects; increase spacing"
            ));
        }
        let count = intervals as usize + 1;
        let mut result = Vec::with_capacity(count);
        let mut cursor = 1;
        for i in 0..count {
            if self.hidden.contains(&i) {
                continue;
            }
            let along = i as f32 * self.spacing / UNITS;
            while cursor < 128 && lengths[cursor] < along {
                cursor += 1;
            }
            let t = ((cursor - 1) as f32
                + (along - lengths[cursor - 1])
                    / (lengths[cursor] - lengths[cursor - 1]).max(1e-9))
                / 128.;
            let offset = self.offsets.get(&i).copied().unwrap_or_default();
            if !offset.is_finite() {
                return Err("Invalid prop offset".into());
            }
            result.push((
                i,
                self.point(t) + offset.truncate(),
                self.heading + offset.z,
            ));
        }
        Ok(result)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Single(Placement),
    Row { kind: Kind, row: Row },
}
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub shape: Shape,
}
impl Entry {
    pub fn kind(&self) -> Kind {
        match &self.shape {
            Shape::Single(p) => p.kind,
            Shape::Row { kind, .. } => *kind,
        }
    }
    pub fn placements(&self, city: &City) -> Result<Vec<(usize, Placement)>, String> {
        match &self.shape {
            Shape::Single(p) => Ok(vec![(0, p.clone())]),
            Shape::Row { kind, row } => {
                let mut elevation = row.elevation;
                row.points()?
                    .into_iter()
                    .map(|(i, p, heading)| {
                        let surface = city
                            .surface_at(p.to_array(), elevation + 0.03)
                            .ok_or("Prop row needs a supported surface")?;
                        if surface.kind == SurfaceKind::Water {
                            return Err("Props need a dry surface".into());
                        }
                        elevation = surface.height;
                        Ok((
                            i,
                            Placement {
                                kind: *kind,
                                position: p.extend(surface.height),
                                heading,
                                sign: row.sign.clone(),
                            },
                        ))
                    })
                    .collect()
            }
        }
    }
    pub fn sign_mut(&mut self) -> Option<&mut SignDesign> {
        let kind = self.kind();
        let value = match &mut self.shape {
            Shape::Single(p) => &mut p.sign,
            Shape::Row { row, .. } => &mut row.sign,
        };
        if value.is_none() {
            *value = SignDesign::for_kind(kind);
        }
        value.as_mut()
    }
    pub fn translate(&mut self, delta: Vec2) {
        match &mut self.shape {
            Shape::Single(p) => p.position += delta.extend(0.),
            Shape::Row { row, .. } => {
                row.a += delta;
                row.b += delta;
                row.control += delta;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use opencity::Program;
    #[test]
    fn random_tree_calls_keep_their_kind_and_resolve_stable_variants() {
        let source = "a=prop(random_tree,0,0); b=prop(random_tree,0.0002,0);";
        let program = opencity::parse(source).unwrap();
        let entries = read(&program).unwrap();
        let variants: Vec<_> = entries
            .iter()
            .map(|entry| {
                assert_eq!(entry.kind(), Kind::RandomTree);
                let Shape::Single(placement) = &entry.shape else {
                    panic!()
                };
                let concrete = placement.parts()[0].template.unwrap().0;
                assert_eq!(concrete, Kind::street_tree(placement.position.to_array()));
                concrete
            })
            .collect();
        assert_eq!(variants.len(), 2);
        let row_variants: std::collections::HashSet<_> = (0..12)
            .map(|i| Kind::street_tree([i as f32 * 0.06, 0., 0.]))
            .collect();
        assert!(
            row_variants.len() >= 3,
            "regular tree rows should mix variants"
        );
        let exported = Program {
            statements: entries
                .iter()
                .map(|entry| opencity::Statement::Call(write(entry)))
                .collect(),
        };
        assert_eq!(read(&exported).unwrap(), entries);
    }
    #[test]
    fn authored_rows_round_trip_with_free_angles_offsets_and_hidden_members() {
        let program=opencity::parse("a=prop_row(cone,0,0,0.05,0.04,0.13,0.02,{spacing:2.5,offsets:{p1:{x:0.002,y:0.003,heading:17}},hidden:{p2:true}});").unwrap();
        let entries = read(&program).unwrap();
        let Shape::Row { row, .. } = &entries[0].shape else {
            panic!()
        };
        assert!(row.points().unwrap().len() > 3);
        assert!(!row.points().unwrap().iter().any(|(i, _, _)| *i == 2));
        let exported = Program {
            statements: entries
                .iter()
                .map(|e| opencity::Statement::Call(write(e)))
                .collect(),
        };
        assert_eq!(read(&exported).unwrap(), entries);
        assert_eq!(
            read(&opencity::parse(&exported.to_road_source()).unwrap()).unwrap(),
            entries
        );
    }
    #[test]
    fn bad_kinds_and_unbounded_rows_are_rejected() {
        for source in [
            "a=prop(unknown,0,0);",
            "a=prop_row(cone,0,0,1,1,2,2,{spacing:0});",
            "a=prop_row(cone,0,0,10,10,20,20,{spacing:0.6});",
            "a=prop(cone,0,0,{collision:none});",
        ] {
            assert!(read(&opencity::parse(source).unwrap()).is_err(), "{source}");
        }
    }
    #[test]
    fn approved_cone_has_exact_scale_white_bands_and_shared_geometry() {
        let a = Placement {
            kind: Kind::Cone,
            position: Vec3::ZERO,
            heading: 0.,
            sign: None,
        }
        .parts();
        let b = Placement {
            kind: Kind::Cone,
            position: Vec3::X,
            heading: 31.,
            sign: None,
        }
        .parts();
        assert!(Arc::ptr_eq(&a[0].mesh, &b[0].mesh));
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for v in &a[0].mesh.vertices {
            let p = Vec3::from_array(v.position);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        assert!((hi.z * UNITS - 1.).abs() < 1e-5);
        assert!(((hi.x - lo.x) * UNITS - 0.5).abs() < 1e-5);
        for (low, high) in [(0.4, 0.55), (0.68, 0.83)] {
            let band: Vec<_> = a[0]
                .mesh
                .vertices
                .iter()
                .filter(|v| v.color[0] > 0.9 && v.color[1] > 0.9 && v.color[2] > 0.9)
                .filter(|v| {
                    v.position[2] * UNITS >= low - 1e-5 && v.position[2] * UNITS <= high + 1e-5
                })
                .collect();
            assert!(!band.is_empty());
        }
        assert!(a[0].mesh.vertices.len() < 2200);
    }
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    use crate::Program;
    #[test]
    fn street_furniture_can_be_authored_and_keeps_its_bundled_scale() {
        let program = opencity::parse(
            "seat=prop(bench,0,0,{heading:30}); bin=prop(trash_can,0.02,0,{heading:90});",
        )
        .unwrap();
        let entries = read(&program).unwrap();
        assert_eq!(
            entries.iter().map(Entry::kind).collect::<Vec<_>>(),
            [Kind::Bench, Kind::TrashCan]
        );
        for (kind, expected_height) in [(Kind::Bench, 0.945821), (Kind::TrashCan, 0.995)] {
            let vertices = &kind.parts()[0].vertices;
            let low = vertices
                .iter()
                .map(|v| v.position[2])
                .fold(f32::INFINITY, f32::min);
            let high = vertices
                .iter()
                .map(|v| v.position[2])
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(((high - low) * UNITS - expected_height).abs() < 0.005);
        }
        let exported = Program {
            statements: entries
                .iter()
                .map(|entry| opencity::Statement::Call(write(entry)))
                .collect(),
        };
        assert_eq!(read(&exported).unwrap(), entries);
    }
    #[test]
    fn catalogue_is_portable_complete_and_visible_in_the_instance_shader() {
        for kind in Kind::ALL.into_iter().chain([
            Kind::Tree,
            Kind::TreeMaple2,
            Kind::TreeLinden,
            Kind::TreeLinden2,
            Kind::TreeLocust,
            Kind::TreeLocust2,
        ]) {
            assert!(!kind.parts().is_empty());
            for part in kind.parts() {
                assert!(!part.vertices.is_empty());
                assert!(part
                    .vertices
                    .iter()
                    .all(|v| (kind.is_tree() || v.surface[1] == 1.)
                        && v.position.iter().all(|x| x.is_finite())));
            }
            if kind.is_tree() {
                let near = &kind.parts()[0].vertices;
                let far = &kind.far_parts()[0].vertices;
                let image = &kind.image_parts()[0].vertices;
                assert!(
                    far.len() < near.len(),
                    "far tree should reduce submitted vertices"
                );
                assert_eq!(
                    image.len(),
                    9,
                    "image tree is two visible triangles plus culling bounds"
                );
                assert!(near.iter().any(|v| (15.0..=17.0).contains(&v.surface[0])));
                assert!(near.iter().any(|v| (19.0..=23.0).contains(&v.surface[0])));
            }
        }
    }
    #[test]
    fn source_updates_preserve_non_prop_statements() {
        let mut p = opencity::parse(
            "road r: carlane; end r(0,0,1,0); a=prop(cone,0,0); b=prop(bench,0.1,0);",
        )
        .unwrap();
        let before = p.statements[..2].to_vec();
        let mut entries = read(&p).unwrap();
        entries.remove(0);
        entries[0].translate(Vec2::new(0.01, 0.02));
        apply(&mut p, &entries).unwrap();
        assert_eq!(p.statements[..2], before);
        assert_eq!(read(&p).unwrap(), entries);
    }
}

#[cfg(test)]
mod grade_tests {
    use super::*;
    #[test]
    fn rows_follow_uphill_roads_without_jumping_to_an_overpass() {
        let program=opencity::parse("road r: carlane; end a=r(0,0,0.5,0); grade(a,0,0.1); land(-2,-2,2,-2,2,2,-2,2); line=prop_row(cone,0,0,0.25,0,0.5,0,{spacing:2.5,elevation:0});").unwrap();
        let city = City::from_program(&program, None).unwrap();
        let points = read(&program).unwrap()[0].placements(&city).unwrap();
        assert!(points.last().unwrap().1.position.z > 0.09);
        let program=opencity::parse("road r: carlane; end low=r(0,-1,0,1); high=r(-1,0,1,0); grade(high,0.08,0.08); land(-2,-2,2,-2,2,2,-2,2); line=prop_row(cone,0,-0.1,0,0,0,0.1,{spacing:2.5,elevation:0});").unwrap();
        let city = City::from_program(&program, None).unwrap();
        assert!(read(&program).unwrap()[0]
            .placements(&city)
            .unwrap()
            .iter()
            .all(|(_, p)| p.position.z < 0.03));
    }
}
