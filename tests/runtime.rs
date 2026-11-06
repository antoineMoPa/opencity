#![cfg(feature = "game-runtime")]
use opencity::{
    parse,
    runtime::{City, Region, SurfaceKind},
    RoadDirection,
};
fn city(source: &str) -> City {
    City::from_program(&parse(source).unwrap(), None).unwrap()
}

#[test]
fn external_models_replace_visual_proxies_but_preserve_support_and_collision() {
    let c = city(
        "ground=land(-1,-1,2,-1,2,2,-1,2); elevation(ground,0.02);
        sample=building(0.3,\"sample.mesh\",0,0,1,0,1,1,0,1);",
    );
    assert!(!c.mesh(Region::WholeMap, 0.04).unwrap().vertices.is_empty());
    let external = c.mesh_with_external_models(Region::WholeMap, 0.04).unwrap();
    assert!(!external.vertices.is_empty());
    assert!(!external
        .vertices
        .iter()
        .any(|v| v.surface[0] == SurfaceKind::Building as u32 as f32));
    assert!(c.surface_at([0.5, 0.5], 1.).unwrap().height.abs() < 0.0001);
    assert!(c.building_collision([0.5, 0.5], 0.01, 0.1, 0.2));
}
#[test]
fn structured_queries_include_footprints_directions_and_containment() {
    let c = city(
        "road street: sidewalk carlane sidewalk; direction forward; end \
            named=street(-1,0,1,0); home=building(0.3,-0.2,0.2,0.2,0.2,0.2,\
            0.6,-0.2,0.6);",
    );
    let all = c.query(Region::WholeMap);
    assert_eq!(all.roads.len(), 1);
    assert_eq!(all.buildings.len(), 1);
    assert_eq!(all.roads[0].1.segment.direction, RoadDirection::Forward);
    assert_eq!(
        all.roads[0].1.definition.lane_direction(1),
        Some(RoadDirection::Forward)
    );
    // Circle entirely within the building; no nearby polygon vertex.
    assert_eq!(
        c.query(Region::radius([0., 0.4], 0.001).unwrap())
            .buildings
            .len(),
        1
    );
    // Circle touches the road's outer sidewalk, away from its centerline.
    assert_eq!(
        c.query(Region::radius([0., 0.05], 0.001).unwrap())
            .roads
            .len(),
        1
    );
    assert!(c
        .query(Region::radius([5., 5.], 1.).unwrap())
        .roads
        .is_empty());
    assert!(Region::radius([f32::NAN, 0.], 1.).is_err());
    assert!(Region::radius([0., 0.], -1.).is_err());
    assert!(c
        .query(Region::Radius {
            center: [0., 0.],
            radius: f32::NAN
        })
        .roads
        .is_empty());
}
#[test]
fn stacked_road_support_respects_ceiling_and_grade() {
    let c = city(
        "road street: carlane; end low=street(-1,0,1,0); high=street(0,-1,\
            0,1); grade(low,0.1,0.3); elevation(high,1);",
    );
    let lower = c.surface_at([0., 0.], 0.5).unwrap();
    let upper = c.surface_at([0., 0.], 2.).unwrap();
    assert!((lower.height - 0.2).abs() < 1e-5, "{lower:?}");
    assert!((lower.gradient[0] - 0.1).abs() < 1e-4);
    assert!((upper.height - 1.0).abs() < 1e-5);
    assert_ne!(lower.road, upper.road);
    assert!(c.surface_at([0., 0.], 0.).is_none());
}
#[test]
fn mesh_and_support_share_surfaces_and_buildings_block_vehicle() {
    let c = city(
        "road street: sidewalk carlane sidewalk; end street(-1,0,1,0); \
            home=building(0.3,-0.2,0.2,0.2,0.2,0.2,0.6,-0.2,0.6);",
    );
    let asphalt = c.surface_at([0., 0.], 1.).unwrap();
    let sidewalk = c.surface_at([0., 0.04], 1.).unwrap();
    assert_eq!(asphalt.kind, SurfaceKind::Asphalt);
    assert_eq!(sidewalk.kind, SurfaceKind::Sidewalk);
    assert!(sidewalk.height > asphalt.height);
    let mesh = c
        .mesh(Region::radius([0., 0.], 0.2).unwrap(), 0.04)
        .unwrap();
    assert!(!mesh.vertices.is_empty());
    assert!(mesh
        .vertices
        .iter()
        .all(|v| v.position.iter().all(|n| n.is_finite())));
    assert!(c.building_collision([0., 0.4], 0.01, 0., 0.5));
    assert!(!c.building_collision([0., 0.4], 0.01, 1., 2.));
    let details = c.street_features(Region::WholeMap);
    assert!(!details.is_empty());
    let again = c.street_features(Region::WholeMap);
    assert_eq!(details.len(), again.len());
    assert!(details
        .iter()
        .zip(again)
        .all(|(a, b)| a.position == b.position));
    assert!(SurfaceKind::Grass.friction() < SurfaceKind::Asphalt.friction());
    assert!(SurfaceKind::Grass.rolling_resistance() > SurfaceKind::Asphalt.rolling_resistance());
}
#[test]
fn real_default_map_can_be_loaded_and_streamed_without_editor() {
    let c = City::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/default_map.road"
    ))
    .unwrap();
    assert_eq!(c.roads.len(), 592);
    assert_eq!(c.query(Region::WholeMap).buildings.len(), 1863);
    let p = c.roads[0].segment.point_at(0.5);
    let mesh = c
        .mesh(Region::radius([p.x, p.y], 0.5).unwrap(), 0.04)
        .unwrap();
    assert!(mesh.vertices.len() > 1000);
    assert!(c.surface_at([p.x, p.y], f32::INFINITY).is_some());
    assert!(c.components.iter().any(|c| c.id.starts_with("profile:")));
    assert!(!c.paths.is_empty());
}

