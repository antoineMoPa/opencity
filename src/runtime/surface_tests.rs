use super::*;

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
