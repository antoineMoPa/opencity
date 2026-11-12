//! Concrete superstructures follow the same adaptive rows as the road surface.
use super::*;
use crate::road_geometry::{CrossSection, RoadGeometry};
const DEPTH: f32 = crate::road_geometry::DECK_DEPTH;
const MIN_CLEARANCE: f32 = 0.02;
const PIER_SPACING: f32 = 0.24;
const CONCRETE: [f32; 3] = [0.32, 0.34, 0.33];

fn ground(city: &City, features: &[&crate::LandscapeFeature], p: [f32; 2]) -> f32 {
    let terrain = city.elevation.as_ref().and_then(|e| e.height_at(p));
    features
        .iter()
        .filter(|f| super::geometry::contains(&f.points, p))
        .map(|f| f.elevation - 0.02 + f.height.unwrap_or(0.))
        .chain(terrain)
        .max_by(f32::total_cmp)
        .unwrap_or(0.)
}
fn edges(row: &CrossSection) -> [[f32; 3]; 2] {
    [row.boundaries[0], *row.boundaries.last().unwrap()]
        .map(|v| [v.point.x, v.point.y, v.height - 0.0002])
}
fn elevated(city: &City, features: &[&crate::LandscapeFeature], row: &CrossSection) -> bool {
    edges(row)
        .iter()
        .all(|p| p[2] - ground(city, features, [p[0], p[1]]) > MIN_CLEARANCE)
}