#[test]
fn full_resolution_terrain_mesh_matches_png_support_triangles() {
    let c = City::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/default_map.road"
    ))
    .unwrap();
    let elevation = c.elevation.as_ref().unwrap();
    let mesh = c
        .mesh(Region::radius([-20., 14.], 0.2).unwrap(), 0.04)
        .unwrap();
    let mut checked = 0;
    for tri in mesh.vertices.chunks_exact(3) {
        if tri
            .iter()
            .all(|v| v.surface[0] == SurfaceKind::Grass as u32 as f32)
        {
            let p = std::array::from_fn::<_, 3, _>(|axis| {
                tri.iter().map(|v| v.position[axis]).sum::<f32>() / 3.
            });
            let expected = elevation.height_at([p[0], p[1]]).unwrap();
            assert!(
                (p[2] - expected).abs() < 0.0001,
                "mesh/support disagree: {p:?} vs {expected}"
            );
            checked += 1;
        }
    }
    assert!(checked > 0);
}

#[test]
fn flat_terrain_runs_shrink_without_changing_sloped_support() {
    let p = opencity::parse("elevation_map(\"test.png\",0,0,10,10,0,1);").unwrap();
    let mut samples = vec![10000_u16; 1024 * 1024];
    samples[512 * 1024 + 512] = 30000;
    let bytes: Vec<_> = samples.iter().flat_map(|v| v.to_be_bytes()).collect();
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 1024, 1024);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Sixteen);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&bytes)
            .unwrap();
    }
    let spec = opencity::heightmap::ElevationMapSpec::from_program(&p)
        .unwrap()
        .unwrap();
    let terrain = opencity::heightmap::ElevationMap::from_png(spec, &png).unwrap();
    let c = City::from_program_with_elevation_loader(&p, |_| Ok(terrain.clone())).unwrap();
    let mesh = c.mesh(Region::WholeMap, 0.005).unwrap();
    assert!(mesh.vertices.len() < 10_000);
    for tri in mesh.vertices.chunks_exact(3) {
        let p: [f32; 3] =
            std::array::from_fn(|axis| tri.iter().map(|v| v.position[axis]).sum::<f32>() / 3.);
        let h = terrain.surface_height_at([p[0], p[1]]).unwrap();
        assert!(
            (p[2] - h).abs() < 0.0001,
            "mesh/support disagree: {p:?} vs {h}"
        );
    }
    let peak = terrain.point(512, 512);
    let fine = c
        .mesh(Region::radius([peak[0], peak[1]], 0.06).unwrap(), 0.005)
        .unwrap();
    assert!(fine.vertices.iter().any(|v| v.position[2] > 0.2));
}

#[test]
fn building_collision_index_matches_footprint_queries_across_cells_and_heights() {
    let c = city(
        "wide=building(0.2,-1,-0.6,1,-0.6,1,0.6,-1,0.6);
        high=building(0.3,1.45,-0.1,1.55,-0.1,1.55,0.1,1.45,0.1); elevation(high,0.5);
        corner=building(0.1,-0.6,1.4,-0.4,1.4,-0.4,1.6,-0.6,1.6);",
    );
    for x in -20..=20 {
        for y in -20..=20 {
            let point = [x as f32 * 0.1 + 0.003, y as f32 * 0.1 + 0.007];
            for radius in [0., 0.01, 0.3, 0.7, 40.] {
                for (bottom, top) in [(0., 0.15), (0.25, 0.45), (0.6, 0.7)] {
                    let expected = c
                        .query(Region::radius(point, radius).unwrap())
                        .buildings
                        .iter()
                        .any(|(_, f)| {
                            f.elevation < top && f.elevation + f.height.unwrap_or(0.) > bottom
                        });
                    assert_eq!(
                        c.building_collision(point, radius, bottom, top),
                        expected,
                        "point={point:?}, radius={radius}, height={bottom}..{top}"
                    );
                }
            }
        }
    }
}
