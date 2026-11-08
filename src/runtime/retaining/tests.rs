use super::*;

#[test]
fn walls_support_collision_without_blocking_the_overpass_or_joined_cuts() {
    let source = "elevation_map(\"a\",-2,-2,4,4,-1,1);water_level(0);
        terrain_cut(-0.2,-1,-0.4,1,-0.4,1,0.4,-1,0.4);
        terrain_cut(-0.2,-0.5,0.3,0.5,0.3,0.5,0.8,-0.5,0.8);
        road bridge: carlane carlane; end
        deck=bridge(0,-1,0,1);grade(deck,1.002,1.002);";
    let program = crate::parse(source).unwrap();
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1024, 1024);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![255; 1024 * 1024])
            .unwrap();
    }
    let city = City::from_program_with_elevation_loader(&program, |spec| {
        ElevationMap::from_png(spec.clone(), &bytes)
    })
    .unwrap();
    assert_eq!(city.surface_at([0., 0.], 0.).unwrap().height, -0.2);
    assert_ne!(
        city.surface_at([0., 0.], 0.).unwrap().kind,
        SurfaceKind::Water
    );
    assert!((city.surface_at([0., 0.], 2.).unwrap().height - 1.002).abs() < 1e-5);
    assert!(city
        .road_barrier_contact([0.8, 0.395], 0.01, -0.19, -0.1)
        .is_some());
    assert!(city
        .road_barrier_contact([0.8, 0.395], 0.01, 1.01, 1.05)
        .is_none());
    assert!(city
        .road_barrier_contact([0., 0.4], 0.01, -0.19, -0.1)
        .is_none());
    let mesh = city.mesh(Region::WholeMap, 0.005).unwrap();
    assert!(mesh
        .vertices
        .iter()
        .any(|v| v.surface[0] == SurfaceKind::Prop as u32 as f32 && v.position[2] < -0.19));
}