pub(super) fn mesh(city: &City, geometry: &RoadGeometry) -> Mesh {
    let mut mesh = Mesh::default();
    // Ground never depends on building shells. Filter them once rather than
    // visiting every building for each adaptive deck-edge sample.
    let features: Vec<_> = city
        .landscape
        .iter()
        .filter(|f| !matches!(f.kind, LandscapeKind::Building | LandscapeKind::Vegetation))
        .collect();
    for id in 0..city.roads.len() {
        let rows = geometry.cross_sections(id);
        let mut since_pier = PIER_SPACING;
        for pair in rows.windows(2) {
            let [a, b] = edges(&pair[0]);
            let [c, d] = edges(&pair[1]);
            let center =
                |a: [f32; 3], b: [f32; 3]| std::array::from_fn::<_, 3, _>(|i| (a[i] + b[i]) * 0.5);
            let p = center(a, b);
            let q = center(c, d);
            since_pier += (q[0] - p[0]).hypot(q[1] - p[1]);
            if !elevated(city, &features, &pair[0]) || !elevated(city, &features, &pair[1]) {
                continue;
            }
            let bottom = |mut p: [f32; 3]| {
                p[2] -= DEPTH;
                p
            };
            // Pavement closes the slab above. A second cap just below it
            // fights the pavement's depth at distant views of elevated roads.
            let slab_start = mesh.vertices.len();
            for face in [
                [bottom(b), bottom(d), bottom(c), bottom(a)],
                [a, bottom(a), bottom(c), c],
                [b, d, bottom(d), bottom(b)],
                [a, b, bottom(b), bottom(a)],
                [c, bottom(c), bottom(d), d],
            ] {
                mesh.quad(face, CONCRETE, SurfaceKind::Prop);
            }
            super::surface_weathering::concrete_mesh(&mut mesh.vertices[slab_start..], |point| {
                super::surface_weathering::base(p, q, point) - DEPTH
            });
            if since_pier < PIER_SPACING {
                continue;
            }
            let top = p[2] - DEPTH;
            let foot = ground(city, &features, [p[0], p[1]]);
            // Leave lower roads, footpaths and buildings clear. Try again on the
            // next adaptive row when a planned pier lands in a crossing.
            let blocked = [-0.014, 0., 0.014].into_iter().any(|x| {
                [-0.014, 0., 0.014].into_iter().any(|y| {
                    let point = [p[0] + x, p[1] + y];
                    city.surface_at(point, top - 0.004).is_some_and(|s| {
                        matches!(s.kind, SurfaceKind::Asphalt | SurfaceKind::Sidewalk)
                            || s.height > foot + 0.004
                    }) || city.building_collision(point, 0.014, foot, top)
                })
            });
            if blocked {
                continue;
            }
            let start = mesh.vertices.len();
            let width = (b[0] - a[0]).hypot(b[1] - a[1]);
            if width < 1e-6 {
                continue;
            }
            // Flared cap and a broad column on a spread footing.
            mesh.cuboid(
                [-0.012, -0.012, foot - 0.002],
                [0.012, 0.012, foot + 0.003],
                CONCRETE,
                SurfaceKind::Prop,
            );
            mesh.cuboid(
                [-0.007, -0.009, foot],
                [0.007, 0.009, top - 0.004],
                CONCRETE,
                SurfaceKind::Prop,
            );
            mesh.cuboid(
                [-0.012, -width * 0.43, top - 0.004],
                [0.012, width * 0.43, top],
                CONCRETE,
                SurfaceKind::Prop,
            );
            let mut pier = Mesh {
                vertices: mesh.vertices[start..].to_vec(),
            };
            super::surface_weathering::concrete_mesh(&mut pier.vertices, |_| foot);
            super::ambient_occlusion::bake_cuboids(&mut pier, 0.012, Some(foot));
            mesh.vertices[start..].copy_from_slice(&pier.vertices);
            let across = [(b[0] - a[0]) / width, (b[1] - a[1]) / width];
            for v in &mut mesh.vertices[start..] {
                let [x, y, z] = v.position;
                v.position = [
                    p[0] + across[1] * x + across[0] * y,
                    p[1] - across[0] * x + across[1] * y,
                    z,
                ];
                let [x, y, z] = v.normal;
                v.normal = [
                    across[1] * x + across[0] * y,
                    -across[0] * x + across[1] * y,
                    z,
                ];
            }
            since_pier = 0.;
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    fn city(source: &str) -> City {
        City::from_program(&crate::parse(source).unwrap(), None).unwrap()
    }
    fn props(city: &City) -> Vec<&Triangle> {
        city.triangles
            .iter()
            .filter(|t| t.kind == SurfaceKind::Prop)
            .collect()
    }
    #[test]
    fn elevated_deck_has_concrete_soffit_and_piers_but_ground_road_does_not() {
        let flat = city("road r: carlane; end r(0,0,1,0);");
        assert!(props(&flat).is_empty());
        let bridge = city("road r: carlane; end s=r(0,0,1,0); elevation(s,0.08);");
        let concrete = props(&bridge);
        assert!(!concrete.is_empty());
        assert!(concrete.iter().any(|t| t
            .points
            .iter()
            .all(|p| (p[2] - (0.08 - 0.0002 - DEPTH)).abs() < 1e-5)));
        assert!(concrete
            .iter()
            .any(|t| t.points.iter().any(|p| p[2] < 0.001)));
        assert!(
            concrete
                .iter()
                .all(|t| !t.points.iter().all(|p| (0.079..0.079999).contains(&p[2]))),
            "a concrete cap immediately below asphalt causes distant depth flicker"
        );
        assert!((bridge.surface_at([0.5, 0.], 0.09).unwrap().height - 0.08).abs() < 1e-5);
        assert!(bridge.surface_at([0.5, 0.], 0.07).is_none());
        assert!(concrete
            .iter()
            .flat_map(|t| t.points)
            .flatten()
            .all(f32::is_finite));
    }
    #[test]
    fn pier_footings_reach_authored_land_below_zero() {
        let bridge = city(
            "road r: carlane; end s=r(0,0,1,0); elevation(s,0.08); land(-1,-1,2,-1,2,1,-1,1);",
        );
        let lowest = props(&bridge)
            .iter()
            .flat_map(|t| t.points)
            .map(|p| p[2])
            .min_by(f32::total_cmp)
            .unwrap();
        assert!((lowest + 0.022).abs() < 1e-5);
    }

    #[test]
    fn crossing_road_stays_clear_of_piers_and_keeps_lower_support() {
        let bridge =
            city("road r: carlane; end r(-0.1,-1,-0.1,1); s=r(-1,0,1,0); elevation(s,0.08);");
        let lower = bridge.surface_at([-0.1, 0.], 0.06).unwrap();
        assert!(lower.height.abs() < 1e-5);
        let upper = bridge.surface_at([-0.1, 0.], 0.09).unwrap();
        assert!((upper.height - 0.08).abs() < 1e-5);
        for t in props(&bridge) {
            for p in t.points {
                if p[2] < 0.04 {
                    assert!(
                        (p[0] + 0.1).abs() > 0.015,
                        "pier obstructs lower lane: {p:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn graded_curved_decks_and_barriers_have_finite_geometry() {
        let bridge = city(
            "road r: sidewalk carlane sidewalk; end s=r(0,0,0.2,0.8,0.8,0.2,1,\
                1); grade(s,0.04,0.12);",
        );
        let concrete = props(&bridge);
        assert!(!concrete.is_empty());
        assert!(concrete
            .iter()
            .flat_map(|t| t.points)
            .all(|p| p.iter().all(|v| v.is_finite())
                && p[2] <= 0.12 + crate::road_geometry::BARRIER_HEIGHT + 1e-6));
    }
}
