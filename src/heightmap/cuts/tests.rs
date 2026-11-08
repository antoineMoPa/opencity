use super::*;

pub(crate) fn map(calls: &str) -> ElevationMap {
    let program = crate::parse(&format!(
        "elevation_map(\"height.png\",-2,-2,4,4,-1,1); water_level(0); {calls}"
    ))
    .unwrap();
    let spec = ElevationMapSpec::from_program(&program).unwrap().unwrap();
    let bytes = crate::heightmap::tests::png_bytes(
        png::BitDepth::Eight,
        1024,
        png::ColorType::Grayscale,
        &vec![255; 1024 * 1024],
    );
    ElevationMap::from_png(spec, &bytes).unwrap()
}

#[test]
fn subpixel_cuts_are_exact_dry_and_leave_the_png_unchanged() {
    let map = map("terrain_cut(-0.2, -0.0003,-1, 0.0003,-1, 0.0003,1, -0.0003,1);");
    assert_eq!(map.height_at([0., 0.]), Some(-0.2));
    assert_eq!(map.surface_height_at([0., 0.]), Some(-0.2));
    assert_eq!(map.original_height_at([0., 0.]), Some(1.));
    assert_eq!(map.height_at([0.0004, 0.]), Some(1.));
    assert_eq!(map.height_at([3., 0.]), None);
    assert_eq!(
        map.surface_maximum_in([-0.0001, -0.5], [0.0001, 0.5]),
        Some(-0.2)
    );
    map.for_each_water_triangle(|triangle| {
        let centroid = [0, 1].map(|axis| triangle.iter().map(|p| p[axis]).sum::<f32>() / 3.);
        assert!(map.cut_at(centroid).is_none());
    });
}

#[test]
fn clipped_terrain_preserves_area_and_matches_graded_support() {
    let map = map("terrain_cut(-0.4, {slope_x:0.2}, -0.5,-1, 0.5,-1, 0.5,1, -0.5,1);");
    assert!((map.height_at([0., 0.]).unwrap() + 0.3).abs() < 1e-6);
    let mut area = 0.;
    map.for_each_triangle(|triangle| {
        let [a, b, c] = triangle;
        area += ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5;
        let p = [0, 1].map(|axis| triangle.iter().map(|p| p[axis]).sum::<f32>() / 3.);
        let height = triangle.iter().map(|p| p[2]).sum::<f32>() / 3.;
        assert!((height - map.height_at(p).unwrap()).abs() < 1e-5);
    });
    assert!((area - 16.).abs() < 1e-4, "{area}");
}

#[test]
fn overlapping_planes_preserve_the_lower_floor_and_never_raise_ground() {
    let map = map("terrain_cut(0, {slope_x:0.5}, -1,-1,1,-1,1,1,-1,1);
        terrain_cut(1.5, {slope_x:-1}, -1,-1,1,-1,1,1,-1,1);");
    let mut area = 0.;
    map.for_each_triangle(|triangle| {
        let [a, b, c] = triangle;
        area += ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5;
        let p = [0, 1].map(|axis| triangle.iter().map(|p| p[axis]).sum::<f32>() / 3.);
        let z = triangle.iter().map(|p| p[2]).sum::<f32>() / 3.;
        assert!((z - map.height_at(p).unwrap()).abs() < 1e-5, "{p:?}: {z}");
    });
    assert!((area - 16.).abs() < 1e-4);
}

#[test]
fn malformed_footprints_are_rejected_and_clockwise_is_supported() {
    for source in [
        "terrain_cut(0,0,0,1,0);",
        "terrain_cut(0,0,0,1,0,0,0);",
        "terrain_cut(0,0,0,1,0,0.3,0.2,1,1,0,1);",
        "terrain_cut(0,{other:1},0,0,1,0,1,1,0,1);",
        "terrain_cut(0,{slope_x:1,slope_x:2},0,0,1,0,1,1,0,1);",
    ] {
        let source = format!("elevation_map(\"a\",-2,-2,4,4,-1,1);{source}");
        assert!(
            crate::parse(&source)
                .map_err(|e| e.to_string())
                .and_then(|p| ElevationMapSpec::from_program(&p))
                .is_err(),
            "{source}"
        );
    }
    let map = map("terrain_cut(-0.2,-1,-1,-1,1,1,1,1,-1);");
    assert_eq!(map.height_at([0., 0.]), Some(-0.2));
}
