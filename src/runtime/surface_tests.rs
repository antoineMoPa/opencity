use super::*;

#[test]
fn raised_sidewalks_have_no_painted_edge() {
    let program =
        crate::parse("road street: sidewalk carlane carlane sidewalk; end street(0,0,1,0);")
            .unwrap();
    let city = City::from_program(&program, None).unwrap();
    let markings: Vec<_> = city
        .triangles
        .iter()
        .filter(|t| t.kind == SurfaceKind::Marking)
        .collect();
    assert!(
        !markings.is_empty(),
        "the lane divider must still be painted"
    );
    // On this symmetric eastbound road, only the center divider is painted.
    assert!(markings
        .iter()
        .flat_map(|t| t.points)
        .all(|p| p[1].abs() <= 0.00051));
}

#[test]
fn support_index_preserves_legacy_queries_across_terrain_decks_and_facades() {
    let program = crate::parse(include_str!("../../tests/fixtures/runtime_support.road")).unwrap();
    let spec = ElevationMapSpec::from_program(&program).unwrap().unwrap();
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 1024, 1024);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Sixteen);
        let samples: Vec<_> = (0..1024 * 1024u32)
            .flat_map(|i| (10000 + (i % 1024) as u16 * 30).to_be_bytes())
            .collect();
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&samples)
            .unwrap();
    }
    let terrain = ElevationMap::from_png(spec, &png).unwrap();
    let mut city = City::from_program_with_elevation_loader(&program, |_| Ok(terrain)).unwrap();
    let render_entries: usize = city.cells.values().map(Vec::len).sum();
    let support_entries: usize = city.support_cells.values().map(Vec::len).sum();
    assert!(
        support_entries * 2 < render_entries,
        "exclude dense facade geometry"
    );
    let signature = |sample: Option<SurfaceSample>| {
        sample.map(|s| {
            (
                s.height.to_bits(),
                s.gradient.map(f32::to_bits),
                s.kind,
                s.road,
            )
        })
    };
    let probes: Vec<_> = (-40..=40)
        .flat_map(|x| (-40..=40).map(move |y| [x as f32 * 0.05, y as f32 * 0.05]))
        .flat_map(|point| {
            [-0.1, 0., 0.02, 0.08, 0.3, 0.5, f32::INFINITY].map(|ceiling| (point, ceiling))
        })
        .chain([
            ([f32::NAN, 0.], 1.),
            ([0., f32::INFINITY], 1.),
            ([0., 0.], f32::NAN),
        ])
        .collect();
    let indexed: Vec<_> = probes
        .iter()
        .map(|&(point, ceiling)| signature(city.surface_at(point, ceiling)))
        .collect();
    // Reuse the exact pre-optimization candidate lists, including render-only
    // surfaces. Compare actual support results rather than duplicating the index.
    city.support_cells = city.cells.clone();
    for ((point, ceiling), expected) in probes.into_iter().zip(indexed) {
        assert_eq!(
            signature(city.surface_at(point, ceiling)),
            expected,
            "legacy support at {point:?} below {ceiling}"
        );
    }
}

#[test]
fn shore_profile_follows_the_bed_on_both_faces_and_excludes_dry_cuts() {
    let program = crate::parse(
        "elevation_map(\"ramp\",-2,-2,4,4,-0.1,0.1); water_level(0.01);
         terrain_cut(-0.2,-0.73,-0.31,0.61,-0.31,0.61,0.47,-0.73,0.47);",
    )
    .unwrap();
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1024, 1024);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Sixteen);
        let samples: Vec<_> = (0..1024 * 1024)
            .flat_map(|i| ((i % 1024 * 37 + i / 1024 * 23) as u16).to_be_bytes())
            .collect();
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&samples)
            .unwrap();
    }
    let city = City::from_program_with_elevation_loader(&program, |spec| {
        ElevationMap::from_png(spec.clone(), &bytes)
    })
    .unwrap();
    let elevation = city.elevation.as_ref().unwrap();
    let mut seen = [false; 3];
    for t in city
        .mesh(Region::WholeMap, 0.32)
        .unwrap()
        .vertices
        .chunks_exact(3)
    {
        let center = [0, 1].map(|i| t.iter().map(|v| v.position[i]).sum::<f32>() / 3.);
        let kind = t[0].surface[0];
        if kind != SurfaceKind::Water as u32 as f32 && kind != SurfaceKind::Grass as u32 as f32 {
            continue;
        }
        if elevation.cut_at(center).is_some() {
            assert_eq!(kind, SurfaceKind::Grass as u32 as f32);
            assert_eq!(t[0].surface[3], 0.);
            seen[0] = true;
            continue;
        }
        for v in t {
            let [x, y, z] = v.position;
            let bed = elevation.original_height_at([x, y]).unwrap();
            assert_eq!(v.surface[3], Vertex::SHORE_PROFILE);
            assert!((v.surface[1] - (bed - 0.01)).abs() < 1e-5);
            assert!((v.surface[2] - 23_f32.atan2(37.)).abs() < 1e-4);
            if kind == SurfaceKind::Water as u32 as f32 {
                assert!((z - 0.01).abs() < 1e-6);
                seen[1] = true;
            } else {
                assert!((z - bed).abs() < 1e-5);
                seen[2] = true;
            }
        }
    }
    assert_eq!(seen, [true; 3]);
}
