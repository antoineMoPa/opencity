#![cfg(feature = "game-runtime")]
use opencity::{
    parse, project_landscape_features,
    runtime::{architecture::BuildingRecipe, City, Region, SurfaceKind},
    BuildingFamily as Family, BuildingPalette, DowntownField,
};

fn feature(family: &str, points: &str) -> opencity::LandscapeFeature {
    let source = format!(
        "tower=building(0.42,{{material:glass,family:{family},floor_height:0.0125,\
        downtown_index:0.8,podium_floors:2,tower_ratio:0.48,seed:42}}, {points});"
    );
    let p = project_landscape_features(&parse(&source).unwrap());
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    p.features[0].clone()
}
#[test]
fn all_families_fit_the_authored_lot_and_height_in_both_windings() {
    for family in Family::ALL {
        for points in ["0,0, 0.3,0, 0.3,0.2, 0,0.2", "0,0, 0,0.2, 0.3,0.2, 0.3,0"] {
            let f = feature(family.keyword(), points);
            let recipe = BuildingRecipe::from_feature(&f).unwrap();
            let mesh = recipe.mesh();
            assert!(mesh.vertices.len() > 200);
            assert_eq!(mesh.vertices.len() % 3, 0);
            assert_eq!(recipe.family, family);
            for v in &mesh.vertices {
                assert!(v
                    .position
                    .iter()
                    .chain(v.normal.iter())
                    .chain(v.surface.iter())
                    .all(|n| n.is_finite()));
                assert!(
                    (-1e-6..=0.300001).contains(&v.position[0]),
                    "{family:?} {v:?}"
                );
                assert!(
                    (-1e-6..=0.200001).contains(&v.position[1]),
                    "{family:?} {v:?}"
                );
                assert!(
                    (-1e-6..=0.420001).contains(&v.position[2]),
                    "{family:?} {v:?}"
                );
            }
            for t in mesh.vertices.chunks_exact(3) {
                let edge = |i: usize| {
                    std::array::from_fn::<_, 3, _>(|j| t[i].position[j] - t[0].position[j])
                };
                let a = edge(1);
                let b = edge(2);
                let cross = [
                    a[1] * b[2] - a[2] * b[1],
                    a[2] * b[0] - a[0] * b[2],
                    a[0] * b[1] - a[1] * b[0],
                ];
                assert!(
                    cross
                        .iter()
                        .zip(t[0].normal)
                        .map(|(a, b)| a * b)
                        .sum::<f32>()
                        > 0.,
                    "winding and explicit normals must agree: {family:?}"
                );
            }
        }
    }
}
#[test]
fn runtime_keeps_facade_uvs_and_replaces_the_old_shell() {
    let source = "tower=building(0.4,{material:glass,family:podium,floor_height:0.0125},\
        0,0,0.3,0,0.3,0.2,0,0.2);";
    let city = City::from_program(&parse(source).unwrap(), None).unwrap();
    let mesh = city.mesh(Region::WholeMap, 0.05).unwrap();
    let authored = project_landscape_features(&parse(source).unwrap());
    let direct = BuildingRecipe::from_feature(&authored.features[0])
        .unwrap()
        .mesh();
    assert_eq!(mesh.vertices.len(), direct.vertices.len());
    for (streamed, direct) in mesh.vertices.iter().zip(&direct.vertices) {
        assert_eq!(streamed.position, direct.position);
        assert_eq!(streamed.normal, direct.normal);
        assert_eq!(streamed.color, direct.color);
        assert_eq!(streamed.surface, direct.surface);
    }
    assert!(mesh
        .vertices
        .iter()
        .any(|v| v.surface[0] == SurfaceKind::Facade as u32 as f32));
    assert!(!mesh
        .vertices
        .iter()
        .any(|v| v.surface[0] == SurfaceKind::Building as u32 as f32));
    assert!(mesh
        .vertices
        .chunks_exact(3)
        .any(|t| t[0].surface[1..3] != t[1].surface[1..3]));
    assert!(city.building_collision([0.15, 0.1], 0.001, 0., 0.03));
    assert!(city.building_collision([0.01, 0.01], 0.001, 0., 0.01));
    assert!(
        !city.building_collision([0.01, 0.01], 0.001, 0.10, 0.20),
        "empty space above the podium must not retain the old full-footprint collision"
    );
}
#[test]
fn legacy_models_and_non_rectangular_lots_keep_their_existing_path() {
    let triangle = feature("auto", "0,0,0.3,0,0.1,0.2,0.05,0.1");
    assert!(BuildingRecipe::from_feature(&triangle).is_none());
    let mut f = feature("auto", "0,0,0.3,0,0.3,0.2,0,0.2");
    assert!(BuildingRecipe::from_feature(&f).is_some());
    f.model = Some("model.glb".into());
    assert!(BuildingRecipe::from_feature(&f).is_none());
}
#[test]
fn recipe_options_validate_and_round_trip_without_losing_metadata() {
    let f = feature("civic", "0,0,0.3,0,0.3,0.2,0,0.2");
    let options = f.building_style.unwrap().architecture.options();
    assert!(options.iter().any(|(key, _)| key == "downtown_index"));
    for invalid in [
        "family:unknown",
        "palette:unknown",
        "glazing:unknown",
        "weathering:2",
        "downtown_index:-0.1",
        "tower_ratio:0.1",
        "podium_floors:3",
        "seed:-1",
        "section:unknown",
        "ground:unknown",
        "setback:yes",
        "corners:unknown",
        "roof_units:9",
        "roof_units:1.5",
    ] {
        let source = format!("building(0.2,{{{invalid}}},0,0,1,0,1,1);");
        assert!(
            !project_landscape_features(&parse(&source).unwrap())
                .errors
                .is_empty(),
            "{invalid}"
        );
    }
}
#[test]
fn downtown_field_is_coherent_and_biases_a_ten_thousand_lot_population() {
    let field = DowntownField {
        center: [0., 0.],
        reach: 20.,
        noise: 0.18,
        seed: 42,
    };
    let mut core = (0., 0, 0);
    let mut outer = (0., 0, 0);
    let mut palettes = std::collections::HashSet::new();
    let mut families = [0; 6];
    for row in 0..100 {
        for column in 0..100 {
            let p = [(column as f32 - 49.5) * 0.56, (row as f32 - 49.5) * 0.56];
            let index = field.index(p);
            let seed = (row * 100 + column) as u64;
            assert!((0.0..=1.0).contains(&index));
            assert!((index - field.index([p[0] + 0.01, p[1]])).abs() < 0.01);
            let family = Family::for_index(index, seed);
            families[family as usize] += 1;
            palettes.insert(BuildingPalette::seeded(family, seed) as u32);
            let record = if index > 0.72 {
                &mut core
            } else if index < 0.18 {
                &mut outer
            } else {
                continue;
            };
            record.0 += f32::from(family.floors_at(index, seed, 32));
            record.1 += 1;
            if matches!(family, Family::CurtainWall | Family::Podium) {
                record.2 += 1;
            }
        }
    }
    assert!(families.into_iter().all(|n| n > 100));
    assert_eq!(palettes.len(), 9);
    assert!(core.0 / core.1 as f32 > outer.0 / outer.1 as f32 * 2.);
    assert!(core.2 as f32 / core.1 as f32 > outer.2 as f32 / outer.1 as f32 * 2.);
    let radial = DowntownField { noise: 0., ..field };
    assert!((radial.index([10., 0.]) - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    assert!(radial.index([40., 0.]) < 1e-6);
}

#[test]
fn default_map_shows_all_families_and_materials_in_each_town() {
    let p = parse(include_str!("../examples/default_map.road")).unwrap();
    let features = project_landscape_features(&p).features;
    let mut total = 0;
    for town in ["west", "east"] {
        let mut counts = [0; 6];
        let mut palettes = std::collections::HashSet::new();
        for f in features
            .iter()
            .filter(|f| f.name.as_ref().is_some_and(|n| n.starts_with(town)))
        {
            if f.building_style.unwrap_or_default().material != opencity::BuildingMaterial::Glass {
                continue;
            }
            let recipe =
                BuildingRecipe::from_feature(f).expect("every stock glass lot is replaced");
            counts[recipe.family as usize] += 1;
            palettes.insert(recipe.palette as u32);
            total += 1;
            assert!(f
                .building_style
                .unwrap()
                .architecture
                .downtown_index
                .is_some());
        }
        assert!(counts.iter().all(|n| *n > 0), "{town}: {counts:?}");
        assert_eq!(palettes.len(), 9, "{town}: full stone/concrete palette");
    }
    assert_eq!(total, 786);
}

#[test]
fn baked_occlusion_is_identical_after_rotation_translation_and_reflection() {
    let f = feature("heritage", "0,0,0.3,0,0.3,0.2,0,0.2");
    let recipe = BuildingRecipe::from_feature(&f).unwrap();
    let reference = recipe.mesh();
    for reflected in [false, true] {
        let (s, c) = 0.73_f32.sin_cos();
        let transformed = BuildingRecipe {
            origin: [500., -600., 2.],
            along: [c, s],
            inward: if reflected { [s, -c] } else { [-s, c] },
            ..recipe
        }
        .mesh();
        assert_eq!(reference.vertices.len(), transformed.vertices.len());
        for (index, v) in transformed.vertices.iter().enumerate() {
            let source = if reflected {
                index / 3 * 3
                    + match index % 3 {
                        1 => 2,
                        2 => 1,
                        _ => 0,
                    }
            } else {
                index
            };
            assert_eq!(v.color, reference.vertices[source].color);
        }
    }
}
