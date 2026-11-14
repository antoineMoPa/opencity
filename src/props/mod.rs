//! Game-owned prop catalogue and ordinary `.road` placement calls.
mod model;
mod source;
use glam::{Mat4, Vec2, Vec3};
use opencity::runtime::{City, Mesh, SurfaceKind};
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
    Tree,
    Bench,
    Lamp,
}
impl Kind {
    pub const ALL: [Self; 4] = [Self::Cone, Self::Tree, Self::Bench, Self::Lamp];
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Cone => "cone",
            Self::Tree => "tree",
            Self::Bench => "bench",
            Self::Lamp => "lamp",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Cone => "Cone",
            Self::Tree => "Tree",
            Self::Bench => "Bench",
            Self::Lamp => "Lamp",
        }
    }
    pub fn from_keyword(s: &str) -> Option<Self> {
        match s {
            "cone" => Some(Self::Cone),
            "tree" => Some(Self::Tree),
            "bench" => Some(Self::Bench),
            "lamp" => Some(Self::Lamp),
            _ => None,
        }
    }
    pub fn radius(self) -> f32 {
        match self {
            Self::Cone => 0.0026,
            Self::Tree => 0.005,
            Self::Bench => 0.010,
            Self::Lamp => 0.0025,
        }
    }
    pub fn spacing(self) -> f32 {
        match self {
            Self::Cone => 2.5,
            Self::Tree => 6.,
            Self::Bench => 3.,
            Self::Lamp => 8.,
        }
    }
    pub fn parts(self) -> &'static [Arc<Mesh>] {
        model::parts(self)
    }
}
#[derive(Clone)]
pub struct Part {
    pub mesh: Arc<Mesh>,
    pub transform: Mat4,
    pub template: Option<(Kind, usize)>,
}
impl Part {
    pub fn baked(mesh: Mesh) -> Self {
        Self {
            mesh: Arc::new(mesh),
            transform: Mat4::IDENTITY,
            template: None,
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
}
impl Placement {
    pub fn transform(&self) -> Mat4 {
        Mat4::from_rotation_translation(
            glam::Quat::from_rotation_z(self.heading.to_radians()),
            self.position,
        )
    }
    pub fn parts(&self) -> Vec<Part> {
        self.kind
            .parts()
            .iter()
            .enumerate()
            .map(|(index, mesh)| Part {
                mesh: mesh.clone(),
                transform: self.transform(),
                template: Some((self.kind, index)),
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
                            },
                        ))
                    })
                    .collect()
            }
        }
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
        }
        .parts();
        let b = Placement {
            kind: Kind::Cone,
            position: Vec3::X,
            heading: 31.,
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
    #[test]
    fn catalogue_is_portable_complete_and_visible_in_the_instance_shader() {
        for kind in Kind::ALL {
            assert!(!kind.parts().is_empty());
            for part in kind.parts() {
                assert!(!part.vertices.is_empty());
                assert!(part
                    .vertices
                    .iter()
                    .all(|v| v.surface[1] == 1. && v.position.iter().all(|x| x.is_finite())));
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
