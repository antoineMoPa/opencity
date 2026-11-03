use super::*;
fn fixture() -> Value {
    let controls: Vec<_> = [[0., 0.], [0.01, 0.], [0., 0.01], [0.01, 0.01]]
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let m = mercator(p).unwrap();
            let y = -m[1] * 0.02 + 3.;
            let x = m[0] * 0.01 + m[1] * 0.003 + 2. + 0.4 * (1. - y).max(0.);
            json!({"id":i.to_string(),"lonlat":p,"world":[x,y]})
        })
        .collect();
    json!({
        "schema":"opencity-georeference-v1",
        "projection":"local_web_mercator",
        "origin_lonlat":[0.,
        0.],
        "world_bounds":[-100.,
        -100.,
        100.,
        100.],
        "north_x_adjustment":[1.,
        0.4],
        "fit_controls":controls
    })
}
#[test]
fn fits_rotation_shear_and_round_trips_both_sides_of_correction() {
    let m = Mapping::load(fixture()).unwrap();
    for p in [[0.003, -0.004], [0.003, 0.004], [0.01, 0.01]] {
        let w = m.to_world(p).unwrap();
        let r = m.to_lonlat(w).unwrap();
        assert!((r[0] - p[0]).abs() < 1e-10 && (r[1] - p[1]).abs() < 1e-10);
    }
    assert!(
        m.errors("fit_controls").unwrap()["max_world"]
            .as_f64()
            .unwrap()
            < 1e-8
    );
}
#[test]
fn rejects_degenerate_controls_and_invalid_geography() {
    let mut s = fixture();
    s["fit_controls"][2]["lonlat"] = json!([0.02, 0.]);
    s["fit_controls"][3]["lonlat"] = json!([0.03, 0.]);
    assert!(Mapping::load(s).is_err());
    assert!(mercator([0., 90.]).is_err());
    assert!(mercator([181., 0.]).is_err());
    assert!(mercator([f64::NAN, 0.]).is_err());
}
#[test]
fn preserves_source_ids_vertices_and_gaps_in_bulk_output() {
    let m = Mapping::load(fixture()).unwrap();
    let source = json!({
        "type":"FeatureCollection",
        "features":[{
            "type":"Feature",
            "properties":{
                "osm_id":8,
                "source_node_ids":[1,
                null,
                3]
            },
            "geometry":{
                "type":"LineString",
                "coordinates":[[0.,
                0.],
                [0.002,
                0.],
                [0.004,
                0.]]
            }
        },
        {
            "type":"Feature",
            "properties":{
                "osm_id":8
            },
            "geometry":{
                "type":"Point",
                "coordinates":[0.006,
                0.]
            }
        }]
    });
    let out = m.map_geojson(source.clone()).unwrap();
    assert_eq!(out["source_geojson"], source);
    assert_eq!(out["mapped_features"].as_array().unwrap().len(), 2);
    assert_eq!(out["vertex_count"], 4);
    assert_eq!(
        out["mapped_features"][0]["world_geometry"]["coordinates"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
